pub mod auth;
pub mod comments;
pub mod issues;
pub mod pulls;
pub mod repo;
pub mod storage;
pub mod users;
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::ApiError;
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
