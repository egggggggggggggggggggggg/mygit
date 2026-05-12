use std::sync::Arc;

///Horribly designed as both comments for issue and pull requests are smashed tgoether requiring an
///additional check to determine if it should be returned. Maybe cache a repo's comments into an in
///mem cache if we ever utilize the cachelayer
use crate::{
    AppState,
    errors::ApiError,
    routes::auth::{AuthUser, MaybeAuthUser},
};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use sqlx::Type;
use uuid::Uuid;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[sqlx(type_name = "comment_target", rename_all = "snake_case")]
pub enum CommentTarget {
    PullRequest,
    Issue,
}
//Unsure of how to preceed regarding files stored in a request. Obviousl we need to store it in the
//file_comments join table but we are unsure of whether the file system actually holds the files.
//This causes blocking on the mainthread so maybe comments and whatnot should be done in the
//background.
#[derive(Serialize, Deserialize)]
pub struct CommentCreationRequest {
    target_type: CommentTarget,
    target_id: Uuid,
    body: String,
    //This might not be the right type for files.
    files: Vec<Uuid>,
}
///Specify the
#[derive(Serialize, Deserialize)]
pub struct CommentAcquireRequest {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    pub repo_id: Uuid,
    pub target_id: Uuid,
    pub target_type: CommentTarget,
}
//First we run an uplod
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
//The fronten
pub async fn get_comments(
    MaybeAuthUser(claims): MaybeAuthUser,
    Json(request): Json<CommentAcquireRequest>,
) -> Result<(), ApiError> {
    Ok(())
}
