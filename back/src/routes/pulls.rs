use std::sync::Arc;

use crate::{
    AppState,
    errors::ApiError,
    routes::{
        auth::{AuthUser, MaybeAuthUser},
        issues::IssuePath,
    },
};
use axum::{
    Json, debug_handler,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use sqlx::{Type, prelude::FromRow};
use time::{OffsetDateTime, PrimitiveDateTime};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[sqlx(type_name = "pr_state", rename_all = "lowercase")]
pub enum PrState {
    Open,
    Closed,
    Merged,
}
#[derive(Deserialize, Serialize, FromRow)]
pub struct PullRequest {
    id: Uuid,
    repository_id: Uuid,
    author_id: Option<Uuid>,
    title: String,
    body: Option<String>,
    state: PrState,
    number: i32,
    head_branch_id: Option<Uuid>,
    base_branch_id: Option<Uuid>,
    merged_at: Option<PrimitiveDateTime>,
    closed_at: Option<PrimitiveDateTime>,
    created_at: PrimitiveDateTime,
    updated_at: PrimitiveDateTime,
}
#[debug_handler]
pub async fn list_pulls(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    Path(path): Path<IssuePath>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PullRequest>>, ApiError> {
    let user_id = maybe_claims.map(|c| c.sub);
    let repo_id = sqlx::query_scalar!(
        r#"
        SELECT r.id
        FROM repositories r
        INNER JOIN users u
            ON u.id = r.owner_id
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
        "#,
        &path.owner,
        &path.repo,
        user_id,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    let pull_requests = sqlx::query_as!(
        PullRequest,
        r#"
        SELECT
            p.id,
            p.repository_id,
            p.author_id,
            p.title,
            p.body,
            p.state as "state: PrState",
            p.number,
            p.head_branch_id,
            p.base_branch_id,
            p.merged_at,
            p.closed_at,
            p.created_at,
            p.updated_at
        FROM pull_requests p
        WHERE p.repository_id = $1
        "#,
        repo_id
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(pull_requests))
}
#[derive(Deserialize, Serialize, Debug)]
pub struct MergeRequest {
    force: bool,
}

pub async fn check_merge() {}

///Before doing any action involving the pr_state of a repo check if it's not open.
///If not open then don't perform any action unless they opt to reopen it.
#[axum::debug_handler]
pub async fn merge_pull(
    AuthUser(claims): AuthUser,
    Path(path): Path<IssuePath>,
    State(state): State<Arc<AppState>>,
) -> Result<(), ApiError> {
    let repo_id = sqlx::query_scalar!(
        r#"
        SELECT r.id
        FROM repositories r
        INNER JOIN users u
            ON u.id = r.owner_id
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
        "#,
        &path.owner,
        &path.repo,
        claims.sub,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    sqlx::query_scalar!(
        r#"
        UPDATE pull_requests 
        SET state = 'merged'
        WHERE repository_id = $1
        "#,
        repo_id
    )
    .execute(&state.pool)
    .await?;
    let repo_path = state.git_storage.join(&path.owner).join(&path.repo);
    let _repo = gix::open(repo_path).map_err(|_| ApiError::Internal)?;
    //Set the
    Ok(())
}
#[axum::debug_handler]
pub async fn close_pull(
    AuthUser(claims): AuthUser,
    Path(path): Path<IssuePath>,
    State(state): State<Arc<AppState>>,
) -> Result<(), ApiError> {
    let repo_id = sqlx::query_scalar!(
        r#"
        SELECT r.id
        FROM repositories r
        INNER JOIN users u
            ON u.id = r.owner_id
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
        "#,
        &path.owner,
        &path.repo,
        claims.sub,
    )
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    let now = OffsetDateTime::now_utc();
    sqlx::query_scalar!(
        r#"
        UPDATE pull_requests
        SET state = 'closed', closed_at = $2
        WHERE repository_id = $1 
            AND state = 'open'
        "#,
        repo_id,
        now.unix_timestamp(),
    )
    .fetch_optional(&state.pool)
    .await?;

    Ok(())
}
pub async fn open_pull() {}
