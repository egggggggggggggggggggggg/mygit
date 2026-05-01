use crate::AppState;
use axum::{Extension, Json, extract::Path};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct NewRepo {
    name: String,
    description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRepo {
    description: Option<String>,
}
pub async fn repo_home() {}
pub async fn create_repo(
    Json(payload): Json<NewRepo>,
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = &state.pool;

    let rec = sqlx::query!(
        r#"
        INSERT INTO repos (name, description)
        VALUES ($1, $2)
        RETURNING id, name, description
        "#,
        payload.name,
        payload.description
    )
    .fetch_one(pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "id": rec.id,
        "name": rec.name,
        "description": rec.description
    })))
}

pub async fn update_repo(
    Path(repo_name): axum::extract::Path<String>,
    Json(payload): Json<UpdateRepo>,
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = &state.pool;

    let rec = sqlx::query!(
        r#"
        UPDATE repos
        SET description = $1
        WHERE name = $2
        RETURNING id, name, description
        "#,
        payload.description,
        repo_name
    )
    .fetch_one(pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "id": rec.id,
        "name": rec.name,
        "description": rec.description
    })))
}

