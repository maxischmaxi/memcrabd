use dashmap::DashMap;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::time::Instant;
use tracing::instrument;

pub mod item;

pub use item::Item;

#[derive(Default)]
pub struct Store {
    items: DashMap<String, Item>,
    next_cas: AtomicU64,
}

impl Store {
    pub fn new() -> Self {
        Self {
            items: DashMap::new(),
            next_cas: AtomicU64::new(1),
        }
    }

    #[instrument(skip(self), level = "trace")]
    pub fn set(&self, key: String, flags: u32, ttl: u64, value: Vec<u8>) {
        let expires_at = if ttl == 0 {
            None
        } else {
            Some(Instant::now() + Duration::from_secs(ttl))
        };

        let cas = self.next_cas.fetch_add(1, Ordering::Relaxed);

        let item = Item::new(value, flags, expires_at, cas);

        self.items.insert(key, item);
    }

    #[instrument(skip(self), level = "trace")]
    pub fn get(&self, key: &str) -> Option<Item> {
        if self
            .items
            .remove_if(key, |_, position| {
                position.expires_at.is_some_and(|exp| Instant::now() >= exp)
            })
            .is_some()
        {
            tracing::trace!(%key, "item expired, removed");
            return None;
        }

        self.items.get(key).map(|r| r.clone())
    }

    #[instrument(skip(self), level = "trace")]
    pub fn delete(&self, key: &str) -> bool {
        self.items.remove(key).is_some()
    }
}
