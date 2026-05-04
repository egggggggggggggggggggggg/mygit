use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id / username
    pub exp: usize,  // expiration timestamp
}
#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub username: Option<String>,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub identifier: String, // email OR username
    pub password: String,
}
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use std::sync::{Arc, OnceLock};
use time::OffsetDateTime;

use crate::AppState;

static JWT_SECRET: OnceLock<Vec<u8>> = OnceLock::new();

pub fn jwt_secret() -> &'static [u8] {
    JWT_SECRET.get_or_init(|| {
        std::env::var("JWT_SECRET")
            .expect("JWT_SECRET must be set")
            .into_bytes()
    })
}
pub fn create_token(user_id: &str) -> String {
    let expiration = OffsetDateTime::now_utc().unix_timestamp() as usize + 60 * 60; // 1 hour

    let claims = Claims {
        sub: user_id.to_string(),
        exp: expiration,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret()),
    )
    .unwrap()
}

pub fn verify_token(token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}
#[axum::debug_handler]
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<String, &'static str> {
    // fetch user by email OR username
    let user = sqlx::query!(
        r#"
        SELECT id, password_hash
        FROM users
        WHERE email = $1 OR username = $1
        "#,
        req.identifier
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| "db error")?;

    let user = user.ok_or("invalid credentials")?;

    let is_valid = verify_password(&req.password, &user.password_hash)?;
    if !is_valid {
        return Err("invalid credentials");
    }

    let token = create_token(&user.id.to_string());
    Ok(token)
}
///This should be able to perform duplication checks.
#[axum::debug_handler]
pub async fn signup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SignupRequest>,
) -> Result<String, &'static str> {
    if req.password.len() < 8 {
        return Err("password too short");
    }
    let email = req.email.to_lowercase();
    let password_hash = hash_password(&req.password)?;

    let user_id = sqlx::query_scalar!(
        r#"
        INSERT INTO users (email, username, password_hash)
        VALUES ($1, $2, $3)
        RETURNING id
        "#,
        email,
        req.username,
        password_hash
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|_| "user creation failed")?;

    let token = create_token(&user_id.to_string());
    Ok(token)
}
pub fn auth_required(auth_header: &str) -> Result<Claims, &'static str> {
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or("missing bearer token")?;

    verify_token(token).map_err(|_| "invalid token")
}
use axum::{extract::FromRequestParts, http::request::Parts};

pub struct AuthUser(pub Claims);

impl<S: Sync> FromRequestParts<S> for AuthUser {
    type Rejection = &'static str;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or("missing auth header")?;

        let claims = auth_required(auth_header)?;
        Ok(AuthUser(claims))
    }
}
pub fn hash_password(password: &str) -> Result<String, &'static str> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| "hashing failed")?
        .to_string();

    Ok(hash)
}
pub fn verify_password(password: &str, hash: &str) -> Result<bool, &'static str> {
    let parsed_hash = PasswordHash::new(hash).map_err(|_| "invalid stored hash")?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
