use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::Time;
use uuid::Uuid;

use crate::{AppState, Pagination, routes::auth::AuthUser};

#[derive(Deserialize)]
pub struct IssueCreation {
    pub title: String,
    pub body: Option<String>,
}

#[derive(Serialize, FromRow)]
pub struct Issue {
    pub id: Uuid,
    pub repository_id: Uuid,
    pub author_id: Option<Uuid>,
    pub assignee_id: Option<Uuid>,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub number: i32,
    pub closed_at: Option<Time>,
    pub created_at: Time,
    pub updated_at: Time,
}

#[derive(Serialize)]
pub struct IssueResponse {
    pub issue: Issue,
}

#[derive(Deserialize)]
pub struct RepoPath {
    pub owner: String,
    pub repo: String,
}

#[derive(Deserialize)]
pub struct IssuePath {
    pub owner: String,
    pub repo: String,
    pub number: i32,
}
///Should first perform a repo access check so maybe this can be middleware?
/// GET /repos/:owner/:repo/issues?page=1&per_page=20
pub async fn list_issues(
    Path(path): Path<RepoPath>,
    Query(pagination): Query<Pagination>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Issue>>, (StatusCode, String)> {
    let page = pagination.page.unwrap_or(1).max(1);
    let per_page = pagination.per_page.unwrap_or(20).clamp(1, 100);

    let offset = ((page - 1) * per_page) as i64;

    let issues = sqlx::query_as::<_, Issue>(
        r#"
        SELECT
            i.id,
            i.repository_id,
            i.author_id,
            i.assignee_id,
            i.title,
            i.body,
            i.state::text as state,
            i.number,
            i.closed_at,
            i.created_at,
            i.updated_at
        FROM issues i
        INNER JOIN repositories r
            ON r.id = i.repository_id
        INNER JOIN users u
            ON u.id = r.owner_id
        WHERE u.username = $1
          AND r.name = $2
        ORDER BY i.number DESC
        LIMIT $3
        OFFSET $4
        "#,
    )
    .bind(&path.owner)
    .bind(&path.repo)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    Ok(Json(issues))
}

/// POST /repos/:owner/:repo/issues
///
/// Requires authentication.
#[axum::debug_handler]
pub async fn create_issue(
    AuthUser(claims): AuthUser,
    Path(path): Path<RepoPath>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<IssueCreation>,
) -> Result<(StatusCode, Json<IssueResponse>), (StatusCode, String)> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Find repository
    let repo_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT r.id
        FROM repositories r
        INNER JOIN users u
            ON u.id = r.owner_id
        WHERE u.username = $1
          AND r.name = $2
        "#,
    )
    .bind(&path.owner)
    .bind(&path.repo)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let repo_id =
        repo_id.ok_or_else(|| (StatusCode::NOT_FOUND, "Repository not found".to_string()))?;
    //Might not be needed.
    // Generate next issue number scoped to repository
    let next_number: i32 = sqlx::query_scalar(
        r#"
        UPDATE repositories
        SET next_issue_number = next_issue_number + 1
        WHERE id = $1
        RETURNING next_issue_number - 1
        "#,
    )
    .bind(repo_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?; // Insert issue
    let issue = sqlx::query_as::<_, Issue>(
        r#"
        INSERT INTO issues (
            repository_id,
            author_id,
            title,
            body,
            number
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING
            id,
            repository_id,
            author_id,
            assignee_id,
            title,
            body,
            state::text as state,
            number,
            closed_at,
            created_at,
            updated_at
        "#,
    )
    .bind(repo_id)
    .bind(claims.sub)
    .bind(&payload.title)
    .bind(&payload.body)
    .bind(next_number)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(IssueResponse { issue })))
}

/// GET /repos/:owner/:repo/issues/:number
pub async fn get_issue(
    Path(path): Path<IssuePath>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Issue>, (StatusCode, String)> {
    let issue = sqlx::query_as::<_, Issue>(
        r#"
        SELECT
            i.id,
            i.repository_id,
            i.author_id,
            i.assignee_id,
            i.title,
            i.body,
            i.state::text as state,
            i.number,
            i.closed_at,
            i.created_at,
            i.updated_at
        FROM issues i
        INNER JOIN repositories r
            ON r.id = i.repository_id
        INNER JOIN users u
            ON u.id = r.owner_id
        WHERE u.username = $1
          AND r.name = $2
          AND i.number = $3
        "#,
    )
    .bind(&path.owner)
    .bind(&path.repo)
    .bind(path.number)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Issue not found".to_string()))?;
    Ok(Json(issue))
}

fn internal_error<E>(err: E) -> (StatusCode, String)
where
    E: std::fmt::Display,
{
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Database error: {err}"),
    )
}
