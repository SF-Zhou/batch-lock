//! A thread-safe key-based locking mechanism for managing concurrent access to resources.
//!
//! This crate provides a flexible lock manager that allows fine-grained locking based on keys.
//! It supports both single-key and batch locking operations with RAII-style lock guards.
//!
//! # Features
//!
//! - Thread-safe key-based locking
//! - Support for both single key and batch locking operations
//! - RAII-style lock guards
//! - Configurable capacity and sharding
//!
//! # Examples
//!
//! Single key locking:
//! ```
//! use batch_lock::LockManager;
//!
//! let lock_manager = LockManager::<String>::new();
//! let key = "resource_1".to_string();
//! let guard = lock_manager.lock(&key);
//! // Critical section - exclusive access guaranteed
//! // Guard automatically releases lock when dropped
//! ```
//!
//! Batch locking:
//! ```
//! use batch_lock::LockManager;
//! use std::collections::BTreeSet;
//!
//! let lock_manager = LockManager::<String>::new();
//! let mut keys = BTreeSet::new();
//! keys.insert("resource_1".to_string());
//! keys.insert("resource_2".to_string());
//!
//! let guard = lock_manager.batch_lock(&keys);
//! // Critical section - exclusive access to all keys guaranteed
//! // Guard automatically releases all locks when dropped
//! ```
use dashmap::{DashMap, Entry};
use std::collections::{BTreeSet, LinkedList};
use std::hash::Hash;
use std::sync::atomic::{AtomicU32, Ordering};

struct WaiterPtr(*const AtomicU32);

impl WaiterPtr {
    fn wake_up(self) {
        let ptr = self.0;
        let waiter = unsafe { &*ptr };
        waiter.store(1, Ordering::Release);
        atomic_wait::wake_one(ptr);
    }
}

unsafe impl Sync for WaiterPtr {}
unsafe impl Send for WaiterPtr {}

/// A thread-safe lock manager that provides key-based locking capabilities.
///
/// The `LockManager` allows concurrent access control based on keys of type `K`.
/// It supports both single-key and batch locking operations.
///
/// Type parameter:
/// - `K`: The key type that must implement `Eq + Hash + Clone` traits
pub struct LockManager<K: Eq + Hash + Clone> {
    map: DashMap<K, LinkedList<WaiterPtr>>,
}

impl<K: Eq + Hash + Clone> LockManager<K> {
    /// Creates a new `LockManager` instance with default capacity.
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    /// Creates a new `LockManager` with the specified capacity.
    ///
    /// # Arguments
    /// * `capacity` - The initial capacity for the internal map
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: DashMap::with_capacity(capacity),
        }
    }

    /// Creates a new `LockManager` with specified capacity and shard amount.
    ///
    /// # Arguments
    /// * `capacity` - The initial capacity for the internal map
    /// * `shard_amount` - The number of shards to use for internal concurrency
    pub fn with_capacity_and_shard_amount(capacity: usize, shard_amount: usize) -> Self {
        Self {
            map: DashMap::with_capacity_and_shard_amount(capacity, shard_amount),
        }
    }

    /// Acquires a lock for a single key.
    ///
    /// This method will block until the lock can be acquired.
    ///
    /// # Arguments
    /// * `key` - The key to lock
    ///
    /// # Returns
    /// Returns a `LockGuard` that will automatically release the lock when dropped
    pub fn lock<'a, 'b>(&'a self, key: &'b K) -> LockGuard<'a, 'b, K> {
        self.raw_lock(key);
        LockGuard::<'a, 'b, K> { map: self, key }
    }

    /// Acquires locks for multiple keys atomically.
    ///
    /// This method will block until all locks can be acquired. The locks are acquired
    /// in a consistent order to prevent deadlocks.
    ///
    /// # Arguments
    /// * `keys` - A `BTreeSet` containing the keys to lock
    ///
    /// # Returns
    /// Returns a `BatchLockGuard` that will automatically release all locks when dropped
    pub fn batch_lock<'a, 'b>(&'a self, keys: &'b BTreeSet<K>) -> BatchLockGuard<'a, 'b, K> {
        for key in keys {
            self.raw_lock(key);
        }
        BatchLockGuard::<'a, 'b, K> { map: self, keys }
    }

    fn raw_lock(&self, key: &K) {
        let waiter = AtomicU32::new(0);
        match self.map.entry(key.clone()) {
            Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().push_back(WaiterPtr(&waiter as _));
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(Default::default());
                waiter.store(1, Ordering::Release);
            }
        };
        while waiter.load(Ordering::Acquire) == 0 {
            atomic_wait::wait(&waiter, 0);
        }
    }

    fn unlock(&self, key: &K) {
        match self.map.entry(key.clone()) {
            Entry::Occupied(mut occupied_entry) => match occupied_entry.get_mut().pop_front() {
                Some(waiter) => {
                    waiter.wake_up();
                }
                None => {
                    occupied_entry.remove();
                }
            },
            Entry::Vacant(_) => panic!("impossible: unlock a non-existent key!"),
        }
    }

    fn batch_unlock(&self, keys: &BTreeSet<K>) {
        for key in keys.iter().rev() {
            self.unlock(key);
        }
    }
}

