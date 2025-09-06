use std::{
    collections::HashMap,
    hash::Hash,
    time::{Duration, Instant},
};

pub struct CacheEntry<V> {
    inner: V,
    last_accessed: Instant,
    expires_at: Instant,
}

pub struct CacheMap<K: Hash + Eq + Clone, V> {
    // todo: maybe replace the underlying implementation with something like dashmap
    // for concurrent access? this is not a concern for now -- heavy traffic not expected
    inner: HashMap<K, CacheEntry<V>>,
    default_ttl: Duration,
    max_size: usize,
}

impl<K: Hash + Eq + Clone, V> CacheMap<K, V> {
    pub fn new() -> Self {
        CacheMap {
            default_ttl: Duration::from_secs(60 * 60),
            max_size: 100,
            inner: HashMap::new(),
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = ttl;
        self
    }

    pub fn with_max_size(mut self, max_size: usize) -> Self {
        self.max_size = max_size;
        self
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let now = Instant::now();

        // check for expiration first, and remove if expired
        if let Some(entry) = self.inner.get(key)
            && now >= entry.expires_at
        {
            self.inner.remove(key);
            return None;
        }

        // update last accessed time if found, and return the value
        if let Some(entry) = self.inner.get_mut(key) {
            entry.last_accessed = now;
            return Some(&entry.inner);
        }

        None
    }

    pub fn insert(&mut self, key: K, value: V) {
        let now = Instant::now();
        let entry = CacheEntry {
            inner: value,
            last_accessed: now,
            expires_at: now + self.default_ttl,
        };

        // evict least recently used if at max capacity
        if self.inner.len() >= self.max_size {
            self.evict_lru();
        }

        self.inner.insert(key, entry);
    }

    pub fn evict_lru(&mut self) {
        if let Some(key) = self
            .inner
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(k, _)| k.clone())
        {
            self.inner.remove(&key);
        }
    }
}
