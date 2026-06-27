use std::net::IpAddr;

use memcrabd::server::bind::{BindTarget, InterfaceResolver, resolve};

mod common;

struct FakeResolver {
    addrs: Vec<IpAddr>,
}

impl InterfaceResolver for FakeResolver {
    async fn resolve(&self, _target: &BindTarget) -> Vec<IpAddr> {
        self.addrs.clone()
    }
}

#[tokio::test]
async fn test_resolve_returns_fake_ips() {
    let fake = FakeResolver {
        addrs: vec!["127.0.0.1".parse().unwrap()],
    };
    let result = resolve(&BindTarget::Interface("whatever".into()), &fake).await;
    let expected: IpAddr = "127.0.0.1".parse().unwrap();
    assert_eq!(result, vec![expected]);
}
