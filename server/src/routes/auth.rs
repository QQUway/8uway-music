use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use std::net::SocketAddr;

use crate::auth;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn login(
    State(app): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    _headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> AppResult<impl IntoResponse> {
    let ip = addr.ip();
    {
        let logins = app.logins.lock().await;
        if let Some(w) = logins.get(&ip) {
            if w.blocked() {
                return Err(AppError::RateLimited);
            }
        }
    }

    auth::purge_expired(&app.pool).await;

    let user_ok = req.username == app.config.admin_user;
    let pass_ok = verify_password(&req.password, &app.config.admin_password_hash).unwrap_or(false);

    if !(user_ok && pass_ok) {
        let mut logins = app.logins.lock().await;
        logins.entry(ip).or_default().register_fail();
        // Constant-ish response regardless of which half failed.
        return Err(AppError::Unauthorized);
    }

    let token = auth::create_session(&app.pool, &req.username, app.config.session_ttl_hours).await?;
    app.logins.lock().await.remove(&ip);

    let cookie = auth::session_cookie(&token, app.config.session_ttl_hours, app.config.cookie_secure);
    let mut res = (StatusCode::OK, Json(json!({ "user": req.username }))).into_response();
    if let Ok(v) = cookie.parse() {
        res.headers_mut().append("set-cookie", v);
    }
    Ok(res)
}

pub async fn logout(
    State(app): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    auth::delete_session(&app.pool, &headers).await;
    let cookie = auth::clear_cookie(app.config.cookie_secure);
    let mut res = StatusCode::OK.into_response();
    if let Ok(v) = cookie.parse() {
        res.headers_mut().append("set-cookie", v);
    }
    res
}

pub async fn me(State(app): State<AppState>, headers: HeaderMap) -> AppResult<Json<serde_json::Value>> {
    match auth::current_user(&app.pool, &headers).await {
        Some(user) => Ok(Json(json!({ "user": user }))),
        None => Err(AppError::Unauthorized),
    }
}

/// Requires a valid session; returns Err(Unauthorized) otherwise.
pub async fn require_admin(app: &AppState, headers: &HeaderMap) -> AppResult<String> {
    auth::current_user(&app.pool, headers)
        .await
        .ok_or(AppError::Unauthorized)
}

fn verify_password(password: &str, phc_hash: &str) -> anyhow::Result<bool> {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    use argon2::Argon2;

    let parsed = PasswordHash::new(phc_hash)
        .map_err(|e| anyhow::anyhow!("bad stored hash: {e}"))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// Used by tests to confirm a session row lands in the DB.
pub async fn session_count(pool: &sqlx::SqlitePool) -> anyhow::Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM sessions")
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>("n"))
}
