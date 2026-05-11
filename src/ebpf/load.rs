use std::collections::HashMap;
use std::net::Ipv4Addr;

use aya::{
    Ebpf, EbpfLoader, include_bytes_aligned,
    maps::{HashMap as AyaHashMap, RingBuf},
    programs::{SchedClassifier, TcAttachType, tc},
};
use tokio::io::Interest;
use tokio::io::unix::AsyncFd;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Spin up the eBPF programs and the egress observer task.
///
/// `config_rx` carries port → service-name updates pushed by the
/// services-list loop in `main`. The observer applies the diff to the
/// kernel-side `WATCH_PORTS` map and emits `(service_name, port)` tuples on
/// `trigger_tx` whenever the kernel reports outgoing traffic on a watched
/// port.
pub fn load_ebpf(
    eth_name: &str,
    config_rx: UnboundedReceiver<HashMap<u16, String>>,
    trigger_tx: UnboundedSender<(String, u16)>,
) {
    crate::ebpf::log::init();
    raise_memlock_rlimit();

    println!("[load_ebpf] eth={eth_name}");

    // Ingress: attach a passive program kept alive for future use.
    {
        let eth_name = eth_name.to_string();
        tokio::spawn(async move {
            match attach(&eth_name, TcAttachType::Ingress) {
                Ok(_bpf) => {
                    println!("[Ingress] attached and idle");
                    std::future::pending::<()>().await
                }
                Err(e) => eprintln!("[Ingress] {e}"),
            }
        });
    }

    // Egress: attach, then poll EVENTS and apply config updates.
    {
        let eth_name = eth_name.to_string();
        tokio::spawn(async move {
            let mut bpf = match attach(&eth_name, TcAttachType::Egress) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("[Egress] {e}");
                    return;
                }
            };
            if let Err(e) = run_observer(&mut bpf, config_rx, trigger_tx).await {
                eprintln!("[Egress] {e}");
            }
        });
    }
}

fn raise_memlock_rlimit() {
    let rlim = libc::rlimit {
        rlim_cur: libc::RLIM_INFINITY,
        rlim_max: libc::RLIM_INFINITY,
    };
    let ret = unsafe { libc::setrlimit(libc::RLIMIT_MEMLOCK, &rlim) };
    if ret != 0 {
        let err = std::io::Error::last_os_error();
        eprintln!("[load_ebpf] setrlimit(RLIMIT_MEMLOCK) failed: {err} (eBPF load may fail)");
    }
}

fn attach(eth_name: &str, direction: TcAttachType) -> Result<Ebpf, String> {
    let mut loader = EbpfLoader::new();
    if direction == TcAttachType::Egress {
        loader.set_global("IS_EGRESS", &1u8, true);
    }

    let mut bpf = loader
        .load(include_bytes_aligned!(env!(
            "NULLNET_BIN_PATH",
            "NULLNET_BIN_PATH not set — build via `cargo xtask build` (see README)"
        )))
        .map_err(|e| format!("[{direction:?}] load eBPF bytecode: {e}"))?;
    println!("[{direction:?}] eBPF bytecode loaded");

    match tc::qdisc_add_clsact(eth_name) {
        Ok(()) => println!("[{direction:?}] clsact qdisc added on {eth_name}"),
        Err(e) => {
            println!("[{direction:?}] clsact qdisc add returned: {e} (ok if already present)")
        }
    }

    let program: &mut SchedClassifier = bpf
        .program_mut("nullnet_filter_ports")
        .ok_or_else(|| {
            format!("[{direction:?}] program 'nullnet_filter_ports' not found in bytecode")
        })?
        .try_into()
        .map_err(|e| format!("[{direction:?}] program is not a SchedClassifier: {e}"))?;

    program
        .load()
        .map_err(|e| format!("[{direction:?}] load program into kernel: {e}"))?;
    println!("[{direction:?}] program loaded into kernel");

    program
        .attach(eth_name, direction)
        .map_err(|e| format!("[{direction:?}] attach to '{eth_name}': {e}"))?;
    println!("[{direction:?}] program attached to {eth_name}");

    Ok(bpf)
}

async fn run_observer(
    bpf: &mut Ebpf,
    mut config_rx: UnboundedReceiver<HashMap<u16, String>>,
    trigger_tx: UnboundedSender<(String, u16)>,
) -> Result<(), String> {
    let events: RingBuf<_> = bpf
        .take_map("EVENTS")
        .ok_or_else(|| "map 'EVENTS' not found".to_string())?
        .try_into()
        .map_err(|e| format!("EVENTS is not a RingBuf: {e}"))?;
    println!("[observer] waiting on EVENTS ring buffer");

    let mut async_fd = AsyncFd::with_interest(events, Interest::READABLE)
        .map_err(|e| format!("registering EVENTS fd with tokio: {e}"))?;

    let mut port_to_service: HashMap<u16, String> = HashMap::new();

    loop {
        tokio::select! {
            maybe_config = config_rx.recv() => {
                let Some(new_config) = maybe_config else {
                    return Ok(());
                };
                apply_watch_ports_diff(bpf, &port_to_service, &new_config);
                port_to_service = new_config;
            }
            res = async_fd.readable_mut() => {
                let mut guard = res.map_err(|e| format!("waiting on EVENTS readable: {e}"))?;
                let events = guard.get_inner_mut();
                while let Some(item) = events.next() {
                    let bytes: &[u8] = &item;
                    if bytes.len() < 6 {
                        continue;
                    }
                    let port = u16::from_le_bytes([bytes[0], bytes[1]]);
                    let dst_ip = Ipv4Addr::new(bytes[2], bytes[3], bytes[4], bytes[5]);
                    if let Some(service_name) = port_to_service.get(&port) {
                        if let Err(e) = trigger_tx.send((service_name.clone(), port)) {
                            eprintln!(
                                "[observer] failed to enqueue trigger for '{service_name}' port {port} dst {dst_ip}: {e}"
                            );
                        } else {
                            println!("[observer] enqueued trigger for '{service_name}' port {port} dst {dst_ip}");
                        }
                    } else {
                        println!("[observer] no service mapped to port {port} dst {dst_ip}");
                    }
                }
                guard.clear_ready();
            }
        }
    }
}

fn apply_watch_ports_diff(bpf: &mut Ebpf, old: &HashMap<u16, String>, new: &HashMap<u16, String>) {
    let Some(map) = bpf.map_mut("WATCH_PORTS") else {
        eprintln!("[observer] WATCH_PORTS map not found; skipping diff");
        return;
    };
    let mut watch_ports: AyaHashMap<_, u16, u8> = match map.try_into() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[observer] WATCH_PORTS is not a HashMap: {e}");
            return;
        }
    };
    for &port in old.keys() {
        if !new.contains_key(&port) {
            match watch_ports.remove(&port) {
                Ok(()) => println!("[observer] unwatched port {port}"),
                Err(e) => eprintln!("[observer] failed to remove watch port {port}: {e}"),
            }
        }
    }
    for &port in new.keys() {
        if !old.contains_key(&port) {
            match watch_ports.insert(port, 0u8, 0) {
                Ok(()) => println!("[observer] watching port {port}"),
                Err(e) => eprintln!("[observer] failed to insert watch port {port}: {e}"),
            }
        }
    }
}
