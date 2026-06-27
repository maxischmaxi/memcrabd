use anyhow::Result;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::instrument;

use crate::server::command::Command;
use crate::store::Store;

#[instrument(skip(stream, store), ret, level = "debug")]
pub async fn handle_set_data<S>(
    command: Command,
    value: Vec<u8>,
    stream: &mut S,
    store: &Arc<Store>,
) -> Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match command {
        Command::Set {
            key,
            flags,
            ttl,
            noreply,
            ..
        } => {
            tracing::debug!(%key, flags, ttl, bytes = value.len(), "storing item");

            store.set(key, flags, ttl, value).await;

            if !noreply {
                stream.write_all(b"STORED\r\n").await?;
            }
        }

        _ => {
            tracing::warn!("invalid set frame - unexpected command variant");
            stream.write_all(b"CLIENT_ERROR invalid frame\r\n").await?;
        }
    }

    Ok(true)
}

#[instrument(skip(stream, store), ret, level = "debug")]
pub async fn handle_command<S>(command: Command, stream: &mut S, store: &Arc<Store>) -> Result<bool>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    match command {
        Command::Get { keys } => {
            tracing::debug!(key_count = keys.len(), "get request");

            for key in keys {
                if let Some(item) = store.get(&key).await {
                    tracing::debug!(%key, flags = item.flags, bytes = item.value.len(), "cache hit");

                    stream
                        .write_all(
                            format!("VALUE {} {} {}\r\n", key, item.flags, item.value.len())
                                .as_bytes(),
                        )
                        .await?;

                    stream.write_all(&item.value).await?;
                    stream.write_all(b"\r\n").await?;
                } else {
                    tracing::debug!(%key, "cache miss");
                }
            }

            stream.write_all(b"END\r\n").await?;
        }

        Command::Delete { key, noreply } => {
            let deleted = store.delete(&key).await;

            tracing::debug!(%key, deleted, "delete");

            if !noreply {
                if deleted {
                    stream.write_all(b"DELETED\r\n").await?;
                } else {
                    stream.write_all(b"NOT_FOUND\r\n").await?;
                }
            }
        }

        Command::Version => {
            tracing::trace!("version request");
            stream.write_all(b"VERSION memcrabd 0.1.0\r\n").await?;
        }

        Command::Quit => {
            tracing::debug!("client requested quit");
            return Ok(false);
        }

        Command::Set { .. } => {
            tracing::warn!("set command reached handle_command - should be in handle_set_data");
            stream
                .write_all(b"CLIENT_ERROR invalid set frame\r\n")
                .await?;
        }
    }

    Ok(true)
}

