use crate::{
    AppState, Pagination,
    errors::ApiError,
    routes::{
        auth::{AuthUser, MaybeAuthUser},
        has_access,
    },
    wraps::{
        commits::commits_for_branch_paginated,
        files::{Node, get_tree},
        read_file_at_commit,
    },
};
use axum::{
    Json,
    body::Body,
    extract::{Path, Query, State},
    http::Response,
};
use gix::{ObjectId, create::Kind};
use reqwest::{StatusCode, header};
use serde::Deserialize;
use std::fs;
use std::sync::Arc;

pub enum RepoErrors {
    NotFound,
    MissingHead,
}

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
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = maybe_claims.map(|c| c.sub);
    let rec = sqlx::query!(
        r#"
        SELECT r.*
        FROM repositories r
        INNER JOIN users u
            ON u.id = r.owner_id
            AND u.username = $1
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
        owner_name,
        repo_name,
        user_id
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::RepoNotFound)?;
    Ok(Json(serde_json::json!({
        "id": rec.id
    })))
}
#[axum::debug_handler]
pub async fn create_repo(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<NewRepo>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = claims.sub;
    let mut tx = state.pool.begin().await.map_err(|_| ApiError::Internal)?;
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
            return ApiError::RepoAlreadyExists;
        }
        ApiError::Database
    })?;
    let repo_path = state
        .git_storage
        .join(user_id.to_string())
        .join(&payload.name);
    if let Err(_e) = init_bare_repo(&repo_path) {
        tx.rollback().await.ok();
        return Err(ApiError::Git);
    }
    tx.commit().await.map_err(|_| ApiError::Internal)?;
    Ok(Json(serde_json::json!({
        "id": rec.id,
        "name": rec.name,
        "description": rec.description,
    })))
}
#[axum::debug_handler]
pub async fn update_repo_metadata(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Path(repo_name): Path<String>,
    Json(payload): Json<UpdateRepo>,
) -> Result<Json<serde_json::Value>, ApiError> {
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
    .await?;
    let rec = rec.ok_or(ApiError::Unauthorized)?;
    Ok(Json(serde_json::json!({
        "id": rec.id,
        "name": rec.name,
        "description": rec.description
    })))
}
pub async fn list_commits(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path((repo_owner, repo_name)): Path<(String, String)>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let user_id = maybe_claims.map(|c| c.sub);
    // Access check
    if !has_access(&state.pool, &repo_owner, &repo_name, user_id).await? {
        return Err(ApiError::Unauthorized);
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
    .map_err(|_| ApiError::CommitListingFailed)?;
    Ok(Json(serde_json::json!({
        "commits": commits,
    })))
}
///A lot of these are gix errors cuz gix doesn't define a centralized error enum.
#[derive(Deserialize)]
pub struct InnerRoute {
    owner: String,
    name: String,
    path: String,
    id: ObjectId,
}
#[axum::debug_handler]
pub async fn repo_tree(
    MaybeAuthUser(claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path(route): Path<InnerRoute>,
) -> Result<Json<Node>, ApiError> {
    let user_id = claims.map(|c| c.sub);
    if !has_access(&state.pool, &route.owner, &route.name, user_id).await? {
        return Err(ApiError::Unauthorized);
    }
    let path = state.git_storage.join(&route.owner).join(&route.name);
    let repo = gix::open(&path).map_err(|_| ApiError::RepoNotFound)?;
    Ok(Json(
        get_tree(&repo, route.id, "").map_err(|_| ApiError::Internal)?,
    ))
}
pub async fn view_file(
    MaybeAuthUser(claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path(route): Path<InnerRoute>,
) -> Result<Response<Body>, ApiError> {
    let user_id = claims.map(|c| c.sub);
    if !has_access(&state.pool, &route.owner, &route.name, user_id).await? {
        return Err(ApiError::Unauthorized);
    }
    let path = state.git_storage.join(&route.owner).join(&route.name);
    let repo = gix::open(&path).map_err(|_| ApiError::RepoNotFound)?;
    let file =
        read_file_at_commit(&repo, route.id, &route.path).map_err(|_| ApiError::RepoNotFound)?;
    let body = Body::from(file);
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(body)
        .unwrap())
}
fn init_bare_repo(path: &std::path::Path) -> Result<(), anyhow::Error> {
    fs::create_dir_all(path)?;
    gix::create::into(path, Kind::Bare, gix::create::Options::default())?;
    Ok(())
}
