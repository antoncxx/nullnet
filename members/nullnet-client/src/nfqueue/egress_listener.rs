//! Egress-trigger NFQUEUE listener (queue 1).
//!
//! The kernel queues the first packet of each NEW flow to a non-internal
//! destination (see `commands::egress`). For packets whose source maps to a
//! registered service container, we fire `egress_trigger` so the server builds
//! the per-initiator egress edge to the proxy, then ACCEPT immediately.
//!
//! Unlike the backend-trigger listener we do NOT hold the packet: once steering
//! is installed, subsequent packets of the flow are policy-routed into the
//! tunnel; the very first SYN may hit the still-default route (dropped by the
//! host firewall) and is simply retransmitted a moment later. This keeps the
//! egress path stateless and avoids the DNAT-ordering dance backend triggers
//! need. Each container is triggered once per process lifetime (edges are torn
//! down only on node disconnect, which restarts the client and clears this set).

use crate::nfqueue::cache::BridgeIpCache;
use crate::nfqueue::parse::ipv4_flow;
use nfq::{Queue, Verdict};
use nullnet_grpc_lib::NullnetGrpcInterface;
use nullnet_grpc_lib::nullnet_grpc::{
    AgentEgressTriggerSendFailed, AgentEvent, agent_event::Event as AgentEventKind,
};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const QUEUE_ID: u16 = 1;
const COPY_RANGE: u16 = 128;
const QUEUE_MAX_LEN: u32 = 4096;
const IDLE_SLEEP: Duration = Duration::from_millis(1);

/// Spawn the egress recv loop on a dedicated OS thread. Verdicts are issued
/// synchronously (always ACCEPT); the gRPC trigger is fired fire-and-forget on
/// the tokio runtime so recv never stalls.
pub fn spawn_egress_recv_thread(grpc: NullnetGrpcInterface, cache: BridgeIpCache) {
    let handle = tokio::runtime::Handle::current();
    let triggered: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
    std::thread::spawn(move || {
        let mut queue = match Queue::open() {
            Ok(q) => q,
            Err(e) => {
                eprintln!("[egress-nfq] open queue failed: {e} (need CAP_NET_ADMIN)");
                return;
            }
        };
        if let Err(e) = queue.bind(QUEUE_ID) {
            eprintln!("[egress-nfq] bind queue {QUEUE_ID} failed: {e}");
            return;
        }
        if let Err(e) = queue.set_copy_range(QUEUE_ID, COPY_RANGE) {
            eprintln!("[egress-nfq] set_copy_range: {e}");
        }
        if let Err(e) = queue.set_queue_max_len(QUEUE_ID, QUEUE_MAX_LEN) {
            eprintln!("[egress-nfq] set_queue_max_len: {e}");
        }
        queue.set_nonblocking(true);
        println!("[egress-nfq] recv loop running on queue {QUEUE_ID}");

        loop {
            match queue.recv() {
                Ok(mut msg) => {
                    if let Some((src_ip, dst_ip, dst_port)) = ipv4_flow(msg.get_payload())
                        && let Some(container) = cache.get(src_ip)
                    {
                        // Fire once per container. Insert first; roll back on error.
                        let first = triggered.lock().unwrap().insert(container.clone());
                        if first {
                            let grpc = grpc.clone();
                            let triggered = triggered.clone();
                            let dst_s = dst_ip.to_string();
                            handle.spawn(async move {
                                let res = grpc
                                    .egress_trigger(
                                        String::new(),
                                        container.clone(),
                                        dst_s.clone(),
                                        u32::from(dst_port),
                                    )
                                    .await;
                                if let Err(e) = res {
                                    eprintln!("[egress-nfq] egress_trigger {container}: {e}");
                                    // Report so the failed egress request is observable,
                                    // then roll back so a later packet retries.
                                    let event = AgentEvent {
                                        event: Some(AgentEventKind::EgressTriggerSendFailed(
                                            AgentEgressTriggerSendFailed {
                                                service_name: container.clone(),
                                                dst_ip: dst_s,
                                                dst_port: u32::from(dst_port),
                                                error_message: e.to_string(),
                                            },
                                        )),
                                    };
                                    let _ = grpc.report_event(event).await;
                                    triggered.lock().unwrap().remove(&container);
                                } else {
                                    println!("[egress-nfq] egress edge requested for {container}");
                                }
                            });
                        }
                    }
                    // Always accept — steering (once up) handles routing; a lost
                    // first SYN is retransmitted.
                    msg.set_verdict(Verdict::Accept);
                    if let Err(e) = queue.verdict(msg) {
                        eprintln!("[egress-nfq] verdict failed: {e}");
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(IDLE_SLEEP);
                }
                Err(e) => {
                    eprintln!("[egress-nfq] recv error: {e}");
                    std::thread::sleep(IDLE_SLEEP);
                }
            }
        }
    });
}
