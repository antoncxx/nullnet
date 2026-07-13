//! Egress-trigger NFQUEUE listener (queue 1) — hold-until-steered.
//!
//! The kernel queues the first packet of each NEW flow to a non-internal
//! destination (see `commands::egress`). For a packet whose source maps to a
//! registered service container we fire `egress_trigger` so the server builds the
//! per-initiator egress edge to the gateway, and — unlike the old accept-and-retry
//! model — we **hold** the packet (defer the verdict) until steering is installed.
//! `control_channel` calls `TriggersState::mark_active` right after
//! `egress::install_steer` succeeds; that wakes the held handler, which verdicts
//! ACCEPT. The released packet then hits the freshly-installed `ip rule` and is
//! policy-routed into the tunnel, so the original SYN is not dropped.
//!
//! This reuses the backend-trigger machinery wholesale: the shared
//! `recv_loop::spawn_queue_loop` plumbing and the `TriggersState` lifecycle,
//! keyed by the initiator container + `EGRESS_TRIGGER_PORT` (egress is
//! once-per-container, so a single sentinel-port entry covers all its flows).

use crate::nfqueue::cache::BridgeIpCache;
use crate::nfqueue::parse::ipv4_flow;
use crate::nfqueue::recv_loop::spawn_queue_loop;
use crate::triggers::{EGRESS_TRIGGER_PORT, TriggerState, TriggersState};
use nfq::{Message, Verdict};
use nullnet_grpc_lib::NullnetGrpcInterface;
use nullnet_grpc_lib::nullnet_grpc::{
    AgentEgressTriggerSendFailed, AgentEvent, agent_event::Event as AgentEventKind,
};
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::Duration;
use tokio::sync::{Notify, Semaphore};
use tokio::time::timeout;

const QUEUE_ID: u16 = 1;
const COPY_RANGE: u16 = 128;
const QUEUE_MAX_LEN: u32 = 4096;
/// How long the handler waits for `egress_trigger` to return.
const TRIGGER_TIMEOUT: Duration = Duration::from_secs(5);
/// How long the handler holds the packet waiting for steering to be installed
/// (`mark_active`) before giving up.
const STEER_TIMEOUT: Duration = Duration::from_secs(5);

/// State shared by every egress per-packet handler. Cloned freely across tasks.
#[derive(Clone)]
struct EgressCtx {
    grpc: NullnetGrpcInterface,
    cache: BridgeIpCache,
    triggers_state: Arc<TriggersState>,
    semaphore: Arc<Semaphore>,
}

/// Spawn the egress-trigger recv loop (queue 1). Shares the bridge-IP cache, the
/// gRPC handle, and the `TriggersState` used by the backend listener.
pub fn spawn_egress_recv_thread(
    grpc: NullnetGrpcInterface,
    cache: BridgeIpCache,
    triggers_state: Arc<TriggersState>,
) {
    let ctx = EgressCtx {
        grpc,
        cache,
        triggers_state,
        semaphore: Arc::new(Semaphore::new(
            crate::nfqueue::listener::HANDLER_CONCURRENCY,
        )),
    };
    spawn_queue_loop(
        QUEUE_ID,
        COPY_RANGE,
        QUEUE_MAX_LEN,
        move |msg, verdict_tx| {
            let ctx = ctx.clone();
            handle_packet(msg, ctx, verdict_tx)
        },
    );
}

async fn handle_packet(mut msg: Message, ctx: EgressCtx, verdict_tx: Sender<Message>) {
    // Backpressure: cap concurrent in-flight handlers (mirrors the backend
    // listener). A held egress packet can occupy a task for up to
    // TRIGGER_TIMEOUT + STEER_TIMEOUT, so without this a container spraying new
    // flows would fan out unbounded tasks and trigger RPCs.
    let _permit = match ctx.semaphore.clone().acquire_owned().await {
        Ok(p) => p,
        Err(_) => {
            msg.set_verdict(Verdict::Drop);
            let _ = verdict_tx.send(msg);
            return;
        }
    };
    // Parse before the async work so no borrow of `msg` is held across an await.
    let flow = ipv4_flow(msg.get_payload());
    let verdict = decide_verdict(&ctx, flow).await;
    msg.set_verdict(verdict);
    let _ = verdict_tx.send(msg);
}

