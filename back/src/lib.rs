pub mod app_routes;
pub mod errors;
pub mod wraps;

use serde::Deserialize;
use sqlx::PgPool;
use std::path::PathBuf;

pub struct AppState {
    pub pool: PgPool,
    pub git_storage: PathBuf,
    pub file_storage: PathBuf,
    pub jwt_secret: &'static [u8],
}
#[derive(Deserialize)]
pub struct Pagination {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}
