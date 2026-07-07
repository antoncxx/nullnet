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
/// tracked in the CT map, but inbound is restricted to established returns,
/// control/data plane, and the listener ports in `INGRESS_ALLOW_PORTS` (plus
/// 80/443 for the reverse proxy). Not the old all-IPv4 permissive mode.
pub static EGRESS_GATEWAY: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    matches!(
        std::env::var("EGRESS_GATEWAY")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
});

/// Extra inbound TCP ports the stateful firewall accepts (unsolicited, i.e. real
/// listeners, not CT returns). Comma-separated, e.g. `INGRESS_ALLOW_PORTS=22,8080`
/// on the co-located server host for SSH + dashboard. The gateway additionally
/// allows 80/443 implicitly (see `ebpf::enable`). Empty on pure client nodes.
pub static INGRESS_ALLOW_PORTS: std::sync::LazyLock<Vec<u16>> = std::sync::LazyLock::new(|| {
    std::env::var("INGRESS_ALLOW_PORTS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|p| p.trim().parse::<u16>().ok())
        .collect()
});
