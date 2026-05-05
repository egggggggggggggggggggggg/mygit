use {
    crate::AppState,
    argon2::password_hash::rand_core::{OsRng, RngCore},
    argon2::password_hash::{PasswordHash, SaltString},
    argon2::{Argon2, PasswordHasher, PasswordVerifier},
    axum::http::StatusCode,
    axum::{
        Json,
        extract::{FromRequestParts, State},
        http::request::Parts,
        response::IntoResponse,
    },
    base64::{Engine as _, engine::general_purpose},
    jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode},
    serde::{Deserialize, Serialize},
    sha2::{Digest, Sha256},
    sqlx::{Executor, Postgres},
    std::sync::Arc,
    time::{Duration, OffsetDateTime},
};
//TODO: Will replace &'static str with an actual error enum for returning.
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // user id / username
    pub exp: usize,  // expiration timestamp
}
#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub identifier: String, // email OR username
    pub password: String,
}
pub fn generate_access_token(
    user_id: &str,
    secret: &'static [u8],
) -> Result<String, jsonwebtoken::errors::Error> {
    let expiration = OffsetDateTime::now_utc().unix_timestamp() as usize + 60 * 60; // 1 hour
    let claims = Claims {
        sub: user_id.to_string(),
        exp: expiration,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
}
pub fn verify_token(
    token: &str,
    secret: &'static [u8],
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )?;
    Ok(data.claims)
}
pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32]; // 256-bit
    OsRng.fill_bytes(&mut bytes);
    general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
pub fn hash_refresh_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}
pub async fn store_refresh_token<'e, E>(
    executor: E,
    user_id: uuid::Uuid,
    token: &str,
) -> Result<(), sqlx::Error>
where
    E: Executor<'e, Database = Postgres>,
{
    let token_hash = hash_refresh_token(token);
    let expires_at = OffsetDateTime::now_utc() + Duration::days(7);
    sqlx::query!(
        r#"
        INSERT INTO refresh_tokens (user_id, token_hash, expires_at)
        VALUES ($1, $2, $3)
        "#,
        user_id,
        token_hash,
        expires_at
    )
    .execute(executor)
    .await?;
    Ok(())
}
pub fn hash_password(password: &str) -> Result<String, &'static str> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| "Failed to hash the password")?
        .to_string();
    Ok(hash)
}
pub fn verify_password(password: &str, hash: &str) -> Result<bool, &'static str> {
    let parsed_hash = PasswordHash::new(hash).map_err(|_| "invalid stored hash")?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}
#[axum::debug_handler]
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<AuthResponse, &'static str> {
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
    let access_token = generate_access_token(&user.id.to_string(), state.jwt_secret)
        .map_err(|_| "Failed to create token")?;
    let refresh_token = generate_refresh_token();
    store_refresh_token(&state.pool, user.id, &refresh_token)
        .await
        .map_err(|_| "Failed to store refresh token")?;
    Ok(AuthResponse {
        access_token,
        refresh_token,
    })
}

#[axum::debug_handler]
pub async fn logout(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<(), &'static str> {
    let token_hash = hash_refresh_token(&req.refresh_token);

    sqlx::query!(
        "DELETE FROM refresh_tokens WHERE token_hash = $1",
        token_hash
    )
    .execute(&state.pool)
    .await
    .map_err(|_| "db error")?;

    Ok(())
}
///This should be able to perform duplication checks.
pub async fn signup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SignupRequest>,
) -> Result<AuthResponse, &'static str> {
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
    .map_err(|e| {
        if let sqlx::Error::Database(db_err) = &e
            && db_err.code().as_deref() == Some("23505")
        {
            if let Some(constraint) = db_err.constraint() {
                return match constraint {
                    "users_email_key" => "email already in use",
                    "users_username_key" => "username already in use",
                    _ => "user already exists",
                };
            }
            return "user already exists";
        }
        "Interal server error"
    })?;
    let access_token = generate_access_token(&user_id.to_string(), state.jwt_secret)
        .map_err(|_| "Failed to create a token")?;
    let refresh_token = generate_refresh_token();
    store_refresh_token(&state.pool, user_id, &refresh_token)
        .await
        .map_err(|_| "Failed to store refresh token")?;
    Ok(AuthResponse {
        access_token,
        refresh_token,
    })
}
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
}
impl IntoResponse for AuthResponse {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}
#[axum::debug_handler]
//Refresh endpoint. General idea is the user calls this endpoint to get a new access token when the
//previous one has expired. First it fetches the requested refresh token from the hash. It checks
//for expiration and if expired returns an Err forcing the user to login again to get a new one. If
//not it rotates itself by deleting the old one and creating a new one. It then returns an access_token
//along with the refresh_token. The rotation stuff utilizes a db transaction. Not entirely sure if
//it's a good idea but it seems good on paper.
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<AuthResponse, &'static str> {
    let token_hash = hash_refresh_token(&req.refresh_token);
    let record = sqlx::query!(
        r#"
        SELECT id, user_id, expires_at
        FROM refresh_tokens
        WHERE token_hash = $1
        "#,
        token_hash
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|_| "db error")?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR.as_str())?;
    let record = record.ok_or("invalid token")?;
    if record.expires_at < OffsetDateTime::now_utc() {
        return Err("Refresh token expired");
    }
    sqlx::query!("DELETE FROM refresh_tokens WHERE id = $1", record.id)
        .execute(&mut *tx)
        .await
        .map_err(|_| "db error")?;
    let new_refresh = generate_refresh_token();
    store_refresh_token(&mut *tx, record.user_id, &new_refresh)
        .await
        .map_err(|_| "db error")?;
    tx.commit().await.map_err(|_| "Transaction failed")?;
    let access_token = generate_access_token(&record.user_id.to_string(), state.jwt_secret)
        .map_err(|_| "Failed to create token")?;
    Ok(AuthResponse {
        access_token,
        refresh_token: new_refresh,
    })
}
pub fn auth_required(auth_header: &str, secret: &'static [u8]) -> Result<Claims, &'static str> {
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or("missing bearer token")?;
    verify_token(token, secret).map_err(|_| "invalid token")
}
pub struct AuthUser(pub Claims);
impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = &'static str;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or("missing auth header")?;
        Ok(Self(auth_required(auth_header, state.jwt_secret)?))
    }
}
