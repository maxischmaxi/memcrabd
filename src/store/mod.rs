pub mod item;

pub use item::Item;

use std::{
    collections::HashMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::{sync::RwLock, time::Instant};

pub struct Store {
    items: RwLock<HashMap<String, Item>>,
    next_cas: AtomicU64,
}

impl Store {
    pub fn new() -> Self {
        Self {
            items: RwLock::new(HashMap::new()),
            next_cas: AtomicU64::new(1),
        }
    }

    pub async fn set(&self, key: String, flags: u32, ttl: u64, value: Vec<u8>) {
        let expires_at = if ttl == 0 {
            None
        } else {
            Some(Instant::now() + Duration::from_secs(ttl))
        };

        let cas = self.next_cas.fetch_add(1, Ordering::Relaxed);

        let item = Item::new(value, flags, expires_at, cas);

        self.items.write().await.insert(key, item);
    }

    pub async fn get(&self, key: &str) -> Option<Item> {
        let mut items = self.items.write().await;

        let item = items.get(key)?;

        if let Some(expires_at) = item.expires_at
            && Instant::now() >= expires_at
        {
            items.remove(key);
            return None;
        }

        Some(item.clone())
    }

    pub async fn delete(&self, key: &str) -> bool {
        self.items.write().await.remove(key).is_some()
    }
}