impl<K: Eq + Hash + Clone> Default for LockManager<K> {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard for a single locked key.
///
/// When this guard is dropped, the lock will be automatically released.
pub struct LockGuard<'a, 'b, K: Eq + Hash + Clone> {
    map: &'a LockManager<K>,
    key: &'b K,
}

impl<'a, 'b, K: Eq + Hash + Clone> Drop for LockGuard<'a, 'b, K> {
    fn drop(&mut self) {
        self.map.unlock(self.key);
    }
}

/// RAII guard for multiple locked keys.
///
/// When this guard is dropped, all locks will be automatically released
/// in the reverse order they were acquired.
pub struct BatchLockGuard<'a, 'b, K: Eq + Hash + Clone> {
    map: &'a LockManager<K>,
    keys: &'b BTreeSet<K>,
}

impl<'a, 'b, K: Eq + Hash + Clone> Drop for BatchLockGuard<'a, 'b, K> {
    fn drop(&mut self) {
        self.map.batch_unlock(self.keys);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicUsize, Arc};

    #[test]
    fn test_lock_map_same_key() {
        let lock_map = Arc::new(LockManager::<u32>::new());
        let total = Arc::new(AtomicUsize::default());
        let current = Arc::new(AtomicU32::default());
        const N: usize = 1 << 12;
        const M: usize = 8;

        let threads = (0..M)
            .map(|_| {
                let lock_map = lock_map.clone();
                let total = total.clone();
                let current = current.clone();
                std::thread::spawn(move || {
                    for _ in 0..N {
                        let _guard = lock_map.lock(&1);
                        let now = current.fetch_add(1, Ordering::AcqRel);
                        assert_eq!(now, 0);
                        total.fetch_add(1, Ordering::AcqRel);
                        let now = current.fetch_sub(1, Ordering::AcqRel);
                        assert_eq!(now, 1);
                    }
                })
            })
            .collect::<Vec<_>>();
        threads.into_iter().for_each(|t| t.join().unwrap());
        assert_eq!(total.load(Ordering::Acquire), N * M);
    }

    #[test]
    fn test_lock_map_random_key() {
        let lock_map = Arc::new(LockManager::<u32>::with_capacity(128));
        let total = Arc::new(AtomicUsize::default());
        const N: usize = 1 << 20;
        const M: usize = 8;

        let threads = (0..M)
            .map(|_| {
                let lock_map = lock_map.clone();
                let total = total.clone();
                std::thread::spawn(move || {
                    for _ in 0..N {
                        let key = rand::random();
                        let _guard = lock_map.lock(&key);
                        total.fetch_add(1, Ordering::AcqRel);
                    }
                })
            })
            .collect::<Vec<_>>();
        threads.into_iter().for_each(|t| t.join().unwrap());
        assert_eq!(total.load(Ordering::Acquire), N * M);
    }

    #[test]
    fn test_batch_lock() {
        let lock_map = Arc::new(LockManager::<usize>::with_capacity_and_shard_amount(
            128, 16,
        ));
        let total = Arc::new(AtomicUsize::default());
        let current = Arc::new(AtomicU32::default());
        const N: usize = 1 << 12;
        const M: usize = 8;

        let threads = (0..M)
            .map(|i| {
                let lock_map = lock_map.clone();
                let total = total.clone();
                let current = current.clone();
                let state = (0..M).filter(|v| *v != i).collect::<BTreeSet<_>>();
                std::thread::spawn(move || {
                    for _ in 0..N {
                        let _guard = lock_map.batch_lock(&state);
                        let now = current.fetch_add(1, Ordering::AcqRel);
                        assert_eq!(now, 0);
                        total.fetch_add(1, Ordering::AcqRel);
                        let now = current.fetch_sub(1, Ordering::AcqRel);
                        assert_eq!(now, 1);
                    }
                })
            })
            .collect::<Vec<_>>();
        threads.into_iter().for_each(|t| t.join().unwrap());
        assert_eq!(total.load(Ordering::Acquire), N * M);
    }

    #[test]
    #[should_panic(expected = "impossible: unlock a non-existent key!")]
    fn test_invalid_unlock() {
        let lock_map = LockManager::<u32>::default();
        let _lock_guard = LockGuard {
            map: &lock_map,
            key: &42,
        };
    }
}
