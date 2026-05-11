use std::sync::Arc;

use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use time::PrimitiveDateTime;
use uuid::Uuid;

use crate::{
    AppState,
    errors::ApiError,
    routes::{auth::MaybeAuthUser, has_access, issues::IssuePath},
};
#[derive(Deserialize, Serialize)]
pub struct PullRequest {
    id: Uuid,
    repository_id: Uuid,
    author_id: Uuid,
    title: String,
    body: Option<String>,
    state: Option<String>,
    number: i32,
    head_branch_id: Uuid,
    base_branch_id: Uuid,
    merged_at: Option<PrimitiveDateTime>,
    closed_at: Option<PrimitiveDateTime>,
    created_at: PrimitiveDateTime,
    updated_at: PrimitiveDateTime,
}
#[axum::debug_handler]
pub async fn list_pulls(
    MaybeAuthUser(maybe_claims): MaybeAuthUser,
    Path(path): Path<IssuePath>,
    State(state): State<Arc<AppState>>,
) -> Result<(), ApiError> {
    let user_id = maybe_claims.map(|c| c.sub);
    if !has_access(&state.pool, &path.owner, &path.repo, user_id).await? {
        return Err(ApiError::Unauthorized);
    }

    let pull_requests = sqlx::query_as!(
        PullRequest,
        r#"
    SELECT p.* 
    FROM pull_requests p
    WHERE 
    "#
    );
    Ok(())
}
///Specified its closed
pub async fn merge_pull() {}
pub async fn close_pull() {}
pub async fn open_pull() {}
