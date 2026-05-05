pub mod routes;
use sqlx::PgPool;
use std::path::PathBuf;
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub git_storage: PathBuf,
    pub jwt_secret: &'static [u8],
}
