use super::preview_request;
use crate::commands::dev::utils::{get_path_as_str, rewrite_redirect};
use crate::commands::dev::{tls, Protocol, ServerConfig};
use crate::terminal::emoji;
use crate::terminal::message::{Message, StdOut};
use std::sync::{Arc, Mutex};

use chrono::prelude::*;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client as HyperClient, Request, Server};
use hyper_rustls::HttpsConnector;
use tokio::net::TcpListener;

pub async fn https(
    server_config: ServerConfig,
    preview_token: Arc<Mutex<String>>,
    host: String,
) -> Result<(), failure::Error> {
    tls::generate_cert()?;

    // set up https client to connect to the preview service
    let https = HttpsConnector::new();
    let client = HyperClient::builder().build::<_, Body>(https);

    let listening_address = server_config.listening_address;

    // create a closure that hyper will use later to handle HTTP requests
    let service = make_service_fn(move |_| {
        let client = client.to_owned();
        let preview_token = preview_token.to_owned();
        let host = host.to_owned();
        let server_config = server_config.to_owned();

        async move {
            Ok::<_, failure::Error>(service_fn(move |req| {
                let client = client.to_owned();
                let preview_token = preview_token.lock().unwrap().to_owned();
                let host = host.to_owned();
                let version = req.version();
                let (parts, body) = req.into_parts();
                let local_host = format!(
                    "{}:{}",
                    server_config.listening_address.ip().to_string(),
                    server_config.listening_address.port().to_string()
                );
                let req_method = parts.method.to_string();
                let now: DateTime<Local> = Local::now();
                let path = get_path_as_str(&parts.uri);
                async move {
                    let mut resp = preview_request(
                        Request::from_parts(parts, body),
                        client,
                        preview_token.to_owned(),
                        host.clone(),
                        Protocol::Https,
                    )
                    .await?;

                    rewrite_redirect(&mut resp, &host, &local_host, true);

                    println!(
                        "[{}] {} {}{} {:?} {}",
                        now.format("%Y-%m-%d %H:%M:%S"),
                        req_method,
                        host,
                        path,
                        version,
                        resp.status()
                    );
                    Ok::<_, failure::Error>(resp)
                }
            }))
        }
    });

    // Create a TCP listener via tokio.
    let tcp = TcpListener::bind(&listening_address).await?;
    // I may have nuked part of the TLS operation.
    let tls_acceptor = &tls::get_tls_acceptor()?;
    let (_socket, addr) = tcp.accept().await.unwrap();
    let server = Server::bind(&addr).serve(service);

    println!("{} Listening on https://{}", emoji::EAR, listening_address);
    StdOut::info("Generated certificate is not verified, browsers will give a warning and curl will require `--insecure`");

    if let Err(e) = server.await {
        eprintln!("{}", e);
    }

    Ok(())
}
