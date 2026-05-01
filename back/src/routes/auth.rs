use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id / username
    pub exp: usize,  // expiration timestamp
}
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use std::sync::OnceLock;
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
#[derive(Deserialize, Serialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[axum::debug_handler]
pub async fn login(
    State(t): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<String, &'static str> {
    let valid = true; // pretend password check worked
    if !valid {
        return Err("invalid credentials");
    }
    let token = create_token(&request.username);
    Ok(token)
}
pub async fn signup(Json(request): Json<LoginRequest>) -> String {
    // Normally: hash password + store in DB
    let token = create_token(&request.username);
    token
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
