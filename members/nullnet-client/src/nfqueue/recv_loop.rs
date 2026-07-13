//! Shared NFQUEUE deferred-verdict recv loop.
//!
//! Both the backend-trigger (queue 0) and egress-trigger (queue 1) listeners
//! run the same plumbing: a dedicated OS thread owns the netfilter queue, pulls
//! packets in non-blocking mode, and hands each to a tokio task. The task issues
//! the verdict asynchronously (after slow gRPC / hold-until-ready work) and sends
//! the `Message` back through a channel; the recv thread drains those verdicts in
//! lockstep. Deferring the verdict is what *holds* the packet in the kernel until
//! the datapath (DNAT / egress steer) is ready, so the original packet isn't lost.

use nfq::{Message, Queue};
use std::future::Future;
use std::sync::mpsc::{Sender, TryRecvError};
use std::time::Duration;

/// Idle sleep between recv polls when there's nothing to do.
const IDLE_SLEEP: Duration = Duration::from_millis(1);

/// Open NFQUEUE `queue_id` and run the recv loop on a dedicated OS thread,
/// dispatching each packet to `handler`. The handler owns the `Message` and must
/// send it back (after `set_verdict`) through the provided `Sender` — that is
/// what releases the held packet into the netfilter pipeline. Handlers run as
/// tokio tasks so slow async work never stalls recv.
///
/// Failure to open/bind the queue is logged and the thread exits; the rest of the
/// client keeps running. With `--queue-bypass` on the iptables rule, the absence
/// of a consumer fail-opens, so traffic flows unaltered.
pub fn spawn_queue_loop<H, Fut>(queue_id: u16, copy_range: u16, queue_max_len: u32, handler: H)
where
    H: Fn(Message, Sender<Message>) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let runtime = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let mut queue = match Queue::open() {
            Ok(q) => q,
            Err(e) => {
                eprintln!("[nfqueue] open queue {queue_id} failed: {e} (need CAP_NET_ADMIN)");
                return;
            }
        };
        if let Err(e) = queue.bind(queue_id) {
            eprintln!("[nfqueue] bind queue {queue_id} failed: {e}");
            return;
        }
        if let Err(e) = queue.set_copy_range(queue_id, copy_range) {
            eprintln!("[nfqueue] set_copy_range queue {queue_id}: {e}");
        }
        if let Err(e) = queue.set_queue_max_len(queue_id, queue_max_len) {
            eprintln!("[nfqueue] set_queue_max_len queue {queue_id}: {e}");
        }
        // Fail-closed on overload: drops become visible instead of silently
        // letting traffic through. `--queue-bypass` on the iptables rule
        // separately covers the no-consumer case (this thread crashed/exited).
        if let Err(e) = queue.set_fail_open(queue_id, false) {
            eprintln!("[nfqueue] set_fail_open queue {queue_id}: {e}");
        }
        queue.set_nonblocking(true);

        let (verdict_tx, verdict_rx) = std::sync::mpsc::channel::<Message>();
        println!("[nfqueue] recv loop running on queue {queue_id}");

        loop {
            let mut did_work = false;
            // Drain any verdicts that came back from per-packet handlers.
            loop {
                match verdict_rx.try_recv() {
                    Ok(msg) => {
                        did_work = true;
                        if let Err(e) = queue.verdict(msg) {
                            eprintln!("[nfqueue] verdict failed on queue {queue_id}: {e}");
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
            // Drain packets currently waiting in the kernel queue.
            loop {
                match queue.recv() {
                    Ok(msg) => {
                        did_work = true;
                        runtime.spawn(handler(msg, verdict_tx.clone()));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => {
                        eprintln!("[nfqueue] recv error on queue {queue_id}: {e}");
                        break;
                    }
                }
            }
            if !did_work {
                std::thread::sleep(IDLE_SLEEP);
            }
        }
    });
}
