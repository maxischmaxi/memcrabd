use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum LogFormat {
    #[default]
    Human,
    Json,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Verbosity(pub u8);

impl Verbosity {
    fn to_filter_string(self) -> &'static str {
        match self.0 {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    }
}

pub fn init(verbosity: Verbosity, format: LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(verbosity.to_filter_string()));

    let registry = tracing_subscriber::registry().with(filter);

    match format {
        LogFormat::Human => {
            registry.with(fmt::layer().with_target(false)).init();
        }
        LogFormat::Json => {
            registry.with(fmt::layer().json().with_target(true)).init();
        }
    }
}

