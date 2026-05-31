use askama::Error as AskamaError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use reqwest::Error as ReqwestError;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Config error: {0}")]
    Config(String),

    #[error("HTTP client error: {0}")]
    Http(#[from] ReqwestError),

    #[error("Template error: {0}")]
    Template(#[from] AskamaError),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Http(_) => StatusCode::BAD_GATEWAY,
            AppError::Template(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let body = format!("Error: {self}");
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_error_config_status() {
        let err = AppError::Config("bad config".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_app_error_display() {
        let err = AppError::Config("test message".to_string());
        assert_eq!(format!("{}", err), "Config error: test message");

        let err2 = AppError::Internal("internal error".to_string());
        assert_eq!(format!("{}", err2), "Internal error: internal error");
    }

    #[test]
    fn test_app_error_debug() {
        let err = AppError::Config("test".to_string());
        let debug = format!("{err:?}");
        assert!(debug.contains("Config"));
    }

    #[test]
    fn test_app_error_body_contains_message() {
        let err = AppError::Config("my config error".to_string());
        // Verify the error message appears in the Display form
        assert!(format!("{}", err).contains("my config error"));
    }
}
