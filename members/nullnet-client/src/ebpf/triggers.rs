use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a `Pending` entry survives before it can be retriggered.
/// Bounds the time we wait for a server VxlanSetup that may never arrive
/// (e.g., server returned ok without dispatching a setup, or the message
/// was lost).
const PENDING_TIMEOUT: Duration = Duration::from_secs(10);

/// Per-trigger-port lifecycle:
/// - `Pending`: backend_trigger fired, waiting for the server to set up the chain.
/// - `Active`: VXLAN is up and DNAT is installed. Stores the bookkeeping
///   needed to tear DNAT down when the matching VxlanTeardown arrives.
pub enum Lifecycle {
    Pending { since: Instant },
    Active { vxlan_id: u32, overlay_ip: Ipv4Addr },
}

#[derive(Default)]
pub struct TriggersState {
    by_port: Mutex<HashMap<u16, Lifecycle>>,
}

impl TriggersState {
    /// Returns true if the caller should fire `backend_trigger` for this port;
    /// false if a trigger is already pending (within timeout) or active.
    pub fn try_mark_pending(&self, port: u16) -> bool {
        let mut by_port = self.by_port.lock().unwrap();
        match by_port.get(&port) {
            Some(Lifecycle::Active { .. }) => return false,
            Some(Lifecycle::Pending { since }) if since.elapsed() < PENDING_TIMEOUT => {
                return false;
            }
            _ => {}
        }
        by_port.insert(
            port,
            Lifecycle::Pending {
                since: Instant::now(),
            },
        );
        true
    }

    pub fn mark_active(&self, port: u16, vxlan_id: u32, overlay_ip: Ipv4Addr) {
        self.by_port.lock().unwrap().insert(
            port,
            Lifecycle::Active {
                vxlan_id,
                overlay_ip,
            },
        );
    }

    /// Drop the entry so the next observed packet on this port retriggers.
    pub fn forget(&self, port: u16) {
        self.by_port.lock().unwrap().remove(&port);
    }

    /// Find the Active entry for `vxlan_id` and remove it. Returns the
    /// `(port, overlay_ip)` so the caller can tear DNAT down.
    pub fn remove_by_vxlan(&self, vxlan_id: u32) -> Option<(u16, Ipv4Addr)> {
        let mut by_port = self.by_port.lock().unwrap();
        let port = by_port.iter().find_map(|(p, lc)| match lc {
            Lifecycle::Active { vxlan_id: v, .. } if *v == vxlan_id => Some(*p),
            _ => None,
        })?;
        match by_port.remove(&port)? {
            Lifecycle::Active { overlay_ip, .. } => Some((port, overlay_ip)),
            Lifecycle::Pending { .. } => None,
        }
    }
}
