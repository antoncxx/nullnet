#![allow(clippy::module_name_repetitions)]

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use crate::FORWARD_PORT;
use crate::crypto::TunnelCipher;
use serde::{Deserialize, Serialize};

/// A VLAN tunnel's traffic is either AES-256-GCM encrypted (server's
/// `ENCRYPTION_ENABLED` was on when the tunnel was set up) or forwarded as-is.
/// Distinct from "unknown vlan_id" (`Peers::get_key` returning `None`), which
/// still means drop — this only ever comes from an explicit `VlanSetup`.
#[derive(Clone)]
pub enum VlanCipher {
    Encrypted(Arc<TunnelCipher>),
    Plaintext,
}

/// Struct containing all peers information.
#[derive(Default)]
pub struct Peers {
    /// Mapping from veth addresses to Ethernet IPs.
    ips: HashMap<VethKey, Ipv4Addr>,
    /// Per-tunnel encryption state, keyed by `vlan_id`. Populated from the
    /// `VlanSetup` the server hands out and used by the userspace forwarder
    /// (`forward/send.rs`, `forward/receive.rs`) to encrypt/decrypt (or pass
    /// through) this tunnel's traffic. `TunnelCipher` is `Arc`-wrapped so
    /// callers can clone a handle out without holding the `Peers` lock across
    /// the actual crypto work.
    keys: HashMap<u16, VlanCipher>,
}

impl Peers {
    pub fn get_socket_by_veth(&self, veth_key: VethKey) -> Option<SocketAddr> {
        self.ips
            .get(&veth_key)
            .map(|ip| SocketAddr::new(IpAddr::V4(*ip), FORWARD_PORT))
    }

    pub fn insert(&mut self, veth_key: VethKey, eth_ip: Ipv4Addr) {
        self.ips.insert(veth_key, eth_ip);
    }

    pub fn insert_key(&mut self, vlan_id: u16, key: &[u8; 32]) {
        self.keys
            .insert(vlan_id, VlanCipher::Encrypted(Arc::new(TunnelCipher::new(key))));
    }

    /// Register `vlan_id` as deliberately unencrypted (server's
    /// `ENCRYPTION_ENABLED` was off for this tunnel).
    pub fn insert_plaintext(&mut self, vlan_id: u16) {
        self.keys.insert(vlan_id, VlanCipher::Plaintext);
    }

    pub fn get_key(&self, vlan_id: u16) -> Option<VlanCipher> {
        self.keys.get(&vlan_id).cloned()
    }

    pub fn remove(&mut self, vlan_id: u16) {
        self.ips.retain(|key, _| key.vlan_id != vlan_id);
        self.keys.remove(&vlan_id);
    }
}

/// Struct identifying veth on a VLAN.
#[derive(Eq, Hash, PartialEq, Clone, Copy, Serialize, Deserialize, Debug)]
#[serde(rename = "veth")]
pub struct VethKey {
    /// IP address of the veth.
    veth_ip: Ipv4Addr,
    /// VLAN ID of the veth.
    vlan_id: u16,
}

impl VethKey {
    pub fn new(veth_ip: Ipv4Addr, vlan_id: u16) -> Self {
        Self { veth_ip, vlan_id }
    }
}
