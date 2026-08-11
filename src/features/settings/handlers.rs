use askama::Template;
use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Html,
};
use serde::Deserialize;
use tower_sessions::Session;

use crate::app::AppState;
use crate::error::AppError;
use crate::utils::{
    csrf, flash,
    template::{render_page, render_template},
};

#[derive(Template)]
#[template(path = "settings.html")]
pub struct SettingsTemplate {
    pub title: String,
    pub flash_kind: String,
    pub flash_message: String,
    pub csrf_token: String,
    pub error: String,
}

/// The password form fragment (also returned on 422 so the inline error swaps
/// in place). The CSRF token travels in the page's `<meta name="csrf-token">`,
/// which survives the swap, so the fragment renders no token of its own.
#[derive(Template)]
#[template(path = "password_form.html")]
pub struct PasswordFormTemplate {
    pub error: String,
}

#[derive(Deserialize)]
pub struct PasswordForm {
    pub current_password: String,
    pub new_password: String,
    pub confirm_password: String,
}

pub async fn settings_page(session: Session) -> Result<Html<String>, AppError> {
    let (flash_kind, flash_message) = flash::get_flash(&session).await;
    let template = SettingsTemplate {
        title: "Settings".to_string(),
        flash_kind,
        flash_message,
        csrf_token: csrf::get_csrf_token(&session).await,
        error: String::new(),
    };
    render_page(&template)
}

/// Changes the login password: verifies the current password against the
/// effective hash (settings override > env), validates the new one, and stores
/// a fresh argon2id hash under `password_hash_override`. On success the session
/// id is rotated (stale sessions die) and the page redirects with a flash; on
/// failure the form is re-rendered in place (422) with the error shown.
pub async fn password_change(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<PasswordForm>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    match state
        .change_password(
            &form.current_password,
            &form.new_password,
            &form.confirm_password,
        )
        .await
    {
        Ok(()) => {
            session.cycle_id().await.map_err(|e| {
                tracing::error!("Failed to rotate session id: {}", e);
                AppError::Internal(format!("Session error: {}", e))
            })?;
            flash::set_flash(&session, "Password changed.", "success").await;
            Ok(settings_redirect())
        }
        Err(e) => {
            let html = render_fragment(&PasswordFormTemplate {
                error: e.message().to_string(),
            })?;
            Err(AppError::Form {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                html: html.0,
            })
        }
    }
}

fn settings_redirect() -> (StatusCode, HeaderMap) {
    let mut headers = HeaderMap::new();
    headers.insert("hx-redirect", "/settings".parse().unwrap());
    (StatusCode::OK, headers)
}

fn render_fragment<T: Template>(template: &T) -> Result<Html<String>, AppError> {
    render_template(template)
        .map(Html)
        .map_err(|(_, html)| AppError::Internal(html.0))
}