async fn decide_verdict(ctx: &EgressCtx, flow: Option<(Ipv4Addr, Ipv4Addr, u16)>) -> Verdict {
    let Some((src_ip, dst_ip, dst_port)) = flow else {
        return Verdict::Accept;
    };
    // Only brokered initiators are held; anything we can't map to a registered
    // container (host process, unmapped source) passes through unaltered.
    let Some(container) = ctx.cache.get(src_ip) else {
        return Verdict::Accept;
    };

    match ctx.triggers_state.state(&container, EGRESS_TRIGGER_PORT) {
        TriggerState::Active => Verdict::Accept,
        TriggerState::Pending(notify) => wait_for_steer(ctx, &container, notify).await,
        TriggerState::Fresh => {
            let notify = ctx
                .triggers_state
                .mark_pending(&container, EGRESS_TRIGGER_PORT, src_ip);
            // Register the waiter BEFORE the gRPC round-trip: the server can
            // dispatch the egress `VxlanSetup` (→ `mark_active`) faster than its
            // reply to `egress_trigger` returns, and `notify_waiters` only wakes
            // already-registered futures (see the backend listener for the race).
            let notified = notify.notified();
            tokio::pin!(notified);
            if notified.as_mut().enable()
                || matches!(
                    ctx.triggers_state.state(&container, EGRESS_TRIGGER_PORT),
                    TriggerState::Active
                )
            {
                return Verdict::Accept;
            }
            let res = timeout(
                TRIGGER_TIMEOUT,
                ctx.grpc.egress_trigger(
                    String::new(),
                    container.clone(),
                    dst_ip.to_string(),
                    u32::from(dst_port),
                ),
            )
            .await;
            match res {
                Ok(Ok(())) => match timeout(STEER_TIMEOUT, notified).await {
                    Ok(_) => Verdict::Accept,
                    Err(_) => {
                        // Trigger accepted but steering is slow. Leave the entry
                        // Pending (ages out at PENDING_TIMEOUT) so a retransmit
                        // re-waits on the same notify; drop this held SYN.
                        eprintln!("[egress-nfq] no egress steer for container {container}");
                        Verdict::Drop
                    }
                },
                Ok(Err(e)) => {
                    eprintln!("[egress-nfq] egress_trigger {container}: {e}");
                    report_trigger_send_failed(&ctx.grpc, &container, dst_ip, dst_port, e);
                    ctx.triggers_state.forget(&container, EGRESS_TRIGGER_PORT);
                    Verdict::Drop
                }
                Err(_) => {
                    eprintln!("[egress-nfq] egress_trigger timeout for container {container}");
                    report_trigger_send_failed(
                        &ctx.grpc,
                        &container,
                        dst_ip,
                        dst_port,
                        format!("egress_trigger timed out after {TRIGGER_TIMEOUT:?}"),
                    );
                    ctx.triggers_state.forget(&container, EGRESS_TRIGGER_PORT);
                    Verdict::Drop
                }
            }
        }
    }
}

/// Hold the packet until steering is marked active (or time out and drop it).
async fn wait_for_steer(ctx: &EgressCtx, container: &str, notify: Arc<Notify>) -> Verdict {
    let notified = notify.notified();
    tokio::pin!(notified);
    if notified.as_mut().enable()
        || matches!(
            ctx.triggers_state.state(container, EGRESS_TRIGGER_PORT),
            TriggerState::Active
        )
    {
        return Verdict::Accept;
    }
    match timeout(STEER_TIMEOUT, notified).await {
        Ok(_) => Verdict::Accept,
        Err(_) => {
            eprintln!("[egress-nfq] timeout waiting for egress steer, container {container}");
            Verdict::Drop
        }
    }
}

/// Fire-and-forget: report a failed `egress_trigger` to the server's event stream.
fn report_trigger_send_failed(
    grpc: &NullnetGrpcInterface,
    container: &str,
    dst_ip: Ipv4Addr,
    dst_port: u16,
    error_message: String,
) {
    let grpc = grpc.clone();
    let event = AgentEvent {
        event: Some(AgentEventKind::EgressTriggerSendFailed(
            AgentEgressTriggerSendFailed {
                service_name: container.to_string(),
                dst_ip: dst_ip.to_string(),
                dst_port: u32::from(dst_port),
                error_message,
            },
        )),
    };
    tokio::spawn(async move {
        let _ = grpc.report_event(event).await;
    });
}
