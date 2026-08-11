use askama::Template;
use axum::{extract::State, response::Html};
use tower_sessions::Session;

use crate::app::AppState;
use crate::error::AppError;
use crate::utils::{csrf, flash, template::render_page};

#[derive(Template)]
#[template(path = "home.html")]
pub struct HomeTemplate {
    pub title: String,
    pub flash_kind: String,
    pub flash_message: String,
    pub csrf_token: String,
}

pub async fn home(session: Session) -> Result<Html<String>, AppError> {
    let (flash_kind, flash_message) = flash::get_flash(&session).await;
    let template = HomeTemplate {
        title: "Home".to_string(),
        flash_kind,
        flash_message,
        csrf_token: csrf::get_csrf_token(&session).await,
    };
    render_page(&template)
}

#[derive(Template)]
#[template(path = "health.html")]
pub struct HealthTemplate {
    pub status: String,
    pub tables: Vec<String>,
}

/// Public health check: reports the app status and the tables known to the
/// database. Not behind auth so load balancers can probe it.
pub async fn health(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let template = HealthTemplate {
        status: "ok".to_string(),
        tables: state.settings_repo().health_tables().await,
    };
    render_page(&template)
}
