use std::{
    collections::HashMap,
    hash::Hash,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Entry<V> {
    value: V,
    expires_at_ms: u64,
}

pub struct TtlCache<K, V> {
    entries: HashMap<K, Entry<V>>,
    ttl_ms: Box<dyn Fn() -> u64 + Send + Sync>,
    now_ms: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + Hash,
{
    pub fn new(ttl_ms: u64) -> Result<Self, String> {
        if ttl_ms == 0 {
            return Err("TTL cache requires a positive ttlMs, got 0".into());
        }
        Ok(Self::with_clock(move || ttl_ms, system_now_ms))
    }

    pub fn with_clock(
        ttl_ms: impl Fn() -> u64 + Send + Sync + 'static,
        now_ms: impl Fn() -> u64 + Send + Sync + 'static,
    ) -> Self {
        Self {
            entries: HashMap::new(),
            ttl_ms: Box::new(ttl_ms),
            now_ms: Box::new(now_ms),
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.expire_key(key);
        self.entries.get(key).map(|entry| &entry.value)
    }

    pub fn has(&mut self, key: &K) -> bool {
        self.expire_key(key);
        self.entries.contains_key(key)
    }

    pub fn set(&mut self, key: K, value: V) {
        let expires_at_ms = (self.now_ms)().saturating_add((self.ttl_ms)());
        self.entries.insert(key, Entry { value, expires_at_ms });
    }

    pub fn delete(&mut self, key: &K) -> bool {
        self.entries.remove(key).is_some()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn expire_key(&mut self, key: &K) {
        let now = (self.now_ms)();
        let expired = self
            .entries
            .get(key)
            .is_some_and(|entry| entry.expires_at_ms <= now);
        if expired {
            self.entries.remove(key);
        }
    }
}

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    };

    #[test]
    fn expires_entries_and_supports_dynamic_ttl() {
        let now = Arc::new(AtomicU64::new(100));
        let ttl = Arc::new(AtomicU64::new(10));
        let mut cache = TtlCache::with_clock(
            {
                let ttl = ttl.clone();
                move || ttl.load(Ordering::Relaxed)
            },
            {
                let now = now.clone();
                move || now.load(Ordering::Relaxed)
            },
        );
        cache.set("a", 1);
        assert_eq!(cache.get(&"a"), Some(&1));
        now.store(110, Ordering::Relaxed);
        assert!(!cache.has(&"a"));

        ttl.store(50, Ordering::Relaxed);
        cache.set("b", 2);
        now.store(159, Ordering::Relaxed);
        assert!(cache.has(&"b"));
        now.store(160, Ordering::Relaxed);
        assert!(!cache.has(&"b"));
    }

    #[test]
    fn static_ttl_must_be_positive() {
        assert!(TtlCache::<String, String>::new(0).is_err());
    }
}
