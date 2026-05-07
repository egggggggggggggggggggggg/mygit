pub mod routes;
pub mod wraps;

use moka::future::Cache;
use serde::Deserialize;
use sqlx::PgPool;
use std::{fs, mem::size_of, path::PathBuf, sync::Arc, time::Duration};

pub struct AppState {
    pub pool: PgPool,
    pub git_storage: PathBuf,
    pub jwt_secret: &'static [u8],
    pub cache: CacheLayer,
}

#[derive(Clone)]
pub struct Tree {}
#[derive(Clone)]
pub struct RepoMeta {}

///Horribly designed with the keys, should replace with referential types
pub struct CacheLayer {
    pub trees: Cache<TreeKey, Arc<Tree>>,
    pub blobs: Cache<BlobKey, Arc<String>>,
    pub repo_meta: Cache<uuid::Uuid, Arc<RepoMeta>>,
}

type TreeKey = (uuid::Uuid, String, String);
type BlobKey = (uuid::Uuid, String);

impl CacheLayer {
    pub fn new() -> Self {
        let memory_limit = memory_limit_bytes().unwrap_or(1024 * 1024 * 1024); // 1 GiB fallback

        // Use at most 10% of available memory.
        let total_cache_budget = memory_limit / 10;

        // Split intentionally:
        // blobs dominate memory usage.
        let blob_budget = total_cache_budget * 70 / 100;
        let tree_budget = total_cache_budget * 20 / 100;
        let meta_budget = total_cache_budget * 10 / 100;

        let trees = Cache::builder()
            .max_capacity(tree_budget)
            .weigher(|_k: &TreeKey, v: &Arc<Tree>| -> u32 { estimate_tree_size(v) })
            .time_to_idle(Duration::from_mins(10))
            .build();

        let blobs = Cache::builder()
            .max_capacity(blob_budget)
            .weigher(|k: &BlobKey, v: &Arc<String>| -> u32 { estimate_blob_size(k, v) })
            .time_to_idle(Duration::from_mins(5))
            .build();

        let repo_meta = Cache::builder()
            .max_capacity(meta_budget)
            .weigher(|_k: &uuid::Uuid, _v: &Arc<RepoMeta>| -> u32 { size_of::<RepoMeta>() as u32 })
            .time_to_live(Duration::from_hours(1))
            .build();

        Self {
            trees,
            blobs,
            repo_meta,
        }
    }
}

impl Default for CacheLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads container memory limit on Linux.
///
/// Handles:
/// - cgroup v2 (`memory.max`)
/// - "max" sentinel
#[cfg(target_os = "linux")]
fn memory_limit_bytes() -> Option<u64> {
    let raw = fs::read_to_string("/sys/fs/cgroup/memory.max").ok()?;
    let trimmed = raw.trim();
    if trimmed == "max" {
        return None;
    }
    trimmed.parse::<u64>().ok()
}
fn estimate_blob_size(k: &BlobKey, v: &Arc<String>) -> u32 {
    let key_size = size_of::<uuid::Uuid>() + k.1.len();
    let value_size = size_of::<String>() + v.len();
    (key_size + value_size) as u32
}
///MUST REPLACE
const fn estimate_tree_size(_tree: &Arc<Tree>) -> u32 {
    // Replace with something real if Tree grows.
    8 * 1024
}
#[derive(Deserialize)]
pub struct Pagination {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}
