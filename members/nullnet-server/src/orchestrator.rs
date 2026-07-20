use crate::env::NET_TYPE;
use crate::events::{Event, EventStore};
use crate::geo::{GeoCache, GeoInfo};
use crate::net::{EgressRole, NetExt};
use crate::net_id_pool::NetIdPool;
use crate::services::changes::{apply_changes, detect_node_disconnect_changes};
use crate::services::input::StackMap;
use nullnet_grpc_lib::nullnet_grpc::{
    ContainerResume, ContainerSuspend, EgressPolicyChanged, MsgId, NetMessage, net_message,
};
use nullnet_liberror::{Error, ErrorHandler, Location, location};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, mpsc, oneshot};
use tonic::{Request, Status, Streaming};
use uuid::Uuid;

type OutboundStream = mpsc::Sender<Result<NetMessage, Status>>;

/// Initiator replica identity keying an egress edge: (node IP, docker container).
/// One edge per initiator replica multiplexes all of its external destinations.
type EgressKey = (IpAddr, Option<String>);

/// Cap on distinct external destinations tracked per egress edge. When full, the
/// least-recently-contacted destination is evicted. Bounds memory for a service
/// that contacts a very large set of hosts (e.g. a crawler).
const MAX_DESTS_PER_EDGE: usize = 256;

/// Per-destination stats on an egress edge, reported by the client (which owns
/// the running count and latest-seen time; the server stores them verbatim).
#[derive(Debug, Clone)]
struct DestStat {
    last_seen: u64,
    count: u64,
    /// Whether the latest attempt was denied by the egress country policy.
    blocked: bool,
}

/// A live egress forward-proxy edge (initiator replica -> proxy host).
#[derive(Debug, Clone)]
struct EgressEdge {
    net_id: u32,
    initiator_ip: IpAddr,
    initiator_docker: Option<String>,
    proxy_ip: IpAddr,
    /// External destinations this edge has carried, keyed by destination IP.
    /// Populated from client destination reports; lives and dies with the edge.
    destinations: HashMap<Ipv4Addr, DestStat>,
}

/// One contacted external destination, for topology rendering.
#[derive(Debug, Clone)]
pub(crate) struct EgressDestination {
    pub(crate) ip: Ipv4Addr,
    pub(crate) last_seen: u64,
    pub(crate) count: u64,
    /// Whether the latest attempt was denied by the egress country policy.
    pub(crate) blocked: bool,
    /// Geo/ASN enrichment, if the lookup has resolved yet (else `None`).
    pub(crate) geo: Option<GeoInfo>,
}

/// Read-only snapshot of a live egress edge, for topology rendering.
#[derive(Debug, Clone)]
pub(crate) struct EgressEdgeInfo {
    pub(crate) net_id: u32,
    pub(crate) initiator_ip: IpAddr,
    pub(crate) initiator_docker: Option<String>,
    pub(crate) proxy_ip: IpAddr,
    /// Contacted destinations, most-recently-seen first.
    pub(crate) destinations: Vec<EgressDestination>,
}

