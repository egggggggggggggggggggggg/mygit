use crate::{
    AppState, Pagination,
    routes::auth::{AuthUser, MaybeAuthUser},
    wraps::commits::commits_for_branch_paginated,
};
use axum::{
    Json,
    extract::{Path, Query, State},
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
}

#[derive(Deserialize)]
pub struct UpdateRepo {
    description: Option<String>,
}
///Can view without auth if repo is public.
pub async fn repo_home(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path((owner_name, repo_name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let user_id = maybe_claims.map(|c| c.sub);
    let owner_id = sqlx::query_scalar!(
        r#"
    SELECT username 
    FROM users
    WHERE id = $1



    "#,
        owner_name
    );
    //Might not wanna do * cause there might be sensitive data but this works for now.
    //Looks for the repository falling under an owner id 2
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
    let user_id = claims.sub;
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
    let user_id = claims.sub;
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
#[axum::debug_handler]
pub async fn list_commits(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path((repo_owner, repo_name)): Path<(String, String)>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let user_id = maybe_claims.map(|c| c.sub);
    // Access check
    let has_access = sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM repositories r
            JOIN users u ON u.id = r.owner_id
            WHERE u.username = $1
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
        )
        "#,
        repo_owner,
        repo_name,
        user_id
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?
    .unwrap_or(false);
    if !has_access {
        return Err(axum::http::StatusCode::NOT_FOUND); // GitHub-style
    }
    //Page should not be limited to 1  but the total amount of commits.
    //Repo meta data should return the amount of commits and issues which avoids having to process
    //all the repo metadata reading which is faster. Also check the in mem cache.
    let page = pagination.page.unwrap_or(1).max(1);
    let per_page = pagination.per_page.unwrap_or(20).clamp(1, 100);

    let path = state.git_storage.clone().join(&repo_owner).join(&repo_name);
    let repo = gix::open(path).unwrap();
    let commits = commits_for_branch_paginated(
        &repo, "main", // or resolve default branch properly
        page, per_page,
    )
    .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "commits": commits,
    })))
}
pub async fn repo_tree() {}
pub async fn view_file() {}
fn init_bare_repo(path: &std::path::Path) -> Result<(), &'static str> {
    fs::create_dir_all(path).map_err(|_| "Failed to create the directory for the repo")?;
    gix::create::into(path, Kind::Bare, gix::create::Options::default())
        .map_err(|_| "failed to initialize bare repo")?;
    Ok(())
}
