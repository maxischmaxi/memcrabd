use clap::Parser;

use crate::server::port::{item_size_valid, port_in_range};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[arg(short = 'U', long = "udp", default_value_t = false)]
    pub udp: bool,

    #[arg(short = 'l', long = "listen")]
    pub listen_interface: Vec<String>,

    #[arg(short = 's', long = "unix-socket")]
    pub unix_socket: Option<String>,

    #[arg(short = 'a', long = "unix-socket-perm", default_value_t = 0o700)]
    pub unix_socket_perm: u16,

    #[arg(short = 'p', long = "port", default_value_t = 11211, value_parser = port_in_range)]
    pub port: u16,

    #[arg(short = 'm', long = "memory", default_value_t = 0)]
    pub memory: u64,

    #[arg(short = 'M', long = "memory-eviction", default_value_t = false)]
    pub memory_eviction: bool,

    #[arg(short = 'I', long = "max-item-size", default_value_t = 1024 * 1024, value_parser = item_size_valid)]
    // 1MB
    pub max_item_size: u64,

    #[arg(short = 'c', long = "max-connections", default_value_t = 1024)]
    pub max_connections: u64,

    #[arg(long = "maxconns-fast", default_value_t = false)]
    pub maxconns_fast: bool,

    #[arg(short = 't', long = "threads", default_value_t = 4)]
    pub threads: u64,

    #[arg(short = 'd', long = "daemonize", default_value_t = false)]
    pub daemonize: bool,

    #[arg(short = 'u', long = "user", default_value = "")]
    pub user: String,

    #[arg(short = 'r', long = "core-dump", default_value_t = false)]
    pub core_dump: bool,

    #[arg(short = 'k', long = "lock-all-memory", default_value_t = false)]
    pub lock_all_memory: bool,

    #[arg(short = 'C', long = "cas-disable", default_value_t = false)]
    pub cas_disable: bool,
}
