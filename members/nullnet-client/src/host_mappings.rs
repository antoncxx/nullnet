use nullnet_grpc_lib::nullnet_grpc::HostMapping;
use std::collections::HashMap;
use std::sync::Mutex;

/// Tracks the `/etc/hosts` entries installed by setup so the matching
/// teardown can remove them. Teardown messages don't carry the mapping,
/// so we record it locally at setup time and look it up on teardown.
#[derive(Default)]
pub struct HostMappingsState {
    by_vlan: Mutex<HashMap<u16, HostMapping>>,
    by_vxlan: Mutex<HashMap<u32, (HostMapping, Option<String>)>>,
}

impl HostMappingsState {
    pub fn record_vlan(&self, vlan_id: u16, hm: HostMapping) {
        self.by_vlan.lock().unwrap().insert(vlan_id, hm);
    }

    pub fn take_vlan(&self, vlan_id: u16) -> Option<HostMapping> {
        self.by_vlan.lock().unwrap().remove(&vlan_id)
    }

    pub fn record_vxlan(&self, vxlan_id: u32, hm: HostMapping, docker_container: Option<String>) {
        self.by_vxlan
            .lock()
            .unwrap()
            .insert(vxlan_id, (hm, docker_container));
    }

    pub fn take_vxlan(&self, vxlan_id: u32) -> Option<(HostMapping, Option<String>)> {
        self.by_vxlan.lock().unwrap().remove(&vxlan_id)
    }
}
