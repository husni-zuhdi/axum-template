use axum::Router;
use axum_template::app::{AppState, create_router};
use axum_template::db;
use axum_template::utils::session_store::SqliteSessionStore;

pub const TEST_PASSWORD: &str = "secret";

/// Builds the real router over an in-memory database with a deterministic
/// argon2id hash for `TEST_PASSWORD` (fixed salt), so tests can log in without
/// knowing a brittle PHC string.
pub async fn test_app() -> (Router, libsql::Connection) {
    let db = db::init_test_database().await.unwrap();
    let session_store = SqliteSessionStore::new(db.clone());

    let password_hash = hash_password(TEST_PASSWORD);
    let cache = moka::future::Cache::builder().build();

    let state = AppState::new(db.clone(), password_hash, cache, session_store);

    (create_router(state), db)
}

/// Deterministic argon2id hash of `TEST_PASSWORD` (fixed salt).
pub fn hash_password(password: &str) -> String {
    argon2::password_hash::PasswordHasher::hash_password(
        &argon2::Argon2::default(),
        password.as_bytes(),
        &argon2::password_hash::SaltString::from_b64("c2FsdHNhbHRzYWx0c2FsdA").unwrap(),
    )
    .unwrap()
    .to_string()
}

/// HTTP client for exercising the app router via `tower::ServiceExt::oneshot`.
/// Carries the session cookie across requests and can read the CSRF token from
/// the session store (mirroring what the HTML pages do for real users).
pub struct TestClient {
    router: Router,
    pub db: libsql::Connection,
    pub cookie: Option<String>,
}

pub struct Response {
    pub status: axum::http::StatusCode,
    pub headers: axum::http::HeaderMap,
    pub body: String,
}

impl TestClient {
    pub fn new(router: Router, db: libsql::Connection) -> Self {
        Self {
            router,
            db,
            cookie: None,
        }
    }

    pub async fn get(&self, path: &str) -> Response {
        let req = axum::http::Request::builder()
            .uri(path)
            .header(axum::http::header::COOKIE, self.cookie())
            .body(axum::body::Body::empty())
            .unwrap();
        self.send(req).await
    }

    pub async fn post_form(&self, path: &str, form: &str, csrf_token: Option<&str>) -> Response {
        let mut builder = axum::http::Request::builder()
            .method("POST")
            .uri(path)
            .header(
                axum::http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header(axum::http::header::COOKIE, self.cookie());
        if let Some(token) = csrf_token {
            builder = builder.header("x-csrf-token", token);
        }
        let req = builder
            .body(axum::body::Body::from(form.to_string()))
            .unwrap();
        self.send(req).await
    }

    /// Performs the login dance: GET /login to obtain a session cookie and
    /// CSRF token, then POST /login with the given password. Adopts the rotated
    /// session cookie from the login response (login cycles the session id).
    pub async fn login(&mut self, password: &str) -> Response {
        let (cookie, token) = self.new_session().await;
        self.cookie = Some(cookie);
        let resp = self
            .post_form("/login", &format!("password={}", password), Some(&token))
            .await;
        if let Some(cookie) = extract_session_cookie(&resp) {
            self.cookie = Some(cookie);
        }
        resp
    }

    /// Starts a fresh session (GET /login) and returns `(cookie, csrf_token)`.
    pub async fn new_session(&self) -> (String, String) {
        let req = axum::http::Request::builder()
            .uri("/login")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = self.send(req).await;

        let cookie = extract_session_cookie(&resp).expect("expected session cookie");
        let token = self.csrf_token_from_db(&cookie).await;
        (cookie, token)
    }

    async fn csrf_token_from_db(&self, cookie: &str) -> String {
        let sid = cookie.split('=').nth(1).unwrap_or_default();
        let mut rows = self
            .db
            .query("SELECT data FROM session_store WHERE id = ?1", [sid])
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let data: String = row.get(0).unwrap();
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        json["data"]["csrf_token"].as_str().unwrap().to_string()
    }

    /// CSRF token for the current session (when a session cookie is set).
    pub async fn csrf_token(&self) -> Option<String> {
        let cookie = self.cookie.as_deref()?;
        Some(self.csrf_token_from_db(cookie).await)
    }

    fn cookie(&self) -> &str {
        self.cookie.as_deref().unwrap_or("")
    }

    pub async fn send(&self, mut req: axum::http::Request<axum::body::Body>) -> Response {
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [127, 0, 0, 1],
                0,
            ))));
        let resp = tower::ServiceExt::oneshot(self.router.clone(), req)
            .await
            .unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = String::from_utf8_lossy(&bytes).to_string();
        Response {
            status,
            headers,
            body,
        }
    }
}

fn extract_session_cookie(resp: &Response) -> Option<String> {
    resp.headers
        .get_all(axum::http::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|c| c.starts_with("id="))
        .map(|c| c.split(';').next().unwrap_or("").to_string())
}

/// The session cookie set by a response, if any (used to adopt a rotated
/// session id after login or a password change).
#[allow(dead_code)]
pub fn response_session_cookie(resp: &Response) -> Option<String> {
    extract_session_cookie(resp)
}
