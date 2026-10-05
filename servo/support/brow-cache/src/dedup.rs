/* Cross-tab resource dedup pool.
 *
 * Key idea: N tabs requesting the same 300 KiB logo cost 300 KiB, not
 * 300*N KiB. Bodies are immutable `Arc<bytes::Bytes>`-like buffers keyed by
 * content hash. When the last tab drops its reference the buffer is freed.
 *
 * We avoid pulling `bytes` in and use `Arc<Vec<u8>>` (clone = atomic refcount
 * bump). The pool holds `Arc` clones of its own so a body stays hot while any
 * tab (or the disk cache path) may want it, plus LRU accounting for the pool's
 * *retained* references only — the pool can be configured to stop retaining
 * under a byte budget without affecting live tab references.
 */

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Immutable shared body.
pub type SharedBody = Arc<Vec<u8>>;

struct PoolInner {
    map: HashMap<String, SharedBody>,
    /// Insertion order for LRU-ish eviction of *retained* entries.
    order: Vec<String>,
    retained_bytes: u64,
    max_retained_bytes: u64,
    hits: u64,
    misses: u64,
    inserts: u64,
}

impl Default for PoolInner {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::new(),
            retained_bytes: 0,
            max_retained_bytes: u64::MAX,
            hits: 0,
            misses: 0,
            inserts: 0,
        }
    }
}

impl std::fmt::Debug for PoolInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolInner")
            .field("entries", &self.map.len())
            .field("retained_bytes", &self.retained_bytes)
            .field("max_retained_bytes", &self.max_retained_bytes)
            .field("hits", &self.hits)
            .field("misses", &self.misses)
            .field("inserts", &self.inserts)
            .finish()
    }
}

/// Thread-safe (the pool is shared by all tab loaders on different threads).
#[derive(Debug, Default)]
pub struct DedupPool {
    inner: Mutex<PoolInner>,
}

impl DedupPool {
    pub fn new(max_retained_bytes: u64) -> Self {
        Self {
            inner: Mutex::new(PoolInner {
                map: HashMap::new(),
                order: Vec::new(),
                retained_bytes: 0,
                max_retained_bytes,
                hits: 0,
                misses: 0,
                inserts: 0,
            }),
        }
    }

    /// Get a body if the pool has it. Returns a cheap Arc clone.
    pub fn get(&self, key: &str) -> Option<SharedBody> {
        let mut inner = self.inner.lock().unwrap();
        if inner.map.contains_key(key) {
            inner.hits += 1;
            return Some(Arc::clone(&inner.map[key]));
        }
        inner.misses += 1;
        None
    }

    /// Insert a body under `key`. If present, returns the existing copy
    /// (the caller should use it instead of its own allocation).
    pub fn insert(&self, key: &str, body: Vec<u8>) -> SharedBody {
        let mut inner = self.inner.lock().unwrap();
        if inner.map.contains_key(key) {
            inner.hits += 1;
            return Arc::clone(&inner.map[key]);
        }
        let shared: SharedBody = Arc::new(body);
        let size = shared.len() as u64;
        inner.map.insert(key.to_string(), Arc::clone(&shared));
        inner.order.push(key.to_string());
        inner.retained_bytes += size;
        inner.inserts += 1;

        // Evict retained copies (oldest first) over budget. Live references
        // from tabs keep their buffers alive regardless.
        while inner.retained_bytes > inner.max_retained_bytes && !inner.order.is_empty() {
            let oldest = inner.order.remove(0);
            if let Some(evicted) = inner.map.remove(&oldest) {
                inner.retained_bytes = inner.retained_bytes.saturating_sub(evicted.len() as u64);
            }
        }
        shared
    }

    /// Fetch-or-insert: dedup fast path for loaders.
    pub fn get_or_insert(&self, key: &str, body: Vec<u8>) -> SharedBody {
        if let Some(existing) = self.get(key) {
            return existing;
        }
        self.insert(key, body)
    }

    /// Explicit removal (e.g. cache invalidation). Live tab references are
    /// unaffected — they hold their own Arcs.
    pub fn invalidate(&self, key: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if let Some(body) = inner.map.remove(key) {
            inner.retained_bytes = inner.retained_bytes.saturating_sub(body.len() as u64);
            inner.order.retain(|k| k != key);
            true
        } else {
            false
        }
    }

