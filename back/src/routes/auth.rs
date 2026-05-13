use {
    crate::AppState,
    argon2::{
        Argon2, PasswordHasher, PasswordVerifier,
        password_hash::{
            PasswordHash, SaltString,
            rand_core::{OsRng, RngCore},
        },
    },
    axum::{
        Json,
        extract::{FromRequestParts, State},
        http::{StatusCode, header, request::Parts},
        response::IntoResponse,
    },
    base64::{Engine as _, engine::general_purpose},
    jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode},
    serde::{Deserialize, Serialize},
    sha2::{Digest, Sha256},
    sqlx::{Executor, Postgres},
    std::sync::Arc,
    time::{Duration, OffsetDateTime},
    uuid::Uuid,
};

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("token expired")]
    TokenExpired,

    #[error("missing header: {0}")]
    MissingHeader(&'static str),

    #[error("duplicate email")]
    DuplicateEmail,

    #[error("duplicate username")]
    DuplicateUsername,

    #[error("invalid input: {0}")]
    InvalidInput(&'static str),

    #[error("password hashing failed")]
    PasswordHashing,

    #[error("password verification failed")]
    PasswordVerification,

    #[error("token generation failed")]
    TokenGen,

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("jwt error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),
}

use std::borrow::Cow;

impl IntoResponse for AuthError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg): (StatusCode, Cow<'static, str>) = match self {
            Self::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                Cow::Borrowed("Invalid credentials"),
            ),
            Self::TokenExpired => (StatusCode::UNAUTHORIZED, Cow::Borrowed("Token expired")),
            Self::MissingHeader(name) => (StatusCode::BAD_REQUEST, Cow::Borrowed(name)),
            Self::DuplicateEmail => (StatusCode::CONFLICT, Cow::Borrowed("Email already exists")),
            Self::DuplicateUsername => (
                StatusCode::CONFLICT,
                Cow::Borrowed("Username already exists"),
            ),
            Self::InvalidInput(details) => (StatusCode::BAD_REQUEST, Cow::Borrowed(details)),
            Self::PasswordHashing => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Cow::Borrowed("Password hashing failed"),
            ),
            Self::PasswordVerification => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Cow::Borrowed("Password verification failed"),
            ),
            Self::TokenGen => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Cow::Borrowed("Token generation failed"),
            ),
            Self::Database(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Cow::Owned(err.to_string()),
            ),
            Self::Jwt(err) => (StatusCode::UNAUTHORIZED, Cow::Owned(err.to_string())),
        };

        (status, msg).into_response()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: uuid::Uuid,
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
    pub aud: String,
    pub jti: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct SignupRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub identifier: String,
    pub password: String,
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

fn map_sqlx_error(err: sqlx::Error) -> AuthError {
    if let sqlx::Error::Database(db_err) = &err
        && db_err.code().as_deref() == Some("23505")
    {
        return match db_err.constraint() {
            Some("users_email_key") => AuthError::DuplicateEmail,
            Some("users_username_key") => AuthError::DuplicateUsername,
            _ => AuthError::Database(err),
        };
    }
    AuthError::Database(err)
}

pub fn generate_access_token(user_id: Uuid, secret: &'static [u8]) -> Result<String, AuthError> {
    let now = time::OffsetDateTime::now_utc();
    let claims = Claims {
        sub: user_id,
        exp: (now + Duration::minutes(10)).unix_timestamp(),
        iat: now.unix_timestamp(),
        iss: "my-api".to_string(),
        aud: "my-app".to_string(),
        jti: Uuid::new_v4(),
    };
    Ok(encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret),
    )?)
}

pub fn verify_token(token: &str, secret: &'static [u8]) -> Result<Claims, AuthError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.set_audience(&["my-app"]);
    validation.set_issuer(&["my-api"]);
    validation.required_spec_claims = std::collections::HashSet::from([
        "exp".to_string(),
        "iat".to_string(),
        "sub".to_string(),
        "iss".to_string(),
        "aud".to_string(),
        "jti".to_string(),
    ]);
    validation.leeway = 30;

    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)?;
    Ok(token_data.claims)
}

pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
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
) -> Result<(), AuthError>
where
    E: Executor<'e, Database = Postgres>,
{
    let token_hash = hash_refresh_token(token);
    let expires_at = OffsetDateTime::now_utc() + time::Duration::days(7);

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
    .await
    .map_err(AuthError::Database)?;

    Ok(())
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|_| AuthError::PasswordHashing)?
        .to_string();

    Ok(hash)
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AuthError> {
    let parsed_hash = PasswordHash::new(hash).map_err(|_| AuthError::InvalidCredentials)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn auth_required(auth_header: &str, secret: &'static [u8]) -> Result<Claims, AuthError> {
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(AuthError::InvalidCredentials)?;

    verify_token(token, secret)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
}

#[axum::debug_handler]
pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginRequest>,
) -> Result<AuthResponse, AuthError> {
    const DUMMY_HASH: &str =
        "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$7vQm0zFjI4G0g8m1QqY6K6v6T6XQ3V8lYQv8h0w5W0A";

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
    .map_err(AuthError::Database)?;

    let (user_id, password_hash) = match user {
        Some(u) => (Some(u.id), u.password_hash),
        None => (None, DUMMY_HASH.to_string()),
    };

    let is_valid = verify_password(&req.password, &password_hash)?;
    if user_id.is_none() || !is_valid {
        return Err(AuthError::InvalidCredentials);
    }

    let user_id = user_id.unwrap();
    let access_token = generate_access_token(user_id, state.jwt_secret)?;
    let refresh_token = generate_refresh_token();
    store_refresh_token(&state.pool, user_id, &refresh_token).await?;

    Ok(AuthResponse {
        access_token,
        refresh_token,
    })
}

#[axum::debug_handler]
pub async fn logout(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<(), AuthError> {
    let token_hash = hash_refresh_token(&req.refresh_token);

    sqlx::query!(
        "DELETE FROM refresh_tokens WHERE token_hash = $1",
        token_hash
    )
    .execute(&state.pool)
    .await
    .map_err(AuthError::Database)?;

    Ok(())
}

#[axum::debug_handler]
pub async fn signup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SignupRequest>,
) -> Result<AuthResponse, AuthError> {
    if req.password.len() < 8 {
        return Err(AuthError::InvalidInput(
            "password must be at least 8 characters",
        ));
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
    .map_err(map_sqlx_error)?;

    let access_token = generate_access_token(user_id, state.jwt_secret)?;
    let refresh_token = generate_refresh_token();
    store_refresh_token(&state.pool, user_id, &refresh_token).await?;

    Ok(AuthResponse {
        access_token,
        refresh_token,
    })
}

#[axum::debug_handler]
pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshRequest>,
) -> Result<AuthResponse, AuthError> {
    let token_hash = hash_refresh_token(&req.refresh_token);
    let mut tx = state.pool.begin().await.map_err(AuthError::Database)?;

    let record = sqlx::query!(
        r#"
        SELECT id, user_id, expires_at
        FROM refresh_tokens
        WHERE token_hash = $1
        "#,
        token_hash
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(AuthError::Database)?
    .ok_or(AuthError::InvalidCredentials)?;

    if record.expires_at < OffsetDateTime::now_utc() {
        return Err(AuthError::TokenExpired);
    }

    sqlx::query!("DELETE FROM refresh_tokens WHERE id = $1", record.id)
        .execute(&mut *tx)
        .await
        .map_err(AuthError::Database)?;

    let new_refresh = generate_refresh_token();
    store_refresh_token(&mut *tx, record.user_id, &new_refresh).await?;
    tx.commit().await.map_err(AuthError::Database)?;

    let access_token = generate_access_token(record.user_id, state.jwt_secret)?;
    Ok(AuthResponse {
        access_token,
        refresh_token: new_refresh,
    })
}

pub struct AuthUser(pub Claims);

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError::MissingHeader("Authorization"))?;

        Ok(Self(auth_required(auth_header, state.jwt_secret)?))
    }
}

pub struct MaybeAuthUser(pub Option<Claims>);

impl FromRequestParts<Arc<AppState>> for MaybeAuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let claims = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|auth_header| auth_required(auth_header, state.jwt_secret).ok());

        Ok(Self(claims))
    }
}
