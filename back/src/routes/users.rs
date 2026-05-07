use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    routes::auth::{AuthUser, MaybeAuthUser},
};

#[derive(Debug, Serialize)]
pub struct UserProfileResponse {
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    // useful metadata
    pub is_self: bool,
    pub is_following: bool,
}

#[axum::debug_handler]
pub async fn user_profile(
    MaybeAuthUser(claims): MaybeAuthUser,
    State(state): State<Arc<AppState>>,
    Path(username): Path<String>,
) -> Result<Json<UserProfileResponse>, &'static str> {
    let user = sqlx::query!(
        r#"
        SELECT
            id,
            username,
            display_name,
            bio,
            avatar_url
        FROM users
        WHERE username = $1
        "#,
        username
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| "db error")?;

    let Some(user) = user else {
        return Err("User does not exist");
    };

    // placeholder for future follow system
    let is_following = false;

    let is_self = claims.as_ref().map(|c| c.sub == user.id).unwrap_or(false);

    Ok(Json(UserProfileResponse {
        username: user.username,
        display_name: user.display_name,
        bio: user.bio,
        avatar_url: user.avatar_url,
        is_self,
        is_following,
    }))
}

#[derive(Debug, Deserialize)]
pub struct UpdateRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateUserResponse {
    pub success: bool,
}

#[axum::debug_handler]
pub async fn update_user(
    AuthUser(claims): AuthUser,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateRequest>,
) -> Result<Json<UpdateUserResponse>, &'static str> {
    // basic validation
    if let Some(display_name) = &payload.display_name
        && display_name.len() > 100
    {
        return Err("display name too long");
    }

    if let Some(avatar_url) = &payload.avatar_url
        && avatar_url.len() > 500
    {
        return Err("avatar url too long");
    }

    sqlx::query!(
        r#"
        UPDATE users
        SET
            display_name = COALESCE($1, display_name),
            bio = COALESCE($2, bio),
            avatar_url = COALESCE($3, avatar_url),
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $4
        "#,
        payload.display_name,
        payload.bio,
        payload.avatar_url,
        claims.sub,
    )
    .execute(&state.pool)
    .await
    .map_err(|_| "db error")?;

    Ok(Json(UpdateUserResponse { success: true }))
}
