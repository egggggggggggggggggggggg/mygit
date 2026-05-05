use crate::{AppState, routes::auth::AuthUser};
use axum::{
    Json,
    extract::{Path, State},
};
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
pub async fn repo_home(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path((owner_id, repo_name)): Path<(uuid::Uuid, String)>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let user_id = claims.sub.parse::<uuid::Uuid>().unwrap();

    let rec = sqlx::query!(
        r#"
        SELECT id, owner_id, is_private
        FROM repositories
        WHERE owner_id = $1 AND name = $2
        "#,
        owner_id,
        repo_name
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::NOT_FOUND)?;

    if rec.is_private && rec.owner_id != user_id {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }

    Ok(Json(serde_json::json!({
        "id": rec.id
    })))
}
#[axum::debug_handler]
pub async fn create_repo(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NewRepo>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let user_id = claims
        .sub
        .parse::<uuid::Uuid>()
        .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?;

    let rec = sqlx::query!(
        r#"
        INSERT INTO repositories (owner_id, name, description)
        VALUES ($1, $2, $3)
        RETURNING id, name, description
        "#,
        user_id,
        payload.name,
        payload.description
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        // handle duplicate repo name per user
        if let sqlx::Error::Database(db_err) = &e
            && db_err.code().as_deref() == Some("23505")
        {
            return axum::http::StatusCode::CONFLICT;
        }
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(serde_json::json!({
        "id": rec.id,
        "name": rec.name,
        "description": rec.description
    })))
}
#[axum::debug_handler]
pub async fn update_repo(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(repo_name): Path<String>,
    Json(payload): Json<UpdateRepo>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let user_id = claims
        .sub
        .parse::<uuid::Uuid>()
        .map_err(|_| axum::http::StatusCode::UNAUTHORIZED)?;

    let rec = sqlx::query!(
        r#"
        UPDATE repositories 
        SET description = $1
        WHERE owner_id = $2 AND name = $3
        RETURNING id, name, description
        "#,
        payload.description,
        user_id,
        repo_name
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    let rec = rec.ok_or(axum::http::StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "id": rec.id,
        "name": rec.name,
        "description": rec.description
    })))
}
