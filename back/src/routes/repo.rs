use crate::{AppState, routes::auth::AuthUser};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use gix::create::Kind;
use serde::Deserialize;
use std::fs;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct NewRepo {
    name: String,
    description: Option<String>,
    is_private: bool,
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
    //Might not wanna do * cause there might be sensitive data but this works for now.
    let rec = sqlx::query!(
        r#"
        SELECT r.*
        FROM repositories r
        WHERE r.owner_id = $1
          AND r.name = $2
          AND (
                r.is_private = false
                OR r.owner_id = $3
                OR EXISTS (
                    SELECT 1
                    FROM repository_collaborators rc
                    WHERE rc.repository_id = r.id
                      AND rc.user_id = $3
                )
          )
        "#,
        owner_id,
        repo_name,
        user_id
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(axum::http::StatusCode::NOT_FOUND)?;
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
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
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
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        // handle duplicate repo name per user
        if let sqlx::Error::Database(db_err) = &e
            && db_err.code().as_deref() == Some("23505")
        {
            return StatusCode::CONFLICT;
        }
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let repo_path = state
        .git_storage
        .join(user_id.to_string())
        .join(&payload.name);
    if init_bare_repo(&repo_path).is_err() {
        tx.rollback().await.ok();
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    tx.commit()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({
        "id": rec.id,
        "name": rec.name,
        "description": rec.description
    })))
}
#[axum::debug_handler]
pub async fn update_repo_metadata(
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
pub async fn list_commits() {}
fn init_bare_repo(path: &std::path::Path) -> Result<(), &'static str> {
    fs::create_dir_all(path).map_err(|_| "Failed to create the directory for the repo")?;
    gix::create::into(path, Kind::Bare, gix::create::Options::default())
        .map_err(|_| "failed to initialize bare repo")?;
    Ok(())
}
