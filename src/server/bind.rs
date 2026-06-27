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

pub async fn resolve(target: &BindTarget) -> Vec<IpAddr> {
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

fn is_link_local(addr: &IpAddr) -> bool {
    match addr {
        IpAddr::V6(v6) => {
            let segs = v6.segments();
            segs[0] & 0xffc0 == 0xfe80
        }
        IpAddr::V4(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[test]
    fn test_parse_listen_wildcard() {
        let args = vec![String::from("*")];
        let input = parse_listen(args);
        let outcome = vec![BindTarget::AnyV4];
        assert_eq!(input, outcome);
    }

    #[test]
    fn test_parse_listen_localhost_ip() {
        let args = vec![(String::from("127.0.0.1"))];
        let input = parse_listen(args);
        let ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let outcome = vec![BindTarget::Ip(ip)];

        assert_eq!(input, outcome);
    }

    #[test]
    fn test_parse_listen_all_interfaces() {
        let args = vec![(String::from("0.0.0.0"))];
        let input = parse_listen(args);
        let outcome = vec![BindTarget::AnyV4];

        assert_eq!(input, outcome);
    }

    #[test]
    fn test_parse_listen_localhost() {
        let args = vec![(String::from("localhost"))];
        let input = parse_listen(args);
        let outcome = vec![BindTarget::Interface(String::from("localhost"))];

        assert_eq!(input, outcome);
    }

    #[test]
    fn test_parse_listen_interface_name() {
        let args = vec![(String::from("enp7s0"))];
        let input = parse_listen(args);
        let outcome = vec![BindTarget::Interface(String::from("enp7s0"))];

        assert_eq!(input, outcome);
    }

    #[test]
    fn test_parse_listen_v6_localhost() {
        let args = vec![(String::from("::"))];
        let input = parse_listen(args);
        let outcome = vec![BindTarget::AnyV6];

        assert_eq!(input, outcome);
    }
}
