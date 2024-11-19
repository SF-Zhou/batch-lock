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

pub struct LockManager<K: Eq + Hash> {
    map: DashMap<K, LinkedList<WaiterPtr>>,
}

impl<K: Eq + Hash + Clone> LockManager<K> {
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: DashMap::with_capacity(capacity),
        }
    }

    pub fn with_capacity_and_shard_amount(capacity: usize, shard_amount: usize) -> Self {
        Self {
            map: DashMap::with_capacity_and_shard_amount(capacity, shard_amount),
        }
    }

    pub fn lock(&self, key: K) -> LockGuard<K> {
        self.raw_lock(&key);
        LockGuard::<K> { map: self, key }
    }

    pub fn batch_lock(&self, keys: BTreeSet<K>) -> BatchLockGuard<K> {
        for key in &keys {
            self.raw_lock(key);
        }
        BatchLockGuard::<K> { map: self, keys }
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

pub struct LockGuard<'a, K: Eq + Hash + Clone> {
    map: &'a LockManager<K>,
    key: K,
}

impl<'a, K: Eq + Hash + Clone> Drop for LockGuard<'a, K> {
    fn drop(&mut self) {
        self.map.unlock(&self.key);
    }
}

pub struct BatchLockGuard<'a, K: Eq + Hash + Clone> {
    map: &'a LockManager<K>,
    keys: BTreeSet<K>,
}

impl<'a, K: Eq + Hash + Clone> Drop for BatchLockGuard<'a, K> {
    fn drop(&mut self) {
        self.map.batch_unlock(&self.keys);
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
                        let _guard = lock_map.lock(1);
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
                        let _guard = lock_map.lock(rand::random());
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
                        let _guard = lock_map.batch_lock(state.clone());
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
            key: 42,
        };
    }
}
