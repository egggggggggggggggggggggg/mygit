pub mod routes;
use moka::future::Cache;
use sqlx::PgPool;
use std::path::PathBuf;
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub git_storage: PathBuf,
    pub jwt_secret: &'static [u8],
}
//Idea of how to structure the cache.
// pub struct CacheLayer {
//     commits: Cache<(RepoId, String), Vec<Commit>>,
//     trees: Cache<(RepoId, String, String), Tree>,
//     blobs: Cache<(RepoId, String), String>,
//     repo_meta: Cache<RepoId, RepoMeta>,
// }
