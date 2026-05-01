use axum::extract::State;

use crate::AppState;

pub async fn user_profile(State(t): State<AppState>) {
    sqlx::query!("SELECT * FROM users").fetch_all(&t.pool).await;
}
