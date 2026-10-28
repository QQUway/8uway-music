use axum::http::header::COOKIE;
use axum::http::HeaderMap;
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use time::OffsetDateTime;

pub const SESSION_COOKIE: &str = "8uway_session";

pub fn hash_token(token: &str) -> String {
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())
}

/// Creates a new session for `user`, returns the raw cookie token.
pub async fn create_session(
    pool: &SqlitePool,
    user: &str,
    ttl_hours: i64,
) -> anyhow::Result<String> {
    let mut token_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut token_bytes);
    let token = hex::encode(token_bytes);
    let expires =
        OffsetDateTime::now_utc() + time::Duration::hours(ttl_hours);
    let expires_str = expires
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into());

    sqlx::query("INSERT INTO sessions (token_hash, user, expires_at) VALUES (?, ?, ?)")
        .bind(hash_token(&token))
        .bind(user)
        .bind(expires_str)
        .execute(pool)
        .await?;

    Ok(token)
}

/// Returns the logged-in username if the request carries a valid session cookie.
pub async fn current_user(pool: &SqlitePool, headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    let token = parse_cookie_value(raw, SESSION_COOKIE)?;
    let token_hash = hash_token(&token);

    let row: Option<(String,)> =
        sqlx::query_as("SELECT user FROM sessions WHERE token_hash = ? AND expires_at > ?")
            .bind(&token_hash)
            .bind(now_rfc3339())
            .fetch_optional(pool)
            .await
            .ok()?;
    row.map(|(u,)| u)
}

pub async fn delete_session(pool: &SqlitePool, headers: &HeaderMap) {
    let Some(raw) = headers.get(COOKIE).and_then(|v| v.to_str().ok()) else {
        return;
    };
    let Some(token) = parse_cookie_value(raw, SESSION_COOKIE) else {
        return;
    };
    let _ = sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(hash_token(&token))
        .execute(pool)
        .await;
}

/// Removes expired sessions; called opportunistically on login.
pub async fn purge_expired(pool: &SqlitePool) {
    let _ = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
        .bind(now_rfc3339())
        .execute(pool)
        .await;
}

pub fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".into())
}

fn parse_cookie_value(header: &str, name: &str) -> Option<String> {
    header.split(';').find_map(|pair| {
        let mut kv = pair.trim().splitn(2, '=');
        let k = kv.next()?.trim();
        let v = kv.next()?.trim();
        (k == name).then(|| v.to_string())
    })
}

/// Builds the Set-Cookie header value.
pub fn session_cookie(token: &str, ttl_hours: i64, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" },
        max_age = ttl_hours * 3600
    )
}

pub fn clear_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}
