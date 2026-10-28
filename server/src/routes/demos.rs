use axum::extract::multipart::Multipart;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use rand::distr::{Alphanumeric, SampleString};
use serde::Serialize;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::routes::auth::require_admin;
use crate::state::{audio_path, AppState};

#[derive(Serialize, sqlx::FromRow)]
pub struct Demo {
    pub id: i64,
    pub title: String,
    pub slug: String,
    pub description: Option<String>,
    pub filename: String,
    pub mime: String,
    pub size_bytes: i64,
    pub duration: Option<f64>,
    pub published: bool,
    pub created_at: String,
}

const ALLOWED_EXT: &[(&str, &str, &[u8])] = &[
    ("mp3", "audio/mpeg", b"ID3"),
    ("mp3", "audio/mpeg", &[0xFF, 0xFB]),
    ("mp3", "audio/mpeg", &[0xFF, 0xFA]),
    ("mp3", "audio/mpeg", &[0xFF, 0xF3]),
    ("mp3", "audio/mpeg", &[0xFF, 0xF2]),
    ("ogg", "audio/ogg", b"OggS"),
    ("flac", "audio/flac", b"fLaC"),
    ("wav", "audio/wav", b"RIFF"),
];

pub async fn list_public(State(app): State<AppState>) -> AppResult<Json<Vec<Demo>>> {
    let demos = sqlx::query_as::<_, Demo>(
        "SELECT id, title, slug, description, filename, mime, size_bytes, duration, published, created_at \
         FROM demos WHERE published = 1 ORDER BY created_at DESC",
    )
    .fetch_all(&app.pool)
    .await?;
    Ok(Json(demos))
}

pub async fn list_all(
    State(app): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<Vec<Demo>>> {
    require_admin(&app, &headers).await?;
    let demos = sqlx::query_as::<_, Demo>(
        "SELECT id, title, slug, description, filename, mime, size_bytes, duration, published, created_at \
         FROM demos ORDER BY created_at DESC",
    )
    .fetch_all(&app.pool)
    .await?;
    Ok(Json(demos))
}

pub async fn upload(
    State(app): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    require_admin(&app, &headers).await?;

    let mut title: Option<String> = None;
    let mut description: Option<String> = None;
    let mut published = false;
    let mut file: Option<(String, Vec<u8>)> = None;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("bad multipart body: {e}")))?
    {
        match field.name().unwrap_or("") {
            "title" => {
                title = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| AppError::BadRequest("bad title field".into()))?,
                );
            }
            "description" => {
                description = Some(
                    field
                        .text()
                        .await
                        .map_err(|_| AppError::BadRequest("bad description field".into()))?,
                );
            }
            "published" => {
                published = field
                    .text()
                    .await
                    .map_err(|_| AppError::BadRequest("bad published field".into()))?
                    == "true";
            }
            "file" => {
                let filename = field.file_name().unwrap_or("upload.bin").to_string();
                let ext = filename
                    .rsplit('.')
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if !ALLOWED_EXT.iter().any(|(e, _, _)| *e == ext) {
                    return Err(AppError::BadRequest(format!(
                        "unsupported file type: .{ext} (allowed: mp3, ogg, flac, wav)"
                    )));
                }
                let mut buf: Vec<u8> = Vec::with_capacity(1024 * 1024);
                let mut too_big = false;
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|_| AppError::BadRequest("failed reading upload".into()))?
                {
                    if buf.len() + chunk.len() > app.config.max_upload_bytes {
                        too_big = true;
                        break;
                    }
                    buf.extend_from_slice(&chunk);
                }
                if too_big {
                    return Err(AppError::TooLarge);
                }
                if buf.is_empty() {
                    return Err(AppError::BadRequest("empty file".into()));
                }
                file = Some((ext, buf));
            }
            _ => {}
        }
    }

    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| AppError::BadRequest("title is required".into()))?;
    if title.len() > 120 {
        return Err(AppError::BadRequest("title too long (max 120)".into()));
    }
    if let Some(d) = description.as_ref() {
        if d.len() > 300 {
            return Err(AppError::BadRequest("description too long (max 300)".into()));
        }
    }
    let (ext, buf) = file.ok_or_else(|| AppError::BadRequest("audio file is required".into()))?;

    // Magic-byte sniff: first allowed entry whose signature matches.
    let head = &buf[..buf.len().min(16)];
    let mime = ALLOWED_EXT
        .iter()
        .find(|(_, _, sig)| head.len() >= sig.len() && &head[..sig.len()] == *sig)
        .map(|(_, m, _)| m)
        .ok_or_else(|| {
            AppError::BadRequest("file content does not look like a valid audio file".into())
        })?
        .to_string();

    let description = description.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());

    let filename = format!(
        "{}.{ext}",
        Uuid::new_v4().simple(),
        ext = ext
    );
    let path = audio_path(&app.config, &filename)
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("bad audio path")))?;

    tokio::fs::write(&path, &buf)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("write failed: {e}")))?;

    let slug = unique_slug(&app, &title).await?;
    let size = buf.len() as i64;

    let rec = sqlx::query(
        "INSERT INTO demos (title, slug, description, filename, mime, size_bytes, published) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&title)
    .bind(&slug)
    .bind(&description)
    .bind(&filename)
    .bind(&mime)
    .bind(size)
    .bind(published)
    .execute(&app.pool)
    .await;

    match rec {
        Ok(r) => {
            let demo = sqlx::query_as::<_, Demo>(
                "SELECT id, title, slug, description, filename, mime, size_bytes, duration, published, created_at \
                 FROM demos WHERE id = ?",
            )
            .bind(r.last_insert_rowid())
            .fetch_one(&app.pool)
            .await?;
            Ok(Json(demo))
        }
        Err(e) => {
            // Roll back the file so we don't leak orphans on DB failure.
            let _ = tokio::fs::remove_file(&path).await;
            Err(e.into())
        }
    }
}

