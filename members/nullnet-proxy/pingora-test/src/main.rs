use async_trait::async_trait;
use pingora_core::services::background::background_service;
use std::{sync::Arc, time::Duration};

use pingora_core::Result;
use pingora_core::server::Server;
use pingora_core::server::configuration::Opt;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_load_balancing::{LoadBalancer, health_check, selection::RoundRobin};
use pingora_proxy::{ProxyHttp, Session};

pub struct LB {
    lb: Arc<LoadBalancer<RoundRobin>>,
    tls: bool,
}

#[async_trait]
impl ProxyHttp for LB {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(&self, _session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        let upstream = self
            .lb
            .select(b"", 256) // hash doesn't matter
            .unwrap();

        println!(
            "upstream peer is: {} (weight = {})",
            upstream.addr, upstream.weight
        );

        let sni = if self.tls { "one.one.one.one" } else { "" }.to_string();
        let peer = Box::new(HttpPeer::new(upstream, self.tls, sni));
        Ok(peer)
    }

    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut pingora_http::RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()> {
        if self.tls {
            upstream_request.insert_header("Host", "one.one.one.one")?;
        }
        Ok(())
    }
}

fn main() {
    let tls = std::env::var("TLS")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    let lb_address = "0.0.0.0:3000";
    println!("Running load balancer at {lb_address}");

    let mut upstream_sockets = if tls {
        vec!["1.1.1.1:443", "1.1.1.2:443"]
    } else {
        vec!["0.0.0.0:8080", "0.0.0.0:8081"]
    };
    // add a bad upstream for testing health check
    upstream_sockets.push("0.0.0.0:777");
    println!("Upstreams: {upstream_sockets:?}");

    // read command line arguments
    let opt = Opt::parse_args();
    let mut my_server = Server::new(Some(opt)).unwrap();
    my_server.bootstrap();

    let mut upstreams = LoadBalancer::try_from_iter(upstream_sockets).unwrap();

    // We add health check in the background so that the bad server is never selected.
    let hc = health_check::TcpHealthCheck::new();
    upstreams.set_health_check(hc);
    upstreams.health_check_frequency = Some(Duration::from_secs(1));

    let background = background_service("health_check", upstreams);

    let upstreams = background.task();

    let mut lb =
        pingora_proxy::http_proxy_service(&my_server.configuration, LB { lb: upstreams, tls });
    lb.add_tcp(lb_address);

    if tls {
        let cert_path = format!("{}/tls/server.crt", env!("CARGO_MANIFEST_DIR"));
        let key_path = format!("{}/tls/key.pem", env!("CARGO_MANIFEST_DIR"));
        let mut tls_settings =
            pingora_core::listeners::tls::TlsSettings::intermediate(&cert_path, &key_path).unwrap();
        tls_settings.enable_h2();
        // lb.add_tls_with_settings(lb_address, None, tls_settings);
    }

    my_server.add_service(lb);
    my_server.add_service(background);
    my_server.run_forever();
}
