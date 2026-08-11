use axum_template::{
    app::{AppState, create_router},
    config::Config,
    db,
    utils::session_store::SqliteSessionStore,
};
use tower_sessions::session_store::ExpiredDeletion;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let config = Config::from_env();

    let password_hash = config
        .password_hash
        .clone()
        .expect("APP_PASSWORD_HASH environment variable is required but not set");

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "axum_template=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::debug!("Initialize DB");
    let db = db::init_database(&config)
        .await
        .expect("Failed to initialize database");

    tracing::debug!("Initialize Cache");
    let cache = moka::future::Cache::builder()
        .max_capacity(1024)
        .time_to_live(std::time::Duration::from_secs(3600))
        .time_to_idle(std::time::Duration::from_secs(600))
        .build();

    let session_store = SqliteSessionStore::new(db.clone());
    let state = AppState::new(db, password_hash, cache, session_store);

    let cleanup_store = state.session_store.clone();
    tokio::task::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
            if let Err(e) = cleanup_store.delete_expired().await {
                tracing::error!("Session cleanup task failed: {}", e);
            }
        }
    });

    let app = create_router(state);

    let addr = config.bind_addr();

    tracing::info!("Server starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .expect("Failed to start server");
}
