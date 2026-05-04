use std::path::PathBuf;

use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub git_storage: PathBuf,
}
pub mod routes;
