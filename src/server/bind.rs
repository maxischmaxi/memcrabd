use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use getifs::interfaces;

#[derive(PartialEq, Debug)]
pub enum BindTarget {
    AnyV4,
    AnyV6,
    Ip(IpAddr),
    Interface(String),
    Hostname(String),
}

impl std::fmt::Display for BindTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindTarget::AnyV4 => write!(f, "AnyV4"),
            BindTarget::AnyV6 => write!(f, "AnyV6"),
            BindTarget::Ip(ip) => write!(f, "Ip: {ip}"),
            BindTarget::Interface(name) => write!(f, "Name: {name}"),
            BindTarget::Hostname(hostname) => write!(f, "Hostname: {hostname}"),
        }
    }
}

pub trait InterfaceResolver {
    async fn resolve(&self, target: &BindTarget) -> Vec<IpAddr>;
}

pub struct SystemResolver;
impl InterfaceResolver for SystemResolver {
    async fn resolve(&self, target: &BindTarget) -> Vec<IpAddr> {
        match target {
            BindTarget::AnyV4 => vec![IpAddr::V4(Ipv4Addr::UNSPECIFIED)],
            BindTarget::AnyV6 => vec![IpAddr::V6(Ipv6Addr::UNSPECIFIED)],
            BindTarget::Ip(ip) => vec![*ip],
            BindTarget::Interface(name) => interfaces()
                .unwrap_or_default()
                .iter()
                .filter(|iface| iface.name().eq_ignore_ascii_case(name))
                .flat_map(|iface| iface.addrs().unwrap_or_default())
                .map(|ip| ip.addr())
                .filter(|addr| !is_link_local(addr))
                .collect(),
            BindTarget::Hostname(hostname) => {
                let Ok(lookup) = tokio::net::lookup_host((hostname.as_str(), 0)).await else {
                    return vec![];
                };

                lookup.map(|i| i.ip()).collect()
            }
        }
    }
}

pub async fn resolve<R: InterfaceResolver>(target: &BindTarget, resolver: &R) -> Vec<IpAddr> {
    resolver.resolve(target).await
}

pub fn parse_listen(args: Vec<String>) -> Vec<BindTarget> {
    let mut targets = Vec::new();

    for s in args {
        match s.trim() {
            "*" | "0.0.0.0" => targets.push(BindTarget::AnyV4),
            "::" => targets.push(BindTarget::AnyV6),
            _ => {
                if let Ok(ip) = s.parse::<IpAddr>() {
                    targets.push(BindTarget::Ip(ip));
                    continue;
                }

                if !s.contains(".") {
                    targets.push(BindTarget::Interface(s));
                    continue;
                }

                targets.push(BindTarget::Hostname(s));
            }
        }
    }

    targets
}

fn is_link_local(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V6(v6) => {
            let segs = v6.segments();
            segs[0] & 0xffc0 == 0xfe80
        }
        IpAddr::V4(_) => false,
    }
}
