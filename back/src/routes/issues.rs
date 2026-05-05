use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Pagination {
    pub page: usize,
    pub per_page: usize,
}
pub async fn list_issues(pagination: Query<Pagination>) {}
pub async fn create_issue() {}
pub async fn get_issue() {}
pub async fn list_pulls() {}
pub async fn repo_tree() {}
pub async fn view_file() {}