    pub fn stats(&self) -> DedupStats {
        let inner = self.inner.lock().unwrap();
        DedupStats {
            entries: inner.map.len(),
            retained_bytes: inner.retained_bytes,
            hits: inner.hits,
            misses: inner.misses,
            inserts: inner.inserts,
        }
    }

    /// Number of live Arc references for a key (1 = only the pool retains it).
    /// Useful in tests and the memory dashboard.
    pub fn strong_count(&self, key: &str) -> usize {
        let inner = self.inner.lock().unwrap();
        inner.map.get(key).map(Arc::strong_count).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DedupStats {
    pub entries: usize,
    pub retained_bytes: u64,
    pub hits: u64,
    pub misses: u64,
    pub inserts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_content_is_identical_arc() {
        let pool = DedupPool::new(1 << 20);
        let key = "k-logo";
        let a = pool.get_or_insert(key, vec![7u8; 1000]);
        let b = pool.get_or_insert(key, vec![0u8; 0]); // second insert ignored
        assert!(Arc::ptr_eq(&a, &b));
        let stats = pool.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.inserts, 1);
        assert_eq!(stats.retained_bytes, 1000);
    }

    #[test]
    fn reference_counting_tracks_tab_clones() {
        let pool = DedupPool::new(1 << 20);
        let key = "k";
        let body = pool.insert(key, vec![0u8; 100]);
        assert_eq!(pool.strong_count(key), 2); // pool + caller

        let tab1 = Arc::clone(&body);
        let tab2 = Arc::clone(&body);
        assert_eq!(pool.strong_count(key), 4);

        drop(tab1);
        drop(tab2);
        drop(body);
        assert_eq!(pool.strong_count(key), 1); // only the pool
    }

    #[test]
    fn budget_evicts_retained_only() {
        let pool = DedupPool::new(1000);
        // 3 bodies of 600 bytes each → only ~1.6 fit; LRU evicts oldest.
        let k1 = pool.insert("k1", vec![1u8; 600]);
        let k2 = pool.insert("k2", vec![2u8; 600]);
        let k3 = pool.insert("k3", vec![3u8; 600]);

        let stats = pool.stats();
        assert!(stats.retained_bytes <= 1200 + 600, "retained {}", stats.retained_bytes);
        // k1 was evicted from the pool's map...
        assert!(pool.get("k1").is_none());
        // ...but the caller's Arc keeps the data alive.
        assert_eq!(k1.as_slice(), vec![1u8; 600].as_slice());
        assert_eq!(k2.as_slice(), vec![2u8; 600].as_slice());
        assert_eq!(k3.as_slice(), vec![3u8; 600].as_slice());
    }

    #[test]
    fn invalidate_preserves_live_references() {
        let pool = DedupPool::new(1 << 20);
        let key = "css";
        let body = pool.insert(key, b"body{}".to_vec());
        let tab_ref = Arc::clone(&body);

        assert!(pool.invalidate(key));
        assert!(pool.get(key).is_none());
        // The tab still holds valid data.
        assert_eq!(tab_ref.as_slice(), b"body{}");
        assert!(pool.stats().retained_bytes == 0);
    }

    #[test]
    fn hit_miss_stats() {
        let pool = DedupPool::new(1 << 20);
        assert!(pool.get("none").is_none());
        pool.insert("k", vec![0; 10]);
        pool.get("k").unwrap();
        let s = pool.stats();
        assert_eq!((s.misses, s.hits), (1, 1));
    }

    #[test]
    fn concurrent_access_smoke() {
        use std::sync::Arc as StdArc;
        let pool = StdArc::new(DedupPool::new(1 << 20));
        let mut handles = Vec::new();
        for t in 0..8u32 {
            let pool = StdArc::clone(&pool);
            handles.push(std::thread::spawn(move || {
                for i in 0..1000u32 {
                    let key = format!("shared-{}", i % 10);
                    let body = pool.get_or_insert(&key, vec![(t + i) as u8; 64]);
                    assert_eq!(body.len(), 64);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let stats = pool.stats();
        assert_eq!(stats.entries, 10);
        assert!(stats.hits > 0);
    }
}
