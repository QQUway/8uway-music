use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt;

use uway_server::testutil::{setup_router, test_app, TEST_PASSWORD, TEST_USER};

fn login_body() -> String {
    format!(r#"{{"username":"{TEST_USER}","password":"{TEST_PASSWORD}"}}"#)
}

async fn login(router: axum::Router) -> axum::http::Response<Body> {
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(login_body()))
        .unwrap();
    router.oneshot(req).await.unwrap()
}

fn session_cookie(res: &axum::http::Response<Body>) -> String {
    res.headers()
        .get("set-cookie")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap()
}

fn cookie_header_from(set_cookie: &str) -> String {
    set_cookie.split(';').next().unwrap().to_string()
}

#[tokio::test]
async fn login_rejects_bad_credentials() {
    let app = test_app().await;
    let router = setup_router(app);

    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"username":"admin","password":"wrong"}"#))
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // me() without session -> 401
    let req = Request::builder().uri("/api/auth/me").body(Body::empty()).unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_logout_me_flow() {
    let app = test_app().await;
    let router = setup_router(app);

    let res = login(router.clone()).await;
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = cookie_header_from(&session_cookie(&res));

    // me() with cookie -> 200
    let req = Request::builder()
        .uri("/api/auth/me")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // logout
    let req = Request::builder()
        .method("POST")
        .uri("/api/auth/logout")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // me() after logout -> 401
    let req = Request::builder()
        .uri("/api/auth/me")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn demo_upload_requires_auth() {
    let app = test_app().await;
    let router = setup_router(app);

    // Build a tiny valid mp3-ish file (ID3 header).
    let boundary = "XBOUNDARY";
    let body = format!(
        "--{boundary}\r\ncontent-disposition: form-data; name=\"title\"\r\n\r\nNight Take\r\n\
         --{boundary}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"take.mp3\"\r\n\
         content-type: audio/mpeg\r\n\r\n"
    );
    let mut bytes = body.into_bytes();
    bytes.extend_from_slice(b"ID3\x04\0\0\0\0\0\0fake-audio");
    bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/demos")
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(bytes))
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    // List stays empty for public.
    let req = Request::builder().uri("/api/demos").body(Body::empty()).unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let list: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap(),
    )
    .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn demo_upload_roundtrip_and_permissions() {
    let app = test_app().await;
    let audio_dir = app.config.audio_dir();
    let router = setup_router(app);

    // login
    let res = login(router.clone()).await;
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = cookie_header_from(&session_cookie(&res));

    // upload (published)
    let boundary = "XBOUNDARY";
    let body = format!(
        "--{boundary}\r\ncontent-disposition: form-data; name=\"title\"\r\n\r\nNight Take\r\n\
         --{boundary}\r\ncontent-disposition: form-data; name=\"published\"\r\n\r\ntrue\r\n\
         --{boundary}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"take.mp3\"\r\n\r\n"
    );
    let mut bytes = body.into_bytes();
    bytes.extend_from_slice(b"ID3\x04\0\0\0\0\0\0fake-audio-bytes-for-test");
    bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/demos")
        .header("cookie", &cookie)
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(bytes))
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK, "upload should succeed");
    let demo: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap(),
    )
    .unwrap();
    let id = demo["id"].as_i64().unwrap();
    assert_eq!(demo["slug"], "night-take");
    assert_eq!(demo["published"], true);

    // file landed in audio dir
    let filename = demo["filename"].as_str().unwrap();
    assert!(audio_dir.join(filename).exists());

    // public list shows it
    let req = Request::builder().uri("/api/demos").body(Body::empty()).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let list: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap(),
    )
    .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);

    // audio file is served
    let req = Request::builder()
        .uri(&format!("/audio/{filename}"))
        .body(Body::empty())
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // unpublish, then public list is empty but /all shows draft
    let req = Request::builder()
        .method("PATCH")
        .uri(&format!("/api/demos/{id}"))
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let req = Request::builder().uri("/api/demos").body(Body::empty()).unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let list: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap(),
    )
    .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 0);

    let req = Request::builder()
        .uri("/api/demos/all")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    let list: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap(),
    )
    .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["published"], false);

    // delete removes row + file
    let req = Request::builder()
        .method("DELETE")
        .uri(&format!("/api/demos/{id}"))
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(!audio_dir.join(filename).exists());

    let req = Request::builder()
        .uri("/api/demos/all")
        .header("cookie", &cookie)
        .body(Body::empty())
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    let list: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap(),
    )
    .unwrap();
    assert_eq!(list.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn upload_rejects_bad_content() {
    let app = test_app().await;
    let router = setup_router(app);

    let res = login(router.clone()).await;
    let cookie = cookie_header_from(&session_cookie(&res));

    // txt file with mp3 extension -> magic-byte mismatch
    let boundary = "XBOUNDARY";
    let body = format!(
        "--{boundary}\r\ncontent-disposition: form-data; name=\"title\"\r\n\r\nBad\r\n\
         --{boundary}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"evil.mp3\"\r\n\r\n"
    );
    let mut bytes = body.into_bytes();
    bytes.extend_from_slice(b"<?php echo 1; // not audio");
    bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let req = Request::builder()
        .method("POST")
        .uri("/api/demos")
        .header("cookie", &cookie)
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(bytes))
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn path_traversal_blocked_on_audio() {
    let app = test_app().await;
    let router = setup_router(app);

    let req = Request::builder()
        .uri("/audio/..%2F..%2Fetc%2Fpasswd")
        .body(Body::empty())
        .unwrap();
    let res = router.oneshot(req).await.unwrap();
    assert!(
        res.status() == StatusCode::NOT_FOUND || res.status() == StatusCode::BAD_REQUEST,
        "traversal must not return file contents"
    );
}
