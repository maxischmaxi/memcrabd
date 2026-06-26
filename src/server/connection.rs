use anyhow::Result;
use std::sync::Arc;
use tokio::{io::AsyncWriteExt, net::TcpStream};

use crate::server::command::Command;
use crate::store::Store;

pub async fn handle_set_data(
    command: Command,
    value: Vec<u8>,
    stream: &mut TcpStream,
    store: &Arc<Store>,
) -> Result<bool> {
    match command {
        Command::Set {
            key,
            flags,
            ttl,
            noreply,
            ..
        } => {
            store.set(key, flags, ttl, value).await;

            if !noreply {
                stream.write_all(b"STORED\r\n").await?;
            }
        }

        _ => {
            stream.write_all(b"CLIENT_ERROR invalid frame\r\n").await?;
        }
    }

    Ok(true)
}

pub async fn handle_command(
    command: Command,
    stream: &mut TcpStream,
    store: &Arc<Store>,
) -> Result<bool> {
    match command {
        Command::Get { keys } => {
            for key in keys {
                if let Some(item) = store.get(&key).await {
                    stream
                        .write_all(
                            format!("VALUE {} {} {}\r\n", key, item.flags, item.value.len())
                                .as_bytes(),
                        )
                        .await?;

                    stream.write_all(&item.value).await?;
                    stream.write_all(b"\r\n").await?;
                }
            }

            stream.write_all(b"END\r\n").await?;
        }

        Command::Delete { key, noreply } => {
            let deleted = store.delete(&key).await;

            if !noreply {
                if deleted {
                    stream.write_all(b"DELETED\r\n").await?;
                } else {
                    stream.write_all(b"NOT_FOUND\r\n").await?;
                }
            }
        }

        Command::Version => {
            stream.write_all(b"VERSION memcrabd 0.1.0\r\n").await?;
        }

        Command::Quit => {
            return Ok(false);
        }

        Command::Set { .. } => {
            stream
                .write_all(b"CLIENT_ERROR invalid set frame\r\n")
                .await?;
        }
    }

    Ok(true)
}
