use memcrabd::server::Listener;
use std::net::{IpAddr, Ipv4Addr};

use memcrabd::server::{
    bind::{BindTarget, parse_listen},
    get_listeners,
};

#[tokio::test]
async fn get_listeners_binds_loopback_v4() {
    // 127.0.0.1 existiert auf jedem System
    // Port 0 = OS wählt einen freien Port
    let listeners = get_listeners(
        vec!["127.0.0.1".to_string()],
        0,
        false, // TCP, nicht UDP
    )
    .await
    .expect("should succeed");

    assert_eq!(listeners.len(), 1, "exactly one TCP listener expected");

    // Verifiziere: es ist TCP und es lauscht auf 127.0.0.1
    match &listeners[0] {
        Listener::Tcp(listener) => {
            let local = listener.local_addr().unwrap();
            assert_eq!(local.ip(), "127.0.0.1".parse::<IpAddr>().unwrap());
            assert_ne!(local.port(), 0, "OS should have assigned a port");
        }
        other => panic!("expected Tcp, got {other}"),
    }
}

#[tokio::test]
async fn get_listeners_binds_loopback_v6() {
    let listeners = get_listeners(vec!["::1".to_string()], 0, false)
        .await
        .expect("should succeed");

    assert_eq!(listeners.len(), 1);

    match &listeners[0] {
        Listener::Tcp(listener) => {
            let local = listener.local_addr().unwrap();
            assert_eq!(local.ip(), "::1".parse::<IpAddr>().unwrap());
        }
        other => panic!("expected Tcp, got {other}"),
    }
}

#[tokio::test]
async fn get_listeners_with_port_zero_gets_assigned_port() {
    let listeners = get_listeners(vec!["127.0.0.1".to_string()], 0, false)
        .await
        .unwrap();

    if let Listener::Tcp(listener) = &listeners[0] {
        let port = listener.local_addr().unwrap().port();
        assert!(port > 0, "OS must assign a real port, not 0");
        // port is u16, so it's inherently <= 65535 — no upper-bound check needed
    }
}

#[tokio::test]
async fn get_listeners_empty_ifaces_binds_unspecified() {
    let listeners = get_listeners(
        vec![], // kein -l angegeben
        0,
        false,
    )
    .await
    .expect("should bind 0.0.0.0");

    assert_eq!(listeners.len(), 1);

    match &listeners[0] {
        Listener::Tcp(listener) => {
            let ip = listener.local_addr().unwrap().ip();
            assert_eq!(
                ip,
                "0.0.0.0".parse::<IpAddr>().unwrap(),
                "should bind INADDR_ANY"
            );
        }
        other => panic!("expected Tcp, got {other}"),
    }
}

#[tokio::test]
async fn get_listeners_nonexistent_interface_yields_empty() {
    let listeners = get_listeners(
        vec!["definitely_not_a_real_interface_xyz".to_string()],
        0,
        false,
    )
    .await
    .unwrap(); // get_listeners gibt Ok([]) zurück, nicht Err

    assert!(
        listeners.is_empty(),
        "no listener for nonexistent interface"
    );
}

#[tokio::test]
async fn get_listeners_can_accept_a_connection() {
    use tokio::net::TcpStream;

    let listeners = get_listeners(vec!["127.0.0.1".to_string()], 0, false)
        .await
        .unwrap();

    let Listener::Tcp(listener) = &listeners[0] else {
        panic!("expected TCP");
    };
    let addr = listener.local_addr().unwrap();

    // Verbinde dich als Client
    let _client = TcpStream::connect(addr).await.expect("should connect");

    // Server-Seite: accept() sollte eine Verbindung liefern
    // (im Test nicht spawnen – einfach direkt accept aufrufen)
    let (server_stream, _) = listener.accept().await.expect("should accept");

    assert!(server_stream.local_addr().is_ok());
}

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
