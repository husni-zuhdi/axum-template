use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

#[derive(Debug)]
pub enum AppError {
    NotFound,
    Conflict(String),
    BadRequest(String),
    Internal(String),
    Form { status: StatusCode, html: String },
}

impl From<libsql::Error> for AppError {
    fn from(e: libsql::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "Not found").into_response(),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg).into_response(),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
            Self::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
            }
            Self::Form { status, html } => (status, Html(html)).into_response(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn test_not_found_status() {
        let resp = AppError::NotFound.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_conflict_status() {
        let resp = AppError::Conflict("already exists".into()).into_response();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(body, "already exists");
    }

    #[tokio::test]
    async fn test_form_status_and_html() {
        let resp = AppError::Form {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            html: "<div>error</div>".into(),
        }
        .into_response();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
