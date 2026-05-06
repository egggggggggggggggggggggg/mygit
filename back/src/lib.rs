pub mod routes;
use gix::Commit;
use moka::future::Cache;
use sqlx::PgPool;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
pub struct AppState {
    pub pool: PgPool,
    pub git_storage: PathBuf,
    pub jwt_secret: &'static [u8],
    pub cache: CacheLayer,
}

fn memory_limit_bytes() -> Option<u64> {
    //This should
    #[cfg(target_os = "linux")]
    fs::read_to_string("/sys/fs/cgroup/memory.max")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
}

pub struct CacheLayer {
    trees: Cache<(uuid::Uuid, String, String), Tree>,
    blobs: Cache<(uuid::Uuid, String), String>,
    repo_meta: Cache<uuid::Uuid, RepoMeta>,
}
impl Default for CacheLayer {
    fn default() -> Self {
        //fallback of around a gig of memory if cant read
        let limit = memory_limit_bytes().unwrap_or(1_000_000);
        let cache_size = limit / 10; // 10% of available memory
        Self {
            trees: Cache::builder().max_capacity(cache_size / 4).build(),
            blobs: Cache::builder().max_capacity(cache_size / 4).build(),
            repo_meta: Cache::builder().max_capacity(cache_size / 4).build(),
        }
    }
}
#[derive(Clone)]
pub struct Tree {}
#[derive(Clone)]
pub struct RepoMeta {}
pub fn test() -> Result<(), anyhow::Error> {
    let repo = gix::open(".")?;
    let mut head = repo.head()?;
    let head_commit = head.peel_to_commit()?;
    let mut revwalk = repo.rev_walk([head_commit.id]);
    Ok(())
}
