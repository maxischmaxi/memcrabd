mod log;
mod protocol;
mod server;
mod store;

use anyhow::Result;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::Instrument;

use store::Store;

#[tokio::main]
async fn main() -> Result<()> {
    log::init(log::Verbosity(1), log::LogFormat::Human);

    let store = Arc::new(Store::new());

    let listener = TcpListener::bind("127.0.0.1:11211").await?;

    tracing::info!(addr = %listener.local_addr()?, "memcrabd listening");

    loop {
        let (stream, addr) = listener.accept().await?;
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
    }
}

