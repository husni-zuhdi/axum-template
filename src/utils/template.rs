use askama::Template;
use axum::http::StatusCode;
use axum::response::Html;

use crate::error::AppError;

pub fn render_template<T: Template>(template: &T) -> Result<String, (StatusCode, Html<String>)> {
    template.render().map_err(|e| {
        tracing::error!("Template render error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("Internal error: {}", e)),
        )
    })
}

/// Renders a full page template, mapping render errors to `AppError::Internal`.
pub fn render_page<T: Template>(template: &T) -> Result<Html<String>, AppError> {
    render_template(template)
        .map(Html)
        .map_err(|(_, html)| AppError::Internal(html.0))
}
