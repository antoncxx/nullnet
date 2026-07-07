use nullnet_grpc_lib::nullnet_grpc::Net;
use std::net::IpAddr;

/// Host running nullnet-proxy, used as the egress forward-proxy for registered
/// services reaching the external internet. `None` disables egress brokering.
pub static PROXY_IP: std::sync::LazyLock<Option<IpAddr>> = std::sync::LazyLock::new(|| {
    match std::env::var("PROXY_IP") {
        Ok(s) if !s.trim().is_empty() => match s.trim().parse() {
            Ok(ip) => Some(ip),
            Err(_) => {
                println!("'PROXY_IP' environment variable is not a valid IP address: '{s}'");
                None
            }
        },
        _ => None,
    }
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
