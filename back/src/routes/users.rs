use std::sync::Arc;

use axum::extract::{Path, State};

use crate::AppState;

pub async fn user_profile(State(t): State<Arc<AppState>>, Path(params): Path<String>) {
    sqlx::query!("SELECT * FROM users WHERE users.")
    sqlx::query!("SELECT * FROM users").fetch_all(&t.pool).await;
}
