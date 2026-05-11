use crate::{
    AppState, Pagination,
    errors::ApiError,
    routes::auth::{AuthUser, MaybeAuthUser},
    wraps::{commits::commits_for_branch_paginated, read_file_at_commit},
};
use axum::{
    Json,
    extract::{Path, Query, State, multipart},
};
use gix::{ObjectId, create::Kind};
use serde::Deserialize;
use sqlx::PgPool;
use std::fs;
use std::sync::Arc;
use uuid::Uuid;

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
use thiserror::Error;

///A lot of these are gix errors cuz gix doesn't define a centralized error enum.
#[derive(Debug, Error)]
pub enum GixError {
    #[error("failed to open repository")]
    OpenRepo(#[from] gix::open::Error),

    #[error("failed to read HEAD")]
    ReadHead(#[from] gix::reference::find::existing::Error),

    #[error("failed to resolve revision")]
    ResolveRev(#[from] gix::revision::spec::parse::single::Error),

    #[error("failed to peel to commit")]
    PeelCommit(#[from] gix::head::peel::to_commit::Error),

    #[error("failed to peel to object")]
    PeelObject(#[from] gix::head::peel::to_object::Error),

    #[error("failed to acquire specified object")]
    AcquireObject(#[from] gix::object::commit::Error),

    #[error("failed to lookup")]
    Lookup(#[from] gix::objs::find::existing::Error),

    #[error("item does not exist")]
    MissingItem,
}
#[derive(Deserialize)]
pub struct InnerRoute {
    owner: String,
    name: String,
    branch: String,
    path: String,
}

pub async fn repo_tree(
    MaybeAuthUser(claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path(route): Path<InnerRoute>,
) {
    let user_id = claims.map(|c| c.sub);
}
pub async fn has_access(
    pool: &PgPool,
    repo_owner: &str,
    repo_name: &str,
    user_id: Option<Uuid>,
) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar!(
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
    .fetch_one(pool)
    .await?
    .unwrap_or(false))
}
pub async fn view_file(
    MaybeAuthUser(claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path((route, id)): Path<(InnerRoute, ObjectId)>,
) -> Result<(), ApiError> {
    let user_id = claims.map(|c| c.sub);
    if !has_access(&state.pool, &route.owner, &route.name, user_id).await? {
        return Err(ApiError::Unauthorized);
    }
    let path = state.git_storage.join(&route.owner).join(&route.name);
    let repo = gix::open(&path).map_err(|_| ApiError::RepoNotFound)?;
    let file = read_file_at_commit(&repo, id, &route.path).map_err(|_| ApiError::RepoNotFound)?;
    let stream = FramedRead::new(file, BytesCodec::new());
    let body = reqwest::Body::wrap_stream(stream);
    let part = reqwest::multipart::Part::stream(value)
    reqwest::multipart::Form::new().part(file, part);

    Ok(())
}
fn init_bare_repo(path: &std::path::Path) -> Result<(), anyhow::Error> {
    fs::create_dir_all(path)?;
    gix::create::into(path, Kind::Bare, gix::create::Options::default())?;
    Ok(())
}
