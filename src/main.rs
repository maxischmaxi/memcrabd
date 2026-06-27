use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tracing::Instrument;

use memcrabd::{
    args::Args,
    server::{
        self, Listener, ParseConfig,
        conns::{Accept, ConnectionLimiter},
        create_unix_listener, get_listeners,
    },
    store::Store,
};

fn main() -> Result<()> {
    let args = Args::parse();

    if args.max_item_size < 1024 {
        eprintln!("Item max size cannot be less than 1024 bytes.");
        std::process::exit(1);
    }

    if args.max_item_size > 1024 * 1024 * 1024 {
        eprintln!("Cannot set item size limit higher than a gigabyte.");
        std::process::exit(1);
    }

    if args.daemonize {
        memcrabd::daemon::daemonize(false, false)?;
    }

    if let Some(pid_path) = &args.pid_file {
        memcrabd::daemon::save_pid(pid_path)?;
    }

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(args.threads.max(1) as usize)
        .enable_all()
        .build()?
        .block_on(async {
            memcrabd::log::init(memcrabd::log::Verbosity(1), memcrabd::log::LogFormat::Human);
            let store = Arc::new(Store::new());

            let limiter = Arc::new(ConnectionLimiter::new(
                args.max_connections,
                args.maxconns_fast,
            ));

            let unix_socket_arg = args.unix_socket.clone();
            let listeners: Vec<Listener> = if let Some(path) = unix_socket_arg {
                let Ok(unix_listener) = create_unix_listener(&path, args.unix_socket_perm).await
                else {
                    tracing::error!("failed to create unix listener");
                    std::process::exit(1);
                };
                vec![unix_listener]
            } else {
                let Ok(tcp_udp_listeners) =
                    get_listeners(args.listen_interface, args.port, args.udp).await
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

            let config = ParseConfig::new(args.max_item_size);

            for listener in listeners {
                let store = store.clone();
                let limiter = limiter.clone();
                let config = config.clone();

                tracing::info!("listening on {listener}");

                tokio::spawn(async move {
                    match listener {
                        Listener::Tcp(tcp) => {
                            accept_loop(tcp, store, limiter, |addr| addr.to_string(), &config)
                                .await;
                        }

                        Listener::Udp(_socket) => {
                            // UDP ist verbindungslos – kein Connection-Tracking
                            // später: UDP-Handler
                        }

                        Listener::Unix(unix) => {
                            accept_loop(unix, store, limiter, |_| String::new(), &config).await;
                        }
                    }
                });
            }

            tokio::signal::ctrl_c().await?;

            if let Some(pid_path) = &args.pid_file
                && let Err(e) = memcrabd::daemon::remove_pid(pid_path)
            {
                tracing::warn!(error = %e, "failed to remove pid file");
            }

            let delete_unix_socket_path = args.unix_socket.clone();
            if let Some(delete_path) = delete_unix_socket_path {
                let _ = std::fs::remove_file(&delete_path);
                tracing::info!("removed unix socket {delete_path}");
            }

            Ok(())
        })
}

async fn accept_loop<L, F>(
    listener: L,
    store: Arc<Store>,
    limiter: Arc<ConnectionLimiter>,
    addr_fmt: F,
    config: &ParseConfig,
) where
    L: Accept,
    F: Fn(L::Addr) -> String,
{
    loop {
        limiter.wait_until_accepting().await;

        let (stream, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!(error = %e, "accept failed");
                continue;
            }
        };

        let addr_str = addr_fmt(addr);
        let guard = match limiter.try_acquire().await {
            Some(guard) => guard,
            None => {
                tracing::warn!(addr = %addr_str, "connection rejected (max)");
                continue;
            }
        };

        tracing::info!(addr = %addr_str, "client connected");

        let store = store.clone();
        let config = config.clone();
        let span = tracing::info_span!("conn", addr = %addr_str);

        tokio::spawn(
            async move {
                let _guard = guard;

                let result = server::handle_connection(stream, store, &config).await;

                if let Err(err) = result {
                    tracing::warn!(addr = %addr_str, error = %err, "connection error");
                }

                tracing::info!(addr = %addr_str, "client disconnected");
            }
            .instrument(span),
        );
    }
}
