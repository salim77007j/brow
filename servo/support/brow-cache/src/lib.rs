/* brow-cache — the memory layer of brow's <100 MB/tab strategy.
 *
 * Two cooperating structures:
 *
 * 1. `DiskCache` — a content-addressed, mmap-backed HTTP resource cache.
 *    Bodies live in fanout files (aa/bb/<sha256>), indexed by an in-memory
 *    LRU map persisted as JSON. Reads use `memmap2` so hot bodies are served
 *    by the page cache without copies into heap memory, and eviction drops
 *    files without touching their (possibly shared) mappings until last use.
 *
 * 2. `DedupPool` — a process-wide pool of immutable `Arc<Bytes>` bodies keyed
 *    by content hash. Identical subresources (logos, frameworks, images)
 *    across N tabs cost 1× memory instead of N×. Entries vanish when the last
 *    reference is dropped, so idle memory returns to the OS immediately.
 */

pub mod dedup;
pub mod disk;

pub use dedup::DedupPool;
pub use disk::{CacheError, DiskCache, DiskCacheStats};
