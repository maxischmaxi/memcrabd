use anyhow::Result;
use bytes::{Buf, BytesMut};

use crate::protocol::text::find_crlf;
use crate::server::command::{Command, parse_command_line};

pub enum Frame {
    Command(Command),
    SetData { command: Command, value: Vec<u8> },
}

pub fn parse_frame(buf: &mut BytesMut) -> Result<Option<Frame>> {
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

            if buf.len() < total_len {
                return Ok(None);
            }

            buf.advance(header_len);

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
