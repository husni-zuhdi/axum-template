use askama::Template;
use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use tower_sessions::Session;

use crate::app::AppState;
use crate::error::AppError;
use crate::features::auth::password::verify_password;
use crate::utils::{csrf, flash, template::render_page};

#[derive(Deserialize)]
pub struct LoginForm {
    pub password: String,
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub error: String,
    pub csrf_token: String,
}

pub async fn auth_middleware(
    session: Session,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, Redirect> {
    let path = request.uri().path().to_string();
    tracing::debug!(path = %path, "Auth check");

    if path == "/login" || path == "/health" || path.starts_with("/static") {
        return Ok(next.run(request).await);
    }

    let is_authenticated = session
        .get::<bool>("authenticated")
        .await
        .unwrap_or(None)
        .unwrap_or(false);

    if is_authenticated {
        Ok(next.run(request).await)
    } else {
        flash::set_flash(&session, "Please log in to continue", "info").await;
        Err(Redirect::to("/login"))
    }
}

pub async fn login_page(session: Session) -> Result<Html<String>, AppError> {
    let template = LoginTemplate {
        error: String::new(),
        csrf_token: csrf::get_csrf_token(&session).await,
    };
    render_page(&template)
}

pub async fn login_submit(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> Result<(StatusCode, HeaderMap), AppError> {
    tracing::info!("Login attempt");
    let current_hash = state.effective_hash().await;
    let is_valid = verify_password(&form.password, &current_hash);

    if is_valid {
        session.insert("authenticated", true).await.map_err(|e| {
            tracing::error!("Failed to create session: {}", e);
            AppError::Internal(format!("Session error: {}", e))
        })?;
        session.cycle_id().await.map_err(|e| {
            tracing::error!("Failed to rotate session id: {}", e);
            AppError::Internal(format!("Session error: {}", e))
        })?;

        tracing::info!("Login successful");
        let mut headers = HeaderMap::new();
        headers.insert("hx-redirect", "/".parse().unwrap());
        Ok((StatusCode::OK, headers))
    } else {
        tracing::warn!("Login failed: invalid password");
        Err(AppError::Form {
            status: StatusCode::UNAUTHORIZED,
            html: r#"<div id="login-error" class="bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded mb-4">Invalid password. Please try again.</div>"#.to_string(),
        })
    }
}

pub async fn logout(session: Session) -> impl IntoResponse {
    let _ = session.remove::<bool>("authenticated").await;
    Redirect::to("/login")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_password_against_known_hash() {
        let hash = crate::features::auth::password::hash_password("secret").unwrap();
        assert!(verify_password("secret", &hash));
        assert!(!verify_password("password", &hash));
        assert!(!verify_password("secret", "not-a-hash"));
    }
}
