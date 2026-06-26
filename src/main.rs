mod protocol;
mod server;
mod store;

use anyhow::Result;
use std::sync::Arc;
use tokio::net::TcpListener;

use store::Store;

#[tokio::main]
async fn main() -> Result<()> {
    let store = Arc::new(Store::new());

    let listener = TcpListener::bind("127.0.0.1:11211").await?;

    println!("memcrabd listening on 127.0.0.1:11211");

    loop {
        let (stream, addr) = listener.accept().await?;
        let store = store.clone();

        println!("client conntected: {addr}");

        tokio::spawn(async move {
            if let Err(err) = server::handle_connection(stream, store).await {
                eprintln!("connection error: {err}")
            }

            println!("client disconnected: {addr}")
        });
    }
}
