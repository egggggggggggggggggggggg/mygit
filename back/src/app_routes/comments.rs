use std::sync::Arc;

///Horribly designed as both comments for issue and pull requests are smashed tgoether requiring an
///additional check to determine if it should be returned. Maybe cache a repo's comments into an in
///mem cache if we ever utilize the cachelayer
use crate::{
    AppState,
    app_routes::auth::{AuthUser, MaybeAuthUser},
    errors::ApiError,
};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use time::{OffsetDateTime, PrimitiveDateTime};
use utoipa::ToSchema;
use uuid::Uuid;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, ToSchema)]
#[sqlx(type_name = "comment_target", rename_all = "snake_case")]
pub enum CommentTarget {
    PullRequest,
    Issue,
}

#[derive(Serialize, Deserialize)]
pub struct CommentCreationRequest {
    target_type: CommentTarget,
    target_id: Uuid,
    body: String,
    files: Vec<Uuid>,
}
pub async fn create_comment(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(request): Json<CommentCreationRequest>,
) -> Result<(), ApiError> {
    let user_id = claims.sub;
    let comment_id = sqlx::query_scalar!(
        r#"
        INSERT INTO comments (
            target_type, 
            author_id,
            body, 
            target_id
        )
        VALUES ($1 , $2, $3, $4)
        RETURNING
            id
        "#,
        request.target_type as CommentTarget,
        user_id,
        request.body,
        request.target_id,
    )
    .fetch_one(&state.pool)
    .await?;
    sqlx::query!(
        r#"
        INSERT INTO comment_files (comment_id, file_id)
        SELECT $1, unnest($2::uuid[])
        "#,
        comment_id,
        &request.files as &[Uuid]
    )
    .execute(&state.pool)
    .await?;
    Ok(())
}

#[derive(Serialize)]
pub struct Comment {
    pub id: Uuid,
    pub author_id: Option<Uuid>,
    pub body: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Deserialize, ToSchema)]
pub struct CommentAcquireRequest {
    pub repo_id: Uuid,
    pub target_type: CommentTarget,
    pub cursor_created_at: Option<PrimitiveDateTime>,
    pub cursor_id: Option<Uuid>,
    pub per_page: Option<i64>,
}

#[derive(Serialize)]
pub struct CommentPage {
    pub comments: Vec<Comment>,
    pub next_cursor: Option<CommentCursor>,
    pub has_more: bool,
}

#[derive(Serialize)]
pub struct CommentCursor {
    pub created_at: OffsetDateTime,
    pub id: Uuid,
}
#[utoipa::path(get, path = "/{username}/{repo}/pulls/{id}/comments", tag = "comments", security(("bearerAuth" =[])), )]
pub async fn get_comments(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Json(request): Json<CommentAcquireRequest>,
) -> Result<Json<CommentPage>, ApiError> {
    let user_id = maybe_claims.map(|c| c.sub);
    let has_acces = sqlx::query_scalar!(
        r#"
        SELECT EXISTS (
            SELECT 1 
            FROM repositories r 
            WHERE r.id = $1 
                AND (
                    r.is_private = false
                    OR r.owner_id = $2 
                    OR EXISTS (
                    SELECT 1 
                    FROM repository_collaborators rc 
                    WHERE rc.repository_id = r.id 
                        AND rc.user_id = $2 
                    )
                )
        )
        "#,
        request.repo_id,
        user_id
    )
    .fetch_one(&state.pool)
    .await?
    .unwrap_or(false);
    if !has_acces {
        return Err(ApiError::Unauthorized);
    }
    let per_page = request.per_page.unwrap_or(50).clamp(1, 100);
    let comments = sqlx::query_as!(
        Comment,
        r#"
        SELECT
            c.id,
            c.author_id,
            c.body,
            c.created_at ,
            c.updated_at 
        FROM comments c
        WHERE
            c.repository_id = $1
            AND c.target_type = $2
            AND (
                $3::timestamp IS NULL
                OR (
                    c.created_at > $3
                    OR (
                        c.created_at = $3
                        AND c.id > $4
                    )
                )
            )
        ORDER BY c.created_at ASC, c.id ASC
        LIMIT $5
        "#,
        request.repo_id,
        request.target_type as CommentTarget,
        request.cursor_created_at,
        request.cursor_id,
        per_page + 1,
    )
    .fetch_all(&state.pool)
    .await?;
    let has_more = comments.len() as i64 > per_page;
    let mut comments = comments;
    if has_more {
        comments.pop();
    }
    let next_cursor = comments.last().map(|comment| CommentCursor {
        created_at: comment.created_at,
        id: comment.id,
    });
    Ok(Json(CommentPage {
        comments,
        next_cursor,
        has_more,
    }))
}
