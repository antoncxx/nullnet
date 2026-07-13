pub static CONTROL_SERVICE_ADDR: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    std::env::var("CONTROL_SERVICE_ADDR").unwrap_or_else(|_| {
        println!("'CONTROL_SERVICE_ADDR' environment variable not set");
        "0.0.0.0".to_string()
    })
});

pub static CONTROL_SERVICE_PORT: std::sync::LazyLock<u16> = std::sync::LazyLock::new(|| {
    let str = std::env::var("CONTROL_SERVICE_PORT").unwrap_or_else(|_| {
        println!("'CONTROL_SERVICE_PORT' environment variable not set");
        String::new()
    });

    str.parse().unwrap_or(50051)
});

/// Set to `true`/`1` on the host running nullnet-proxy. This node is the egress
/// boundary: it terminates ingress and forwards brokered egress to the internet.
/// Its eBPF firewall runs in **stateful** mode — all outbound is allowed and
/// tracked in the CT map, while inbound obeys the configured allowlist + CT
/// returns. Not the old all-IPv4 permissive mode.
pub static EGRESS_GATEWAY: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| env_bool("EGRESS_GATEWAY"));

/// Explicit destination-port allowlists for the stateful firewall — nothing is
/// hardcoded, so every host-service allow is opt-in. Comma-separated ports, keyed
/// by direction and protocol:
///   - `INGRESS_ALLOW_TCP_PORTS` — local TCP listeners (e.g. `22,80,443`).
///   - `INGRESS_ALLOW_UDP_PORTS` — local UDP listeners (e.g. Swarm gossip `7946`).
///   - `EGRESS_ALLOW_TCP_PORTS`  — TCP dsts the node may initiate to.
///   - `EGRESS_ALLOW_UDP_PORTS`  — UDP dsts the node may initiate to (e.g. DNS
///     `53`, NTP `123`). Replies come back via the CT map, no ingress entry needed.
///
/// ICMP is portless and always allowed (both directions). CT-tracked established
/// returns and nullnet's own control/data plane are always allowed too.
pub static INGRESS_ALLOW_TCP_PORTS: std::sync::LazyLock<Vec<u16>> =
    std::sync::LazyLock::new(|| env_ports("INGRESS_ALLOW_TCP_PORTS"));
pub static INGRESS_ALLOW_UDP_PORTS: std::sync::LazyLock<Vec<u16>> =
    std::sync::LazyLock::new(|| env_ports("INGRESS_ALLOW_UDP_PORTS"));
pub static EGRESS_ALLOW_TCP_PORTS: std::sync::LazyLock<Vec<u16>> =
    std::sync::LazyLock::new(|| env_ports("EGRESS_ALLOW_TCP_PORTS"));
pub static EGRESS_ALLOW_UDP_PORTS: std::sync::LazyLock<Vec<u16>> =
    std::sync::LazyLock::new(|| env_ports("EGRESS_ALLOW_UDP_PORTS"));

/// Parse a comma-separated port list from `name` (blank/invalid entries skipped).
fn env_ports(name: &str) -> Vec<u16> {
    std::env::var(name)
        .unwrap_or_default()
        .split(',')
        .filter_map(|p| p.trim().parse::<u16>().ok())
        .collect()
}

/// Parse a boolean env var (`1`/`true`/`yes`, case-insensitive).
fn env_bool(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}