pub async fn toggle_published(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    require_admin(&app, &headers).await?;
    let result = sqlx::query("UPDATE demos SET published = 1 - published WHERE id = ?")
        .bind(id)
        .execute(&app.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn delete_demo(
    State(app): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> AppResult<impl IntoResponse> {
    require_admin(&app, &headers).await?;

    let row = sqlx::query("SELECT filename FROM demos WHERE id = ?")
        .bind(id)
        .fetch_optional(&app.pool)
        .await?
        .ok_or(AppError::NotFound)?;
    let filename: String = row.get("filename");

    sqlx::query("DELETE FROM demos WHERE id = ?")
        .bind(id)
        .execute(&app.pool)
        .await?;

    if let Some(path) = audio_path(&app.config, &filename) {
        let _ = tokio::fs::remove_file(path).await;
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Slugifies a title; appends a random suffix if the slug already exists.
async fn unique_slug(app: &AppState, title: &str) -> AppResult<String> {
    let base = slugify(title);
    let mut candidate = base.clone();
    loop {
        let exists = sqlx::query("SELECT 1 FROM demos WHERE slug = ?")
            .bind(&candidate)
            .fetch_optional(&app.pool)
            .await?;
        if exists.is_none() {
            return Ok(candidate);
        }
        candidate = format!(
            "{base}-{suf}",
            suf = Alphanumeric.sample_string(&mut rand::rng(), 4).to_lowercase()
        );
    }
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = true; // avoid leading dash
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-').to_string();
    if trimmed.is_empty() {
        "demo".into()
    } else {
        trimmed.chars().take(64).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basics() {
        assert_eq!(slugify("limbo — first take"), "limbo-first-take");
        assert_eq!(slugify("  Hello, World! "), "hello-world");
        assert_eq!(slugify("🎵"), "demo");
        assert_eq!(slugify("///"), "demo");
        assert_eq!(slugify("a  b"), "a-b");
    }

    #[tokio::test]
    async fn unique_slug_suffixes_on_conflict() {
        let app = crate::testutil::test_app().await;
        let s1 = unique_slug(&app, "Night Take").await.unwrap();
        sqlx::query("INSERT INTO demos (title, slug, filename, mime, size_bytes) VALUES ('x', ?, 'f', 'audio/mpeg', 1)")
            .bind(&s1)
            .execute(&app.pool)
            .await
            .unwrap();
        let s2 = unique_slug(&app, "Night Take").await.unwrap();
        assert_eq!(s1, "night-take");
        assert!(s2.starts_with("night-take-"));
        assert_ne!(s1, s2);
    }

    #[tokio::test]
    async fn demo_roundtrip() {
        let app = crate::testutil::test_app().await;
        sqlx::query("INSERT INTO demos (title, slug, filename, mime, size_bytes, published) VALUES ('T', 't', 'f.mp3', 'audio/mpeg', 1, 1)")
            .execute(&app.pool)
            .await
            .unwrap();
        let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM demos WHERE published = 1")
            .fetch_one(&app.pool)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }
}
