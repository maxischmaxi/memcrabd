use anyhow::Result;

#[derive(Debug)]
pub enum Command {
    Set {
        key: String,
        flags: u32,
        ttl: u64,
        bytes: usize,
        noreply: bool,
    },
    Get {
        keys: Vec<String>,
    },
    Delete {
        key: String,
        noreply: bool,
    },
    Version,
    Quit,
}

pub fn parse_command_line(line: &str) -> Result<Command> {
    let parts: Vec<&str> = line.split_whitespace().collect();

    match parts.as_slice() {
        ["set", key, flags, ttl, bytes] => Ok(Command::Set {
            key: key.to_string(),
            flags: flags.parse()?,
            ttl: ttl.parse()?,
            bytes: bytes.parse()?,
            noreply: false,
        }),

        ["set", key, flags, ttl, bytes, "noreply"] => Ok(Command::Set {
            key: key.to_string(),
            flags: flags.parse()?,
            ttl: ttl.parse()?,
            bytes: bytes.parse()?,
            noreply: true,
        }),

        ["get", keys @ ..] if !keys.is_empty() => Ok(Command::Get {
            keys: keys.iter().map(|key| key.to_string()).collect(),
        }),

        ["delete", key] => Ok(Command::Delete {
            key: key.to_string(),
            noreply: false,
        }),

        ["delete", key, "noreply"] => Ok(Command::Delete {
            key: key.to_string(),
            noreply: true,
        }),

        ["version"] => Ok(Command::Version),

        ["quit"] => Ok(Command::Quit),

        _ => anyhow::bail!("unknown command"),
    }
}
