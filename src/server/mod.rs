pub mod bind;
pub mod command;
pub mod conns;
pub mod port;

use crate::protocol::text::find_crlf;
use crate::server::command::{Command, parse_command_line};

use anyhow::{Context, Result};
use bytes::{Buf, BytesMut};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, UdpSocket, UnixListener},
};

use crate::{
    server::{
        bind::{InterfaceResolver, SystemResolver, parse_listen},
        conns::{handle_command, handle_set_data},
    },
    store::Store,
};

pub enum Frame {
    Command(Command),
    SetData { command: Command, value: Vec<u8> },
    ServerError(String),
}

#[derive(Clone)]
pub struct ParseConfig {
    pub max_item_size: u64,
}

impl ParseConfig {
    pub fn new(max_item_size: u64) -> Self {
        Self { max_item_size }
    }
}

pub enum Listener {
    Tcp(TcpListener),
    Udp(UdpSocket),
    Unix(UnixListener),
}

impl std::fmt::Display for Listener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Listener::Tcp(listener) => {
                write!(f, "{} (tcp)", listener.local_addr().unwrap().ip())
            }
            Listener::Udp(socket) => write!(f, "{} (udp)", socket.local_addr().unwrap().ip()),
            Listener::Unix(listener) => {
                write!(
                    f,
                    "{} (unix)",
                    listener
                        .local_addr()
                        .unwrap()
                        .as_abstract_name()
                        .map_or(String::new(), |b| String::from_utf8_lossy(b).into_owned())
                )
            }
        }
    }
}

pub async fn create_unix_listener(path: &str, perm: u16) -> anyhow::Result<Listener> {
    let _ = std::fs::remove_file(path);

    let listener =
        UnixListener::bind(path).with_context(|| format!("failed to bind unix listener {path}"))?;

    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(perm as u32))
        .with_context(|| format!("failed to set permissions on {path}"))?;

    tracing::info!("Listening on {path} (unix)");
    Ok(Listener::Unix(listener))
}

pub async fn get_listeners(ifaces: Vec<String>, port: u16, udp: bool) -> Result<Vec<Listener>> {
    let mut addrs: Vec<IpAddr> = Vec::new();

    if ifaces.is_empty() {
        let s_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        let Ok(listener) = TcpListener::bind(s_addr).await else {
            tracing::error!("failed to get unspecified ip addr");
            std::process::exit(1);
        };
        return Ok(vec![Listener::Tcp(listener)]);
    }

    let bind_targets = parse_listen(ifaces);
    let resolver = SystemResolver {};

    for target in bind_targets {
        let target_addrs = resolver.resolve(&target).await;
        for addr in target_addrs {
            addrs.push(addr)
        }
    }

    let mut listeners: Vec<Listener> = Vec::new();

    for addr in addrs {
        let s_addr = SocketAddr::new(addr, port);

        if udp {
            let Ok(socket) = UdpSocket::bind(s_addr).await else {
                tracing::warn!("failed to get udp socket for {s_addr}");
                continue;
            };
            listeners.push(Listener::Udp(socket));
        } else {
            let Ok(listener) = TcpListener::bind(s_addr).await else {
                tracing::warn!("failed to get tcp listener for {s_addr}");
                continue;
            };
            listeners.push(Listener::Tcp(listener));
        }
    }

    Ok(listeners)
}

pub async fn handle_connection<S>(
    mut stream: S,
    store: Arc<Store>,
    config: &ParseConfig,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut read_buf = BytesMut::with_capacity(4096);

    loop {
        let n = stream.read_buf(&mut read_buf).await?;

        if n == 0 {
            return Ok(());
        }

        while let Some(frame) = parse_frame(&mut read_buf, config)? {
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
                Frame::ServerError(message) => {
                    stream
                        .write_all(format!("{message}\r\n").as_bytes())
                        .await?;
                }
            }
        }
    }
}

pub fn parse_frame(buf: &mut BytesMut, config: &ParseConfig) -> Result<Option<Frame>> {
    let Some(line_end) = find_crlf(buf) else {
        return Ok(None);
    };

    let line = &buf[..line_end];
    let line_str = std::str::from_utf8(line)?;

    let command = parse_command_line(line_str)?;

    match command {
        Command::Set { bytes, .. } => {
            let header_len = line_end + 2;
            let total_len = header_len + bytes + 2;

            if u64::try_from(bytes).unwrap_or(u64::MAX) > config.max_item_size {
                if buf.len() < total_len {
                    return Ok(None);
                }

                buf.advance(total_len);

                return Ok(Some(Frame::ServerError(
                    "SERVER_ERROR object too large for cache".into(),
                )));
            }

            if buf.len() < total_len {
                return Ok(None);
            }

            let value = buf[..bytes].to_vec();

            buf.advance(bytes);

            if &buf[..2] != b"\r\n" {
                anyhow::bail!("expected CRLF after set data");
            }

            buf.advance(2);
            Ok(Some(Frame::SetData { command, value }))
        }

        _ => {
            buf.advance(line_end + 2);
            Ok(Some(Frame::Command(command)))
        }
    }
}
