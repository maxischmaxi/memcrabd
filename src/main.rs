mod log;
mod protocol;
mod server;
mod store;

use anyhow::Result;
use std::sync::Arc;
use tracing::Instrument;

use store::Store;

use clap::Parser;

use crate::server::{Listener, create_unix_listener, get_listeners, port::port_in_range};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short = 'U', long = "udp", default_value_t = false)]
    udp: bool,

    #[arg(short = 'l', long = "listen")]
    listen_interface: Vec<String>,

    #[arg(short = 's', long = "unix-socket")]
    unix_socket: Option<String>,

    #[arg(short = 'a', long = "unix-socket-perm", default_value_t = 0o700)]
    unix_socket_perm: u16,

    #[arg(short = 'p', long = "port", default_value_t = 11211, value_parser = port_in_range)]
    port: u16,

    #[arg(short = 'm', long = "memory", default_value_t = 0)]
    memory: u64,

    #[arg(short = 'M', long = "memory-eviction", default_value_t = false)]
    memory_eviction: bool,

    #[arg(short = 'I', long = "max-item-size", default_value_t = 0)]
    max_item_size: u64,

    #[arg(short = 'c', long = "max-connections", default_value_t = 0)]
    max_connections: u64,

    #[arg(short = 't', long = "threads", default_value_t = 1)]
    threads: u64,

    #[arg(short = 'd', long = "daemonize", default_value_t = false)]
    daemonize: bool,

    #[arg(short = 'u', long = "user", default_value = "")]
    user: String,

    #[arg(short = 'r', long = "core-dump", default_value_t = false)]
    core_dump: bool,

    #[arg(short = 'k', long = "lock-all-memory", default_value_t = false)]
    lock_all_memory: bool,

    #[arg(short = 'C', long = "cas-disable", default_value_t = false)]
    cas_disable: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    log::init(log::Verbosity(1), log::LogFormat::Human);

    let store = Arc::new(Store::new());

    let unix_socket_arg = args.unix_socket.clone();
    let listeners: Vec<Listener> = if let Some(path) = unix_socket_arg {
        let Ok(unix_listener) = create_unix_listener(&path, args.unix_socket_perm).await else {
            tracing::error!("failed to create unix listener");
            std::process::exit(1);
        };
        vec![unix_listener]
    } else {
        let Ok(tcp_udp_listeners) = get_listeners(args.listen_interface, args.port, args.udp).await
        else {
            tracing::error!("failed to create listeners");
            std::process::exit(1);
        };
        tcp_udp_listeners
    };

    if listeners.is_empty() {
        tracing::error!("failed to start server, no interfaces found");
        std::process::exit(1);
    }

    for listener in listeners {
        let store = store.clone();
        tracing::info!("listening on {listener}");

        tokio::spawn(async move {
            match listener {
                Listener::Tcp(tcp) => loop {
                    let Ok((stream, addr)) = tcp.accept().await else {
                        tracing::error!("failed to accept");
                        return;
                    };
                    let store = store.clone();

                    tracing::info!(%addr, "client connected");

                    tokio::spawn(async move {
                        let span = tracing::info_span!("conn", %addr);

                        let result = server::handle_connection(stream, store)
                            .instrument(span)
                            .await;

                        if let Err(err) = result {
                            tracing::warn!(%addr, error = %err, "connection error");
                        }

                        tracing::info!(%addr, "client disconnected");
                    });
                },

                Listener::Udp(socket) => {}

                Listener::Unix(unix) => {
                    let Ok((stream, _)) = unix.accept().await else {
                        tracing::error!("failed to accept unix connection");
                        return;
                    };

                    let store = store.clone();

                    tokio::spawn(async move {
                        let span = tracing::info_span!("conn");

                        let result = server::handle_connection(stream, store)
                            .instrument(span)
                            .await;

                        if let Err(err) = result {
                            tracing::warn!(error = %err, "connection error");
                        }

                        tracing::info!("client disconnected");
                    });
                }
            }
        });
    }

    tokio::signal::ctrl_c().await?;

    let delete_unix_socket_path = args.unix_socket.clone();
    if let Some(delete_path) = delete_unix_socket_path {
        let _ = std::fs::remove_file(&delete_path);
        tracing::info!("removed unix socket {delete_path}");
    }

    Ok(())
}
