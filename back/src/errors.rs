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
}

//
// Error -> HTTP Response
//

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
