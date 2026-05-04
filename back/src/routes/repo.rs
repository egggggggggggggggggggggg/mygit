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

#[axum::debug_handler]
pub async fn repo_home(Extension(state): Extension<Arc<AppState>>, Path(repo_name): Path<String>) {
    let pool = &state.pool;
    let rec = sqlx::query!(
        r#"
        SELECT id, owner_id 
        FROM repositories
        WHERE name = $1
        "#,
        repo_name
    )
    .fetch_one(pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR);
}
#[axum::debug_handler]
pub async fn create_repo(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<NewRepo>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = &state.pool;

    let rec = sqlx::query!(
        r#"
        INSERT INTO repositories (name, description)
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
#[axum::debug_handler]
pub async fn update_repo(
    Extension(state): Extension<Arc<AppState>>,
    Path(repo_name): axum::extract::Path<String>,
    Json(payload): Json<UpdateRepo>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let pool = &state.pool;

    let rec = sqlx::query!(
        r#"
        UPDATE repositories 
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
