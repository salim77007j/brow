/* Content-addressed, mmap-backed disk cache with LRU eviction. */

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("key not found")]
    NotFound,
    #[error("value too large for cache: {0} bytes")]
    TooLarge(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntryMeta {
    /// Size in bytes.
    pub size: u64,
    /// Last-access time (nanos since epoch) for LRU.
    pub last_access: u64,
    /// Optional content-type for the serving layer.
    pub content_type: Option<String>,
}

/// Opaque handle to a mapped body. The mapping stays valid (private mmap of a
/// file we own) until dropped.
pub struct MappedBody {
    mmap: Mmap,
    pub content_type: Option<String>,
}

impl MappedBody {
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }

    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DiskCacheStats {
    pub entries: u64,
    pub total_bytes: u64,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

/// Disk cache rooted at a directory:
/// ```text
/// <root>/data/aa/<sha256>
/// <root>/index.json
/// ```
pub struct DiskCache {
    root: PathBuf,
    index: HashMap<String, EntryMeta>,
    max_bytes: u64,
    stats: DiskCacheStats,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// LRU stamp with sub-second precision (nanos since epoch). Second-granularity
/// stamps make touches within the same second indistinguishable, which breaks
/// deterministic eviction.
fn now_stamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

pub fn content_key(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut hex = String::with_capacity(64);
    for b in digest {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

impl DiskCache {
    /// Open (or initialize) a cache with a byte budget. Entries beyond the
    /// budget are evicted LRU-first on the next `put`.
    pub fn open(root: &Path, max_bytes: u64) -> Result<Self, CacheError> {
        fs::create_dir_all(root.join("data"))?;
        let index_path = root.join("index.json");
        let index: HashMap<String, EntryMeta> = if index_path.exists() {
            let raw = fs::read_to_string(&index_path)?;
            serde_json::from_str(&raw)?
        } else {
            HashMap::new()
        };
        let mut cache = Self {
            root: root.to_path_buf(),
            index,
            max_bytes,
            stats: DiskCacheStats::default(),
        };
        cache.prune_missing_files();
        Ok(cache)
    }

    fn data_path(&self, key: &str) -> PathBuf {
        // Fanout: data/aa/ab/<key> (two levels) to keep dirs small.
        self.root.join("data").join(&key[0..2]).join(&key[2..4]).join(key)
    }

    fn prune_missing_files(&mut self) {
        let root = self.root.clone();
        let dead: Vec<String> = self
            .index
            .keys()
            .filter(|key| {
                let path = root
                    .join("data")
                    .join(&key[0..2])
                    .join(&key[2..4])
                    .join(key.as_str());
                !path.exists()
            })
            .cloned()
            .collect();
        for key in dead {
            self.index.remove(&key);
        }
    }

    pub fn key_for(&self, data: &[u8]) -> String {
        content_key(data)
    }

    /// Insert (or refresh) a body. Returns the content key.
    pub fn put(&mut self, data: &[u8], content_type: Option<&str>) -> Result<String, CacheError> {
        if data.len() as u64 > self.max_bytes {
            return Err(CacheError::TooLarge(data.len()));
        }
        let key = content_key(data);
        let path = self.data_path(&key);
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            // Write temp + rename: a crash never leaves a half-written body.
            let tmp = path.with_extension("part");
            let mut f = File::create(&tmp)?;
            f.write_all(data)?;
            f.sync_all()?;
            fs::rename(&tmp, &path)?;
        }
        let size = data.len() as u64;
        self.index.insert(
            key.clone(),
            EntryMeta {
                size,
                last_access: now_stamp(),
                content_type: content_type.map(str::to_string),
            },
        );
        self.evict_if_needed();
        self.persist_index()?;
        Ok(key)
    }

    /// Fetch a body as an mmap. Touches LRU state.
    pub fn get(&mut self, key: &str) -> Result<MappedBody, CacheError> {
        let meta = self.index.get_mut(key).ok_or(CacheError::NotFound)?;
        meta.last_access = now_stamp();
        self.stats.hits += 1;
        let path = self.data_path(key);
        let file = File::open(&path).map_err(|_| CacheError::NotFound)?;
        // Safety: we only map files this cache wrote and never mutate them
        // in place (writes go through temp+rename), so no concurrent
        // modification of the mapping's backing file occurs.
        let mmap = unsafe { Mmap::map(&file)? };
        let content_type = self
            .index
            .get(key)
            .and_then(|m| m.content_type.clone());
        Ok(MappedBody {
            mmap,
            content_type,
        })
    }

    pub fn contains(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    pub fn remove(&mut self, key: &str) -> bool {
        if self.index.remove(key).is_some() {
            let _ = fs::remove_file(self.data_path(key));
            let _ = self.persist_index();
            true
        } else {
            false
        }
    }

    /// Drop everything.
    pub fn clear(&mut self) -> Result<(), CacheError> {
        self.index.clear();
        let data_dir = self.root.join("data");
        if data_dir.exists() {
            fs::remove_dir_all(&data_dir)?;
            fs::create_dir_all(&data_dir)?;
        }
        self.persist_index()?;
        Ok(())
    }

    fn total_bytes(&self) -> u64 {
        self.index.values().map(|m| m.size).sum()
    }

    fn evict_if_needed(&mut self) {
        let mut total = self.total_bytes();
        if total <= self.max_bytes {
            return;
        }
        // LRU: oldest last_access first.
        let mut by_age: Vec<(String, u64, u64)> = self
            .index
            .iter()
            .map(|(k, m)| (k.clone(), m.last_access, m.size))
            .collect();
        by_age.sort_by_key(|(_, access, _)| *access);
        for (key, _, size) in by_age {
            if total <= self.max_bytes {
                break;
            }
            if self.index.remove(&key).is_some() {
                let _ = fs::remove_file(self.data_path(&key));
                total = total.saturating_sub(size);
                self.stats.evictions += 1;
            }
        }
    }

    fn persist_index(&self) -> Result<(), CacheError> {
        let path = self.root.join("index.json");
        let tmp = self.root.join("index.json.tmp");
        let json = serde_json::to_vec(&self.index)?;
        let mut f = File::create(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn stats(&self) -> DiskCacheStats {
        DiskCacheStats {
            entries: self.index.len() as u64,
            total_bytes: self.total_bytes(),
            ..self.stats
        }
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn set_max_bytes(&mut self, max_bytes: u64) {
        self.max_bytes = max_bytes;
        self.evict_if_needed();
        let _ = self.persist_index();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache() -> (tempfile::TempDir, DiskCache) {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::open(dir.path(), 1 << 20).unwrap(); // 1 MiB
        (dir, cache)
    }

    #[test]
    fn put_get_roundtrip_via_mmap() {
        let (_dir, mut cache) = cache();
        let body = vec![42u8; 100_000];
        let key = cache.put(&body, Some("image/png")).unwrap();

        let mapped = cache.get(&key).unwrap();
        assert_eq!(mapped.len(), 100_000);
        assert_eq!(mapped.content_type.as_deref(), Some("image/png"));
        assert!(mapped.as_bytes().iter().all(|&b| b == 42));
    }

    #[test]
    fn content_addressing_dedupes_writes() {
        let (_dir, mut cache) = cache();
        let body = b"identical body".to_vec();
        let k1 = cache.put(&body, None).unwrap();
        let k2 = cache.put(&body, None).unwrap();
        assert_eq!(k1, k2);
        assert_eq!(cache.stats().entries, 1);
    }

    #[test]
    fn miss_and_contains() {
        let (_dir, mut cache) = cache();
        assert!(!cache.contains("deadbeef"));
        assert!(matches!(cache.get("deadbeef"), Err(CacheError::NotFound)));
    }

    #[test]
    fn lru_eviction_respects_budget() {
        let dir = tempfile::tempdir().unwrap();
        // Budget fits exactly 4 x 100 KiB bodies.
        let mut cache = DiskCache::open(dir.path(), 400 * 1024).unwrap();
        let body = vec![1u8; 100 * 1024];
        let mut keys: Vec<String> = Vec::new();
        for i in 0..6u8 {
            let mut b = body.clone();
            b[0] = i;
            keys.push(cache.put(&b, None).unwrap());
            if i == 3 {
                // Touch key[1] so it becomes more recently used than 2 and 3.
                cache.get(&keys[1]).unwrap();
            }
        }

        let stats = cache.stats();
        assert!(stats.total_bytes <= 400 * 1024, "budget exceeded: {}", stats.total_bytes);
        assert!(stats.evictions >= 2);
        // key[0] (oldest, untouched) must have been evicted first.
        assert!(!cache.contains(&keys[0]));
        // key[1] (touched after key[0..=3] were written) must survive.
        assert!(cache.contains(&keys[1]));
        assert!(cache.get(&keys[1]).is_ok());
    }

    #[test]
    fn too_large_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = DiskCache::open(dir.path(), 1024).unwrap();
        let big = vec![0u8; 4096];
        assert!(matches!(cache.put(&big, None), Err(CacheError::TooLarge(4096))));
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let key = {
            let mut cache = DiskCache::open(dir.path(), 1 << 20).unwrap();
            cache.put(b"persisted body", Some("text/plain")).unwrap()
        };
        let mut cache = DiskCache::open(dir.path(), 1 << 20).unwrap();
        assert!(cache.contains(&key));
        let mapped = cache.get(&key).unwrap();
        assert_eq!(mapped.as_bytes(), b"persisted body");
        assert_eq!(mapped.content_type.as_deref(), Some("text/plain"));
    }

    #[test]
    fn remove_and_clear() {
        let (_dir, mut cache) = cache();
        let key = cache.put(b"remove me", None).unwrap();
        assert!(cache.remove(&key));
        assert!(!cache.contains(&key));
        assert!(matches!(cache.get(&key), Err(CacheError::NotFound)));

        cache.put(b"one", None).unwrap();
        cache.put(b"two", None).unwrap();
        cache.clear().unwrap();
        assert_eq!(cache.stats().entries, 0);
    }

    #[test]
    fn set_max_bytes_triggers_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = DiskCache::open(dir.path(), 10 * 1024 * 1024).unwrap();
        let keys: Vec<String> = (0..4u8)
            .map(|i| {
                let mut b = vec![2u8; 100 * 1024];
                b[0] = i;
                cache.put(&b, None).unwrap()
            })
            .collect();
        cache.set_max_bytes(250 * 1024);
        let stats = cache.stats();
        assert!(stats.total_bytes <= 250 * 1024);
        // Newest entries survive.
        assert!(cache.contains(&keys[3]));
    }

    #[test]
    fn mmap_read_matches_file_content() {
        let (_dir, mut cache) = cache();
        let body: Vec<u8> = (0..250_000u32).map(|i| (i % 251) as u8).collect();
        let key = cache.put(&body, None).unwrap();
        let mapped = cache.get(&key).unwrap();
        assert_eq!(mapped.as_bytes().len(), body.len());
        // Spot-check + full compare via reader over the mapping.
        assert_eq!(&mapped.as_bytes()[..16], &body[..16]);
        let mut from_map = Vec::new();
        from_map.extend_from_slice(mapped.as_bytes());
        assert_eq!(from_map, body);
    }
}
