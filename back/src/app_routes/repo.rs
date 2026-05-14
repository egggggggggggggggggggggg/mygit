use crate::{
    AppState, Pagination,
    app_routes::{
        auth::{AuthUser, MaybeAuthUser},
        has_access,
    },
    errors::ApiError,
    wraps::{
        CommitInfo,
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
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Arc;
use utoipa::ToSchema;

pub enum RepoErrors {
    NotFound,
    MissingHead,
}

#[derive(Deserialize, ToSchema)]
pub struct NewRepo {
    name: String,
    description: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateRepo {
    description: Option<String>,
}
#[utoipa::path(
    get,
    path = "/{username}/{repo}",
    tag = "repositories",
    params(
        ("owner_name" = String, Path, description = "Repository owner username"),
        ("repo_name" = String, Path, description = "Repository name")
    ),
    // public or authenticated access (optional auth)
    security(("bearerAuth" = [])),
    responses(
        (status = 200, description = "Repository summary", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Repository not found"),
        (status = 500, description = "Internal server error")
    )
)]
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
#[utoipa::path(
    post,
    path = "/user/repos",
    tag = "repositories",
    security(("bearerAuth" = [])),
    request_body(content = NewRepo),
    responses(
        (status = 200, description = "Created repository", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
        (status = 409, description = "Repository already exists"),
        (status = 500, description = "Internal server error")
    )
)]
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
    let repo_path = &state
        .git_storage
        .join(user_id.to_string())
        .join(&payload.name);
    if let Err(_e) = init_bare_repo(repo_path) {
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
#[derive(Serialize, Deserialize, Debug, ToSchema)]
pub struct DeletionRequest {
    force: bool,
    repo_name: String,
    username: String,
}
#[utoipa::path(
    delete,
    path = "/{username}/{repo}",
    tag = "repositories",
    security(("bearerAuth" = [])),
    request_body(content = DeletionRequest),
    params(
        ("username" = String, Path, description = "Repository owner username"),
        ("repo" = String, Path, description = "Repository name")
    ),
    responses(
        (status = 200, description = "Repository deleted"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn delete_repo(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeletionRequest>,
) -> Result<StatusCode, ApiError> {
    let user_id = claims.sub;
    let can_delete = sqlx::query_scalar!(
        r#"
        SELECT
            EXISTS (
                SELECT 1
                FROM repositories r
                JOIN users u ON u.id = r.owner_id
                WHERE r.name = $1
                  AND u.username = $2
                  AND r.owner_id = $3
            )
            OR
            EXISTS (
                SELECT 1
                FROM repositories r
                JOIN users u ON u.id = r.owner_id
                JOIN repository_collaborators rc ON rc.repository_id = r.id
                WHERE r.name = $1
                  AND u.username = $2
                  AND rc.user_id = $3
                  AND rc.role = 'admin'
            ) AS allowed;
        "#,
        payload.repo_name,
        payload.username,
        user_id
    )
    .fetch_one(&state.pool)
    .await?
    .unwrap_or(false);
    if !can_delete {
        return Err(ApiError::Unauthorized);
    }
    let path = &state
        .git_storage
        .join(user_id.to_string())
        .join(payload.repo_name);
    remove_repo(path).map_err(|_| ApiError::Internal)?;
    Ok(StatusCode::OK)
}
#[utoipa::path(
    patch,
    path = "/{username}/{repo}",
    tag = "repositories",
    security(("bearerAuth" = [])),
    request_body(content = UpdateRepo),
    params(
        ("username" = String, Path, description = "Repository owner username"),
        ("repo" = String, Path, description = "Repository name")
    ),
    responses(
        (status = 200, description = "Updated repository metadata", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
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

#[utoipa::path(
    get,
    path = "/{repo_owner}/{repo_name}/commits",
    tag = "commits",
    params(
        ("repo_owner" = String, Path, description = "Repository owner username"),
        ("repo_name" = String, Path, description = "Repository name")
    ),
    responses(
        (status = 200, description = "List of commits", body = [CommitInfo]),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_commits(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path((repo_owner, repo_name)): Path<(String, String)>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<Vec<CommitInfo>>, ApiError> {
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
    Ok(Json(commits))
}
///A lot of these are gix errors cuz gix doesn't define a centralized error enum.
#[derive(Deserialize)]
pub struct InnerRoute {
    owner: String,
    name: String,
    path: String,
    id: ObjectId,
}
#[utoipa::path(
    get,
    path = "/{owner}/{name}/tree/{id}",
    tag = "repositories",
    params(
        ("owner" = String, Path, description = "Repository owner username"),
        ("name" = String, Path, description = "Repository name"),
        ("id" = String, Path, description = "Object id (tree) to list")
    ),
    responses(
        (status = 200, description = "Tree node", body = Node),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearerAuth" = []))
)]
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
#[utoipa::path(
    get,
    path = "/{owner}/{name}/blob/{id}",
    tag = "repositories",
    params(
        ("owner" = String, Path, description = "Repository owner username"),
        ("name" = String, Path, description = "Repository name"),
        ("id" = String, Path, description = "Object id (blob)"),
        ("path" = String, Path, description = "File path within the repository")
    ),
    responses(
        (status = 200, description = "File contents", content_type = "application/octet-stream"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "File not found"),
        (status = 500, description = "Internal server error")
    ),
    security(("bearerAuth" = []))
)]
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
fn remove_repo(path: &std::path::Path) -> Result<(), std::io::Error> {
    fs::remove_file(path)
}
