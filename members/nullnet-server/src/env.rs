use nullnet_grpc_lib::nullnet_grpc::Net;
use std::net::IpAddr;

/// Host running nullnet-proxy, used as the egress forward-proxy for registered
/// services reaching the external internet. `None` disables egress brokering.
pub static PROXY_IP: std::sync::LazyLock<Option<IpAddr>> =
    std::sync::LazyLock::new(|| match std::env::var("PROXY_IP") {
        Ok(s) if !s.trim().is_empty() => match s.trim().parse() {
            Ok(ip) => Some(ip),
            Err(_) => {
                println!("'PROXY_IP' environment variable is not a valid IP address: '{s}'");
                None
            }
        },
        _ => None,
    });

pub static NET_TYPE: std::sync::LazyLock<Net> = std::sync::LazyLock::new(|| {
    let str = std::env::var("NET_TYPE").unwrap_or_else(|_| {
        println!("'NET_TYPE' environment variable not set");
        String::new()
    });

    match str.to_uppercase().as_str() {
        "VXLAN" => Net::Vxlan,
        "VLAN" => Net::Vlan,
        _ => Net::default(),
    }
});

/// Set to `false`/`0`/`no` to disable per-tunnel VLAN/VXLAN encryption
/// entirely: VLAN falls back to plain (unencrypted) userspace forwarding,
/// VXLAN falls back to a bare vxlan/veth link with no XFRM SA/policy or
/// MACsec. Defaults to enabled, preserving existing behavior when unset —
/// this is an opt-out, not an opt-in, so it needs its own parser rather than
/// nullnet-client's `env_bool` (which defaults false/opt-in).
pub static ENCRYPTION_ENABLED: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    match std::env::var("ENCRYPTION_ENABLED") {
        Ok(s) => !matches!(s.trim().to_ascii_lowercase().as_str(), "0" | "false" | "no"),
        Err(_) => true,
    }
});
