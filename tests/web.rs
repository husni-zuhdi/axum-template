mod common;

use common::{TEST_PASSWORD, TestClient, response_session_cookie, test_app};

#[tokio::test]
async fn auth_middleware_redirects_unauthenticated() {
    let (app, db) = test_app().await;
    let client = TestClient::new(app, db);

    let resp = client.get("/").await;
    assert_eq!(resp.status, axum::http::StatusCode::SEE_OTHER);
    assert_eq!(resp.headers.get("location").unwrap(), "/login");
}

#[tokio::test]
async fn login_invalid_password_rejected() {
    let (app, db) = test_app().await;
    let mut client = TestClient::new(app, db);

    let resp = client.login("wrong-password").await;
    assert_eq!(resp.status, axum::http::StatusCode::UNAUTHORIZED);
    assert!(resp.body.contains("login-error"));
}

#[tokio::test]
async fn login_valid_then_home_renders() {
    let (app, db) = test_app().await;
    let mut client = TestClient::new(app, db);

    let resp = client.login(TEST_PASSWORD).await;
    assert_eq!(resp.status, axum::http::StatusCode::OK);
    assert_eq!(resp.headers.get("hx-redirect").unwrap(), "/");

    let resp = client.get("/").await;
    assert_eq!(resp.status, axum::http::StatusCode::OK);
    assert!(resp.body.contains("Home"));
}

#[tokio::test]
async fn health_is_public() {
    let (app, db) = test_app().await;
    let client = TestClient::new(app, db);

    let resp = client.get("/health").await;
    assert_eq!(resp.status, axum::http::StatusCode::OK);
    assert!(resp.body.contains("ok"));
    assert!(resp.body.contains("settings"));
    assert!(resp.body.contains("session_store"));
}

#[tokio::test]
async fn post_without_csrf_token_rejected() {
    let (app, db) = test_app().await;
    let mut client = TestClient::new(app, db);

    let (cookie, _) = client.new_session().await;
    client.cookie = Some(cookie);

    let resp = client.post_form("/login", "password=whatever", None).await;
    assert_eq!(resp.status, axum::http::StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn password_change_flow() {
    let (app, db) = test_app().await;
    let mut client = TestClient::new(app, db);

    client.login(TEST_PASSWORD).await;

    let resp = client.get("/settings").await;
    assert_eq!(resp.status, axum::http::StatusCode::OK);
    assert!(resp.body.contains(">Password</h2>"));
    assert!(resp.body.contains(r#"name="current_password""#));

    let token = client.csrf_token().await.unwrap();

    // Wrong current password -> inline 422.
    let resp = client
        .post_form(
            "/settings/password",
            "current_password=wrong&new_password=New$ecret1&confirm_password=New$ecret1",
            Some(&token),
        )
        .await;
    assert_eq!(resp.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    assert!(resp.body.contains("Current password is incorrect."));

    // Weak new password -> inline 422.
    let resp = client
        .post_form(
            "/settings/password",
            "current_password=secret&new_password=weak&confirm_password=weak",
            Some(&token),
        )
        .await;
    assert_eq!(resp.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    assert!(resp.body.contains("at least 8 characters"));

    // Valid change -> hx-redirect, session id rotated (new cookie).
    let resp = client
        .post_form(
            "/settings/password",
            "current_password=secret&new_password=New$ecret1&confirm_password=New$ecret1",
            Some(&token),
        )
        .await;
    assert_eq!(
        resp.status,
        axum::http::StatusCode::OK,
        "body: {}",
        resp.body
    );
    assert_eq!(resp.headers.get("hx-redirect").unwrap(), "/settings");
    let rotated = response_session_cookie(&resp).expect("rotated session cookie");
    assert_ne!(rotated, client.cookie.as_deref().unwrap());
    client.cookie = Some(rotated);

    // Override stored; effective hash now the new one.
    let mut rows = client
        .db
        .query(
            "SELECT value FROM settings WHERE key = 'password_hash_override'",
            (),
        )
        .await
        .unwrap();
    let stored = rows
        .next()
        .await
        .unwrap()
        .unwrap()
        .get::<String>(0)
        .unwrap();
    assert!(stored.starts_with("$argon2id$"));

    // The current session survives the rotation.
    let resp = client.get("/settings").await;
    assert_eq!(resp.status, axum::http::StatusCode::OK);

    // The old password no longer verifies.
    let resp = client
        .post_form(
            "/settings/password",
            "current_password=secret&new_password=Another$ecret1&confirm_password=Another$ecret1",
            Some(&token),
        )
        .await;
    assert_eq!(resp.status, axum::http::StatusCode::UNPROCESSABLE_ENTITY);
    assert!(resp.body.contains("Current password is incorrect."));

    // New password logs in.
    let new = client.login("New$ecret1").await;
    assert_eq!(new.status, axum::http::StatusCode::OK);
}

#[tokio::test]
async fn logout_clears_session() {
    let (app, db) = test_app().await;
    let mut client = TestClient::new(app, db);

    client.login(TEST_PASSWORD).await;

    let resp = client.get("/logout").await;
    assert_eq!(resp.status, axum::http::StatusCode::SEE_OTHER);
    assert_eq!(resp.headers.get("location").unwrap(), "/login");

    // No session cookie carried -> unauthenticated again.
    client.cookie = None;
    let resp = client.get("/").await;
    assert_eq!(resp.status, axum::http::StatusCode::SEE_OTHER);
}
