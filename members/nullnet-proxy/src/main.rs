mod env;
mod nullnet_proxy;

use crate::nullnet_proxy::NullnetProxy;
use async_trait::async_trait;
use nullnet_grpc_lib::nullnet_grpc::ProxyRequest;
use nullnet_liberror::{ErrorHandler, Location, location};
use pingora_core::server::Server;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_core::{Error, ErrorType, Result};
use pingora_proxy::{ProxyHttp, Session};
use std::thread;
use std::time::Instant;

const PROXY_PORT: u16 = 80;

#[async_trait]
impl ProxyHttp for NullnetProxy {
    type CTX = ();
    fn new_ctx(&self) -> Self::CTX {}

    async fn upstream_peer(&self, session: &mut Session, _ctx: &mut ()) -> Result<Box<HttpPeer>> {
        println!(
            "Received new proxy request from client: {:?}\n",
            session.client_addr()
        );

        let init_t = Instant::now();

        let host_header = session
            .get_header("host")
            .ok_or("No host header in request")
            .handle_err(location!())
            .map_err(|_| Error::explain(ErrorType::BindError, "No host header in request"))?;
        let host_str = host_header
            .to_str()
            .handle_err(location!())
            .map_err(|_| Error::explain(ErrorType::BindError, "Invalid host header"))?;
        let url = host_str
            .strip_suffix(&format!(":{PROXY_PORT}"))
            .unwrap_or(host_str);
        let client_ip = session
            .client_addr()
            .ok_or("Client address not found in session")
            .handle_err(location!())
            .map_err(|_| {
                Error::explain(ErrorType::BindError, "Client address not found in session")
            })?
            .as_inet()
            .ok_or("Client address is not an Inet address")
            .handle_err(location!())
            .map_err(|_| {
                Error::explain(
                    ErrorType::BindError,
                    "Client address is not an Inet address",
                )
            })?
            .ip()
            .to_string();

        let service_name = url.to_string();
        let proxy_req = ProxyRequest {
            client_ip,
            service_name,
        };
        println!("{proxy_req:?}");
        let upstream = self
            .get_or_add_upstream(proxy_req)
            .await
            .map_err(|_| Error::explain(ErrorType::BindError, "Failed to retrieve upstream"))?;
        println!("upstream: {upstream}\n");

        let peer = Box::new(HttpPeer::new(upstream, false, String::new()));

        println!(
            "TOTAL VLANS SETUP TIME: {} ms\n",
            init_t.elapsed().as_millis()
        );

        Ok(peer)
    }
}

#[tokio::main]
async fn main() -> Result<(), nullnet_liberror::Error> {
    // let _gag1: gag::Redirect<std::fs::File>;
    // let _gag2: gag::Redirect<std::fs::File>;
    // if let Some((gag1, gag2)) = redirect_stdout_stderr_to_file() {
    //     _gag1 = gag1;
    //     _gag2 = gag2;
    // } else {
    //     println!("Failed to redirect stdout and stderr to file, logs will be printed to console");
    // }

    let proxy_address = format!("0.0.0.0:{PROXY_PORT}");

    // start proxy server
    let mut my_server = Server::new(None).handle_err(location!())?;
    my_server.bootstrap();

    let nullnet_proxy = NullnetProxy::new().await?;
    let mut proxy = pingora_proxy::http_proxy_service(&my_server.configuration, nullnet_proxy);
    proxy.add_tcp(&proxy_address);
    my_server.add_service(proxy);

    println!("Running Nullnet proxy at {proxy_address}\n");

    // run on separate thread to avoid "cannot start a runtime from within a runtime"
    let handle = thread::spawn(|| my_server.run_forever());
    handle.join().unwrap();
    Ok(())
}

// fn redirect_stdout_stderr_to_file()
// -> Option<(gag::Redirect<std::fs::File>, gag::Redirect<std::fs::File>)> {
//     let dir = "/var/log/nullnet";
//     std::fs::create_dir_all(dir).handle_err(location!()).ok()?;
//     let timestamp = chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S");
//     let file_path = format!("{dir}/proxy_{timestamp}.txt");
//     if let Ok(logs_file) = std::fs::OpenOptions::new()
//         .create(true)
//         .append(true)
//         .open(&file_path)
//     {
//         println!("Writing logs to '{file_path}'");
//         return Some((
//             gag::Redirect::stdout(logs_file.try_clone().ok()?).ok()?,
//             gag::Redirect::stderr(logs_file).ok()?,
//         ));
//     }
//     None
// }
