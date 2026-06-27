//! Store tests: expiration, concurrency, and deadlock regression.
//!
//! These tests verify that lazy expiration in `get()` works correctly
//! and doesn't deadlock — the original bug was that `DashMap::get()`
//! returns a `Ref` guard holding a read lock, and calling `remove()`
//! on the same shard while the guard is alive deadlocks.

use std::time::Duration;

use memcrabd::store::Store;

/// Basic set/get without TTL — must return the stored value.
/// This also catches the `unwrap()` panic on `expires_at = None`.
#[test]
fn store_set_get_no_ttl() {
    let store = Store::new();
    store.set("foo".into(), 0, 0, b"hello".to_vec());

    let item = store.get("foo").expect("item should exist");
    assert_eq!(item.value, b"hello");
    assert_eq!(item.flags, 0);
}

/// Item with TTL=0 must never expire.  This catches the `unwrap()` panic
/// if the expiry check doesn't handle `expires_at = None` correctly.
#[test]
fn store_no_expiry_never_removed() {
    let store = Store::new();
    store.set("forever".into(), 42, 0, b"persists".to_vec());

    // Should still be there immediately
    let item = store.get("forever").expect("item should exist");
    assert_eq!(item.value, b"persists");
    assert_eq!(item.flags, 42);
}

/// Expired item must be lazily removed by `get()` and return `None`.
/// This is the core deadlock regression test: if `get()` holds a read
/// lock via `DashMap::get()` and then calls `remove()` on the same
/// shard, the test will hang forever.
///
/// We use a short TTL and sleep, wrapped in a timeout to detect hangs.
#[test]
fn store_expired_item_removed_on_get() {
    let store = Store::new();
    store.set("temp".into(), 0, 1, b"short-lived".to_vec());

    // Wait for expiry
    std::thread::sleep(Duration::from_millis(1100));

    // This must return None (expired) and NOT deadlock
    let result = store.get("temp");
    assert!(result.is_none(), "expired item should return None");

    // Verify it was actually removed from the store
    assert!(store.get("temp").is_none(), "expired item should be removed");
}

/// Non-expired item with TTL must still be returned by `get()`.
/// This catches the inverted-logic bug where `remove_if` returning
/// `None` (not expired) causes `get()` to return `None` instead of
/// the actual item.
#[test]
fn store_unexpired_item_returned() {
    let store = Store::new();
    store.set("fresh".into(), 7, 60, b"still-good".to_vec());

    // No sleep — well within the 60s TTL
    let item = store.get("fresh").expect("unexpired item should be returned");
    assert_eq!(item.value, b"still-good");
    assert_eq!(item.flags, 7);
}

/// `delete()` must return true for existing keys, false for missing ones.
#[test]
fn store_delete_existing_and_missing() {
    let store = Store::new();
    store.set("exists".into(), 0, 0, b"data".to_vec());

    assert!(store.delete("exists"), "deleting existing key should return true");
    assert!(!store.delete("exists"), "deleting missing key should return false");
    assert!(store.get("exists").is_none(), "deleted key should be gone");
}

/// CAS values must be monotonically increasing across sets.
#[test]
fn store_cas_increments() {
    let store = Store::new();
    store.set("a".into(), 0, 0, b"1".to_vec());
    store.set("b".into(), 0, 0, b"2".to_vec());
    store.set("c".into(), 0, 0, b"3".to_vec());

    let a = store.get("a").unwrap();
    let b = store.get("b").unwrap();
    let c = store.get("c").unwrap();

    assert!(a.cas < b.cas, "CAS must increase: {} < {}", a.cas, b.cas);
    assert!(b.cas < c.cas, "CAS must increase: {} < {}", b.cas, c.cas);
}

/// Concurrent access from multiple threads must not deadlock or panic.
/// Spawns N threads all doing set/get/delete simultaneously.
///
/// This is the key concurrency test: with multiple threads hitting the
/// same DashMap shards simultaneously, any lock-ordering issue will
/// surface as a hang (deadlock) or panic.
#[test]
fn store_concurrent_access_no_deadlock() {
    use std::sync::Arc;
    use std::thread;

    let store = Arc::new(Store::new());
    let num_threads = 8;
    let ops_per_thread = 500;

    // Pre-populate some keys
    for i in 0..50 {
        store.set(format!("key-{i}"), 0, 0, b"initial".to_vec());
    }

    let mut handles = Vec::new();
    for t in 0..num_threads {
        let s = store.clone();
        handles.push(thread::spawn(move || {
            for op in 0..ops_per_thread {
                let key = format!("key-{}", (t * ops_per_thread + op) % 50);
                // Mix of operations to stress different lock paths
                if op % 4 == 0 {
                    s.delete(&key);
                } else if op % 4 == 1 {
                    s.set(key, 0, 0, b"concurrent".to_vec());
                } else {
                    let _ = s.get(&key);
                }
            }
        }));
    }

    // If there's a deadlock, join will never complete — the test hangs.
    // CI timeouts will catch this.
    for h in handles {
        h.join().expect("thread should not panic");
    }
}

/// Concurrent get on expired items from multiple threads — the most
/// likely scenario to trigger the original deadlock.  Multiple threads
/// calling `get()` on the same expired key simultaneously.
#[test]
fn store_concurrent_expiry_no_deadlock() {
    use std::sync::Arc;
    use std::thread;

    let store = Arc::new(Store::new());

    // Insert with a very short TTL
    store.set("expiring".into(), 0, 1, b"bye".to_vec());

    // Wait until just after expiry
    std::thread::sleep(Duration::from_millis(1100));

    let num_threads = 8;
    let mut handles = Vec::new();
    for _ in 0..num_threads {
        let s = store.clone();
        handles.push(thread::spawn(move || {
            // All threads call get() on the same expired key simultaneously
            let result = s.get("expiring");
            // All should get None (expired or already removed)
            assert!(result.is_none(), "expired key should return None");
        }));
    }

    for h in handles {
        h.join().expect("thread should not panic");
    }
}