pub mod command;
pub mod connection;
pub mod frame;

pub use connection::{handle_command, handle_set_data};
pub use frame::{Frame, parse_frame};

use anyhow::Result;
use bytes::BytesMut;
use std::sync::Arc;
use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::store::Store;

pub async fn handle_connection(mut stream: TcpStream, store: Arc<Store>) -> Result<()> {
    let mut read_buf = BytesMut::with_capacity(4096);

    loop {
        let n = stream.read_buf(&mut read_buf).await?;

        if n == 0 {
            return Ok(());
        }

        while let Some(frame) = parse_frame(&mut read_buf)? {
            match frame {
                Frame::Command(command) => {
                    let should_continue = handle_command(command, &mut stream, &store).await?;

                    if !should_continue {
                        return Ok(());
                    }
                }
                Frame::SetData { command, value } => {
                    let should_continue =
                        handle_set_data(command, value, &mut stream, &store).await?;

                    if !should_continue {
                        return Ok(());
                    }
                }
            }
        }
    }
}
