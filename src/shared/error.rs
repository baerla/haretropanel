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
