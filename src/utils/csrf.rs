use axum_tower_sessions_csrf::get_or_create_token;
use tower_sessions::Session;

pub async fn get_csrf_token(session: &Session) -> String {
    match get_or_create_token(session).await {
        Ok(token) => token,
        Err(e) => {
            tracing::error!("Failed to get CSRF token: {}", e);
            String::new()
        }
    }
}