#[derive(Debug, Clone)]
pub struct Orchestrator {
    clients: Arc<RwLock<HashMap<IpAddr, OutboundStream>>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
    net_id_pool: Arc<Mutex<NetIdPool>>,
    /// Live egress edges, keyed by initiator replica. Separate from the service
    /// StackMap because the proxy end is infrastructure, not a registered service.
    egress_edges: Arc<RwLock<HashMap<EgressKey, EgressEdge>>>,
    /// IP → country/ASN cache enriching contacted egress destinations.
    geo: GeoCache,
    pub(crate) events: EventStore,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            net_id_pool: Arc::new(Mutex::new(NetIdPool::new())),
            egress_edges: Arc::new(RwLock::new(HashMap::new())),
            geo: GeoCache::from_env(),
            events: EventStore::new(),
        }
    }

    pub(crate) async fn add_client(
        &self,
        request: Request<Streaming<MsgId>>,
        outbound: OutboundStream,
        services: Arc<RwLock<StackMap>>,
    ) -> Result<(), Error> {
        let client_ip = request
            .remote_addr()
            .ok_or("Could not get remote address for control channel request")
            .handle_err(location!())?
            .ip();

        self.clients.write().await.insert(client_ip, outbound);
        self.events
            .emit(Event::node_connected(client_ip.to_string()))
            .await;

        let mut inbound = request.into_inner();
        let orchestrator = self.clone();
        tokio::spawn(async move {
            while let Ok(Some(msg_id)) = inbound.message().await {
                if let Some(tx) = orchestrator.pending.lock().await.remove(&msg_id.id) {
                    let _ = tx.send(());
                }
            }

            println!("Control channel from '{client_ip}' closed");
            orchestrator
                .events
                .emit(Event::node_disconnected(client_ip.to_string()))
                .await;
            orchestrator
                .handle_node_disconnect(client_ip, &services)
                .await;
        });

        Ok(())
    }

    pub(crate) async fn remove_client(&self, ip: &IpAddr) {
        self.clients.write().await.remove(ip);
    }

    pub(crate) async fn handle_node_disconnect(
        &self,
        client_ip: IpAddr,
        services: &Arc<RwLock<StackMap>>,
    ) {
        self.remove_client(&client_ip).await;

        // A disconnected node may host replicas in multiple stacks; apply
        // the per-stack disconnect logic to each.
        let mut services_guard = services.write().await;
        let stack_names: Vec<String> = services_guard.keys().cloned().collect();
        for stack in stack_names {
            let Some(stack_map) = services_guard.get_mut(&stack) else {
                continue;
            };
            let changes = detect_node_disconnect_changes(stack_map, client_ip);
            apply_changes(changes, stack_map, None, self, &stack).await;
        }
        drop(services_guard);

        // Tear down egress edges anchored on the disconnected node, whether it
        // was an initiator or the proxy itself.
        self.teardown_egress_edges_for_node(client_ip).await;
    }

    /// Ensure a single egress edge exists from `(initiator_ip, initiator_docker)`
    /// to the proxy. Returns `Ok(true)` if a new edge was built, `Ok(false)` if
    /// one already exists (idempotent — one edge per initiator replica serves all
    /// external destinations). Race-safe: the slot is reserved under the write
    /// lock before the async NET setup, so concurrent triggers collapse to one.
    pub(crate) async fn ensure_egress_edge(
        &self,
        initiator_ip: IpAddr,
        initiator_docker: Option<String>,
        proxy_ip: IpAddr,
    ) -> Result<bool, Error> {
        let key = (initiator_ip, initiator_docker.clone());

        // Reserve the slot (net_id filled in after allocation).
        {
            let mut edges = self.egress_edges.write().await;
            if edges.contains_key(&key) {
                return Ok(false);
            }
            edges.insert(
                key.clone(),
                EgressEdge {
                    net_id: 0,
                    initiator_ip,
                    initiator_docker: initiator_docker.clone(),
                    proxy_ip,
                    destinations: HashMap::new(),
                },
            );
        }

        let Some(net_id) = self.allocate_net_id().await else {
            self.egress_edges.write().await.remove(&key);
            return Err("NET ID pool exhausted").handle_err(location!());
        };

        // Gateway is the server side (Intercept -> forward/MASQUERADE); initiator
        // is the client side (Steer -> policy-route + SNAT). docker tuple is (client, server).
        let dockers = (initiator_docker.clone(), None);
        let proxy_res = self.send_net_setup(
            proxy_ip,
            None,
            net_id,
            initiator_ip,
            dockers.clone(),
            None,
            EgressRole::Intercept,
        );
        let init_res = self.send_net_setup(
            initiator_ip,
            Some("nullnet-egress".to_string()),
            net_id,
            proxy_ip,
            dockers,
            None,
            EgressRole::Steer,
        );
        let (proxy_ok, init_ok) = tokio::join!(proxy_res, init_res);

        if proxy_ok.is_none() || init_ok.is_none() {
            self.send_net_teardown(initiator_ip, initiator_docker, proxy_ip, None, net_id)
                .await;
            self.egress_edges.write().await.remove(&key);
            return Err("egress edge NET setup failed").handle_err(location!());
        }

        // Promote the reservation to a live edge with its allocated net_id.
        // If a concurrent teardown (container death / node disconnect) removed
        // the reservation during the async build above, the slot is gone: reap
        // the tunnel we just built and free the id, rather than leaking an
        // orphaned edge that no map entry can ever reap.
        let promoted = {
            let mut edges = self.egress_edges.write().await;
            match edges.get_mut(&key) {
                Some(edge) => {
                    edge.net_id = net_id;
                    true
                }
                None => false,
            }
        };
        if !promoted {
            self.send_net_teardown(initiator_ip, initiator_docker, proxy_ip, None, net_id)
                .await;
            return Ok(false);
        }
        Ok(true)
    }

    /// Snapshot the live egress edges (initiator replica -> proxy) for topology
    /// rendering. Reservations that never completed (`net_id == 0`) are omitted.
    pub(crate) async fn egress_edges_snapshot(&self) -> Vec<EgressEdgeInfo> {
        self.egress_edges
            .read()
            .await
            .values()
            .filter(|e| e.net_id != 0)
            .map(|e| {
                let mut destinations: Vec<EgressDestination> = e
                    .destinations
                    .iter()
                    .map(|(ip, s)| EgressDestination {
                        ip: *ip,
                        last_seen: s.last_seen,
                        count: s.count,
                        blocked: s.blocked,
                        geo: self.geo.get(*ip),
                    })
                    .collect();
                destinations.sort_by(|a, b| b.last_seen.cmp(&a.last_seen).then(a.ip.cmp(&b.ip)));
                EgressEdgeInfo {
                    net_id: e.net_id,
                    initiator_ip: e.initiator_ip,
                    initiator_docker: e.initiator_docker.clone(),
                    proxy_ip: e.proxy_ip,
                    destinations,
                }
            })
            .collect()
    }

    /// Record a client-reported external destination on the edge keyed by
    /// `(initiator_ip, initiator_docker)` — the SAME key `ensure_egress_edge` uses,
    /// so the report lands on the correct edge. `count`/`last_seen` are the
    /// client's authoritative values and stored verbatim. No-op if no edge exists
    /// (the client re-sends on its next flush once the edge is up). Bounded by
    /// `MAX_DESTS_PER_EDGE` with least-recently-seen eviction.
    pub(crate) async fn record_egress_destination(
        &self,
        initiator_ip: IpAddr,
        initiator_docker: Option<String>,
        dst_ip: Ipv4Addr,
        count: u64,
        last_seen: u64,
        blocked: bool,
    ) {
        let key = (initiator_ip, initiator_docker);
        let mut edges = self.egress_edges.write().await;
        let Some(edge) = edges.get_mut(&key) else {
            return;
        };
        // Kick off (cached, once-per-IP) geo/ASN enrichment for the UI.
        self.geo.ensure(dst_ip);
        match edge.destinations.get_mut(&dst_ip) {
            Some(stat) => {
                stat.last_seen = last_seen;
                stat.count = count;
                stat.blocked = blocked;
            }
            None => {
                if edge.destinations.len() >= MAX_DESTS_PER_EDGE {
                    if let Some(oldest) = edge
                        .destinations
                        .iter()
                        .min_by_key(|(_, s)| s.last_seen)
                        .map(|(ip, _)| *ip)
                    {
                        edge.destinations.remove(&oldest);
                    }
                }
                edge.destinations.insert(
                    dst_ip,
                    DestStat {
                        last_seen,
                        count,
                        blocked,
                    },
                );
            }
        }
    }

    /// Country (uppercase alpha-2) of `ip` for the egress policy check,
    /// awaiting the (cached, once-per-IP) geo lookup. `None` = unknown.
    pub(crate) async fn destination_country(&self, ip: Ipv4Addr) -> Option<String> {
        self.geo
            .lookup_now(ip)
            .await?
            .country_code
            .map(|c| c.to_uppercase())
    }

    /// Tear down every egress edge anchored on `node_ip` (as initiator or proxy).
    async fn teardown_egress_edges_for_node(&self, node_ip: IpAddr) {
        let removed: Vec<EgressEdge> = {
            let mut edges = self.egress_edges.write().await;
            let keys: Vec<EgressKey> = edges
                .iter()
                .filter(|(_, e)| e.initiator_ip == node_ip || e.proxy_ip == node_ip)
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter().filter_map(|k| edges.remove(&k)).collect()
        };
        for e in removed {
            // Skip reservations that never completed (net_id still 0).
            if e.net_id == 0 {
                continue;
            }
            self.send_net_teardown(
                e.initiator_ip,
                e.initiator_docker,
                e.proxy_ip,
                None,
                e.net_id,
            )
            .await;
        }
    }

    /// Tear down egress edges on `node_ip` whose initiator container is no longer
    /// in `live` (container died / dereg'd with the node still up). Host-process
    /// edges (no container) are left alone.
    pub(crate) async fn teardown_egress_edges_for_missing_containers(
        &self,
        node_ip: IpAddr,
        live: &std::collections::HashSet<String>,
    ) {
        let removed: Vec<EgressEdge> = {
            let mut edges = self.egress_edges.write().await;
            let keys: Vec<EgressKey> = edges
                .iter()
                .filter(|(_, e)| {
                    e.initiator_ip == node_ip
                        && e.initiator_docker
                            .as_ref()
                            .is_some_and(|c| !live.contains(c))
                })
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter().filter_map(|k| edges.remove(&k)).collect()
        };
        for e in removed {
            if e.net_id == 0 {
                continue;
            }
            println!(
                "[egress] reaping edge for gone container {:?} on {}",
                e.initiator_docker, e.initiator_ip
            );
            self.send_net_teardown(
                e.initiator_ip,
                e.initiator_docker,
                e.proxy_ip,
                None,
                e.net_id,
            )
            .await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_net_setup(
        &self,
        dest: IpAddr,
        remote_server_name: Option<String>,
        net_id: u32,
        remote: IpAddr,
        docker_containers: (Option<String>, Option<String>),
        dnat_port: Option<u32>,
        egress: EgressRole,
    ) -> Option<Ipv4Addr> {
        let outbound = self.clients.read().await.get(&dest).cloned();
        if let Some(outbound) = outbound {
            let (tx, rx) = oneshot::channel();
            let msg_id = Uuid::new_v4().to_string();
            self.pending.lock().await.insert(msg_id.clone(), tx);

            let (server_net, message) = NET_TYPE.setup(
                msg_id.clone(),
                dest,
                remote_server_name,
                net_id,
                remote,
                docker_containers,
                dnat_port,
                egress,
            )?;

            if outbound.send(Ok(message)).await.is_err() {
                self.pending.lock().await.remove(&msg_id);
                return None;
            }

            if let Ok(result) = tokio::time::timeout(Duration::from_secs(30), rx).await {
                result.ok().map(|()| server_net)
            } else {
                self.pending.lock().await.remove(&msg_id);
                None
            }
        } else {
            None
        }
    }

    /// Fire-and-forget: tell the host running `docker_container` to `docker pause` it.
    /// Mirrors `send_net_teardown` — no ack; the caller marks the replica suspended.
    pub(crate) async fn send_container_suspend(&self, dest: IpAddr, docker_container: String) {
        let outbound = self.clients.read().await.get(&dest).cloned();
        if let Some(outbound) = outbound {
            println!("Suspending container '{docker_container}' on {dest}");
            let message = NetMessage {
                message: Some(net_message::Message::ContainerSuspend(ContainerSuspend {
                    docker_container,
                })),
            };
            let _ = outbound.send(Ok(message)).await.handle_err(location!());
        }
    }

    /// Fire-and-forget broadcast: an egress country policy changed on a config
    /// reload. Every client drops its cached policy verdicts and flushes
    /// conntrack, so live flows re-verdict (newly-denied ones die). Coarse by
    /// design — reloads are rare and re-verdicting is cheap.
    pub(crate) async fn broadcast_egress_policy_changed(&self) {
        let outbounds: Vec<(IpAddr, OutboundStream)> = self
            .clients
            .read()
            .await
            .iter()
            .map(|(ip, o)| (*ip, o.clone()))
            .collect();
        for (ip, outbound) in outbounds {
            println!("Notifying {ip} of egress policy change");
            let message = NetMessage {
                message: Some(net_message::Message::EgressPolicyChanged(
                    EgressPolicyChanged {},
                )),
            };
            let _ = outbound.send(Ok(message)).await.handle_err(location!());
        }
    }

    /// Ack'd: tell the host to `docker unpause` `docker_container` and wait until it
    /// confirms the container is running again. Mirrors `send_net_setup`'s pending-map
    /// + 30s timeout. Returns `true` once the client acks (service is serving).
    pub(crate) async fn send_container_resume(
        &self,
        dest: IpAddr,
        docker_container: String,
    ) -> bool {
        let outbound = self.clients.read().await.get(&dest).cloned();
        let Some(outbound) = outbound else {
            return false;
        };

        let (tx, rx) = oneshot::channel();
        let msg_id = Uuid::new_v4().to_string();
        self.pending.lock().await.insert(msg_id.clone(), tx);

        println!("Resuming container '{docker_container}' on {dest}");
        let message = NetMessage {
            message: Some(net_message::Message::ContainerResume(ContainerResume {
                msg_id: Some(MsgId { id: msg_id.clone() }),
                docker_container,
            })),
        };

        if outbound.send(Ok(message)).await.is_err() {
            self.pending.lock().await.remove(&msg_id);
            return false;
        }

        if let Ok(result) = tokio::time::timeout(Duration::from_secs(30), rx).await {
            result.is_ok()
        } else {
            self.pending.lock().await.remove(&msg_id);
            false
        }
    }

    pub(crate) async fn allocate_net_id(&self) -> Option<u32> {
        self.net_id_pool.lock().await.allocate()
    }

    pub(crate) async fn connected_node_ips(&self) -> Vec<IpAddr> {
        self.clients.read().await.keys().copied().collect()
    }

    pub(crate) async fn pool_stats(&self) -> (u32, u32) {
        self.net_id_pool.lock().await.stats()
    }

    pub(crate) async fn send_net_teardown(
        &self,
        client: IpAddr,
        client_docker: Option<String>,
        server: IpAddr,
        server_docker: Option<String>,
        net_id: u32,
    ) {
        for (dest, side, docker) in [(client, "c", client_docker), (server, "s", server_docker)] {
            let outbound = self.clients.read().await.get(&dest).cloned();
            if let Some(outbound) = outbound {
                println!("Sending network {net_id} teardown to client {dest}");

                let message = NET_TYPE.teardown(net_id, side, docker);

                let _ = outbound.send(Ok(message)).await.handle_err(location!());
            }
        }
        self.net_id_pool.lock().await.free(net_id);
    }
}

#[cfg(test)]
impl Orchestrator {
    pub(crate) async fn net_ids_in_use(&self) -> u32 {
        self.net_id_pool.lock().await.in_use()
    }

    pub(crate) async fn register_fake_client(&self, ip: IpAddr) {
        self.register_recording_client(ip).await;
    }

    /// Like `register_fake_client`, but returns a log of every `NetMessage` sent
    /// to the client so tests can assert suspend/resume commands were issued.
    pub(crate) async fn register_recording_client(
        &self,
        ip: IpAddr,
    ) -> Arc<Mutex<Vec<NetMessage>>> {
        use nullnet_grpc_lib::nullnet_grpc::net_message;

        let log = Arc::new(Mutex::new(Vec::new()));
        let (tx, mut rx) = mpsc::channel::<Result<NetMessage, Status>>(64);
        self.clients.write().await.insert(ip, tx);

        let pending = self.pending.clone();
        let log_task = log.clone();
        tokio::spawn(async move {
            while let Some(Ok(msg)) = rx.recv().await {
                // Record before acking so a caller blocked on the ack (resume)
                // is guaranteed to observe the message once it unblocks.
                let ack_id = match &msg.message {
                    Some(net_message::Message::VlanSetup(
                        nullnet_grpc_lib::nullnet_grpc::VlanSetup { msg_id, .. },
                    ))
                    | Some(net_message::Message::VxlanSetup(
                        nullnet_grpc_lib::nullnet_grpc::VxlanSetup { msg_id, .. },
                    ))
                    | Some(net_message::Message::ContainerResume(ContainerResume {
                        msg_id, ..
                    })) => msg_id.clone(),
                    _ => None,
                };
                log_task.lock().await.push(msg);
                if let Some(msg_id) = ack_id
                    && let Some(tx) = pending.lock().await.remove(&msg_id.id)
                {
                    let _ = tx.send(());
                }
            }
        });

        log
    }
}
