use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::PrimitiveDateTime;
use uuid::Uuid;

use crate::{
    AppState, Pagination,
    routes::auth::{AuthUser, MaybeAuthUser},
};

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
    pub state: Option<String>,
    pub number: i32,
    pub closed_at: Option<PrimitiveDateTime>,
    pub created_at: PrimitiveDateTime,
    pub updated_at: PrimitiveDateTime,
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
///Replace this with Cursor pagination.
/// GET /repos/:owner/:repo/issues?page=1&per_page=20
pub async fn list_issues(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    Path(path): Path<RepoPath>,
    Query(pagination): Query<Pagination>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<Issue>>, (StatusCode, String)> {
    let user_id = maybe_claims.map(|c| c.sub);
    let page = pagination.page.unwrap_or(1).max(1);
    let per_page = pagination.per_page.unwrap_or(20).clamp(1, 100);

    let offset = ((page - 1) * per_page) as i64;
    //This utilizes OFFSET which does not scale too well on large db. Might wanna replace with
    //actual paging logic.
    let issues = sqlx::query_as!(
        Issue,
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
        ORDER BY i.number DESC
        LIMIT $4
        OFFSET $5        
        "#,
        &path.owner,
        &path.repo,
        user_id,
        per_page as i64,
        offset,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;
    Ok(Json(issues))
}

/// POST /repos/:owner/:repo/issues
/// This just isn't worth the loss in information.
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
    let repo_id: Option<Uuid> = sqlx::query_scalar!(
        r#"
        SELECT r.id
        FROM repositories r
        INNER JOIN users u
            ON u.id = r.owner_id
        WHERE u.username = $1
          AND r.name = $2
        "#,
        &path.owner,
        &path.repo,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;
    let repo_id =
        repo_id.ok_or_else(|| (StatusCode::NOT_FOUND, "Repository not found".to_string()))?;
    //Might not be needed.
    // Generate next issue number scoped to repository
    let next_number = sqlx::query_scalar!(
        r#"
        UPDATE repositories
        SET next_issue_number = next_issue_number + 1
        WHERE id = $1
        RETURNING next_issue_number - 1
        "#,
        repo_id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?; // Insert issue
    let issue = sqlx::query_as!(
        Issue,
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
        repo_id,
        claims.sub,
        &payload.title,
        payload.body,
        next_number
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;
    tx.commit().await.map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(IssueResponse { issue })))
}
/// GET /repos/:owner/:repo/issues/:number
pub async fn get_issue(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    Path(path): Path<IssuePath>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Issue>, (StatusCode, String)> {
    let user_id = maybe_claims.map(|c| c.sub);
    let issue = sqlx::query_as!(
        Issue,
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
            AND i.number = $4
        "#,
        &path.owner,
        &path.repo,
        user_id,
        path.number,
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "Issue not found".to_string()))?;
    Ok(Json(issue))
}
pub async fn change_issue_state(
    AuthUser(claims): AuthUser,
    Path(path): Path<IssuePath>,
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, String)> {
    let user_id = claims.sub;
    let result = sqlx::query!(
        r#"
        UPDATE issues i
        SET
            state = 'closed',
            closed_at = NOW(),
            updated_at = NOW()
        FROM repositories r
        INNER JOIN users u
            ON u.id = r.owner_id
        WHERE i.repository_id = r.id
            AND u.username = $1
            AND r.name = $2
            AND i.number = $3
            AND (
                r.owner_id = $4
                OR EXISTS (
                    SELECT 1
                    FROM repository_collaborators rc
                    WHERE rc.repository_id = r.id
                        AND rc.user_id = $4
                )
            )
        "#,
        path.owner,
        path.repo,
        path.number,
        user_id,
    )
    .execute(&state.pool)
    .await
    .map_err(internal_error)?;
    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Issue not found".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
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
