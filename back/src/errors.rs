use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Serialize)]
struct ErrorResponse {
    code: &'static str,
    message: String,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("token expired")]
    TokenExpired,

    #[error("missing authorization header")]
    MissingAuthHeader,

    #[error("invalid token")]
    InvalidToken,

    #[error("authentication required")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,
    #[error("validation error: {0}")]
    Validation(String),

    #[error("repository not found")]
    RepoNotFound,

    #[error("repository already exists")]
    RepoAlreadyExists,

    #[error("filesystem operation failed")]
    Filesystem,

    #[error("git operation failed")]
    Git,

    #[error("database operation failed")]
    Database,

    #[error("internal server error")]
    Internal,

    #[error("commits listing for branch failed")]
    CommitListingFailed,

    #[error("could not find the specified file")]
    FileNotFound,

    #[error("unsupported file type")]
    UnsupportedFileType,

    #[error("dumb arbitrary file upload error")]
    ArbitraryFileUpload,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::InvalidCredentials => (StatusCode::UNAUTHORIZED, "invalid_credentials"),

            Self::TokenExpired => (StatusCode::UNAUTHORIZED, "token_expired"),

            Self::MissingAuthHeader => (StatusCode::BAD_REQUEST, "missing_auth_header"),

            Self::InvalidToken => (StatusCode::UNAUTHORIZED, "invalid_token"),

            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),

            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),

            Self::Validation(_) => (StatusCode::BAD_REQUEST, "validation_error"),

            Self::RepoNotFound => (StatusCode::NOT_FOUND, "repo_not_found"),

            Self::RepoAlreadyExists => (StatusCode::CONFLICT, "repo_already_exists"),

            Self::Filesystem => (StatusCode::INTERNAL_SERVER_ERROR, "filesystem_error"),

            Self::Git => (StatusCode::INTERNAL_SERVER_ERROR, "git_error"),

            Self::Database => (StatusCode::INTERNAL_SERVER_ERROR, "database_error"),

            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),

            Self::CommitListingFailed => (StatusCode::BAD_REQUEST, "listing commits failed"),

            Self::FileNotFound => (StatusCode::INTERNAL_SERVER_ERROR, "file not found"),

            Self::UnsupportedFileType => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "file not allowed"),

            Self::ArbitraryFileUpload => {
                (StatusCode::UNSUPPORTED_MEDIA_TYPE, "arbitrary error idk")
            }
        };
        tracing::error!(
            status = %status,
            error_code = code,
            error = ?self
        );
        (
            status,
            Json(ErrorResponse {
                code,
                message: self.to_string(),
            }),
        )
            .into_response()
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        tracing::error!("io error: {:?}", err);
        Self::Filesystem
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!("database error: {:?}", err);
        Self::Database
    }
}

impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        tracing::error!("jwt error: {:?}", err);

        use jsonwebtoken::errors::ErrorKind;

        match err.kind() {
            ErrorKind::ExpiredSignature => Self::TokenExpired,
            _ => Self::InvalidToken,
        }
    }
}
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
