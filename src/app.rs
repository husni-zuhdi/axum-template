use axum::Router;
use axum::middleware::from_fn;
use axum_tower_sessions_csrf::CsrfMiddleware;
use libsql::Connection;
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tower_sessions::SessionManagerLayer;

use crate::features::auth::handlers as auth;
use crate::features::auth::password::{self, PasswordError};
use crate::features::home::handlers as home;
use crate::features::settings::handlers as settings;
use crate::services::settings::SettingsRepository;
use crate::utils::session_store::SqliteSessionStore;

#[derive(Clone)]
pub struct AppState {
    db: Connection,
    password_hash: String,
    cache: moka::future::Cache<String, serde_json::Value>,
    pub session_store: SqliteSessionStore,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Connection,
        password_hash: String,
        cache: moka::future::Cache<String, serde_json::Value>,
        session_store: SqliteSessionStore,
    ) -> Self {
        Self {
            db,
            password_hash,
            cache,
            session_store,
        }
    }

    pub fn settings_repo(&self) -> SettingsRepository<'_> {
        SettingsRepository::new(&self.db)
    }

    pub async fn effective_hash(&self) -> String {
        password::effective_hash(&self.db, &self.password_hash).await
    }

    pub async fn change_password(
        &self,
        current: &str,
        new: &str,
        confirm: &str,
    ) -> Result<(), PasswordError> {
        password::change_password(&self.db, &self.password_hash, current, new, confirm).await
    }

    /// Returns the shared cache handle so services can read/write entries.
    pub fn cache(&self) -> &moka::future::Cache<String, serde_json::Value> {
        &self.cache
    }
}

pub fn create_router(state: AppState) -> Router {
    let session_layer = SessionManagerLayer::new(state.session_store.clone());

    let governor_conf = tower_governor::governor::GovernorConfigBuilder::default()
        .period(Duration::from_secs(60))
        .burst_size(5)
        .finish()
        .unwrap();

    let governor_limiter = governor_conf.limiter().clone();
    let interval = Duration::from_secs(60);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(interval);
            governor_limiter.retain_recent();
        }
    });

    let login_rate_limit =
        ServiceBuilder::new().layer(tower_governor::GovernorLayer::new(governor_conf));

    let protected_routes = Router::new()
        .route("/", axum::routing::get(home::home))
        .route("/logout", axum::routing::get(auth::logout))
        .route("/settings", axum::routing::get(settings::settings_page))
        .route(
            "/settings/password",
            axum::routing::post(settings::password_change),
        );

    let login_routes = Router::new()
        .route(
            "/login",
            axum::routing::get(auth::login_page).post(auth::login_submit),
        )
        .layer(login_rate_limit);

    let public_routes = Router::new()
        .route("/health", axum::routing::get(home::health))
        .nest_service("/static", ServeDir::new("static"));

    Router::new()
        .merge(protected_routes)
        .merge(login_routes)
        .merge(public_routes)
        .layer(axum::middleware::from_fn(auth::auth_middleware))
        .layer(from_fn(CsrfMiddleware::middleware))
        .layer(session_layer)
        .with_state(state)
}
