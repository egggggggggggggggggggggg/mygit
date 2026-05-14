use serde::{Deserialize, Serialize};
use sqlx::prelude::{FromRow, Type};
use time::{OffsetDateTime, PrimitiveDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Serialize, FromRow, ToSchema)]
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type, ToSchema)]
#[sqlx(type_name = "pr_state", rename_all = "lowercase")]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

#[derive(Deserialize, Serialize, FromRow, ToSchema)]
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
    merged_at: Option<OffsetDateTime>,
    closed_at: Option<OffsetDateTime>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}
