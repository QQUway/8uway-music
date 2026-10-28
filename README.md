# 8Uway — Music Portfolio

Personal music portfolio for **8Uway** (Rifqi Habib Ur Rahman). Static Astro frontend + Rust (Axum) backend for demo-song hosting with browser-based upload/management.

## Features

- **Home** — bio hero, song cards with embeds:
  - *limbo* — YouTube embed + YouTube Music link
  - *Melancholic Ensemble* (Northanger) — Spotify embed
- **Covers** — TikTok covers from [@kiki.arloji](https://www.tiktok.com/@kiki.arloji) + gig photo strip
- **Demos** — public list of published demo songs with a sticky bottom audio player
- **Admin** (`/admin`) — login, upload demos (mp3/ogg/flac/wav, ≤50 MB, content-sniffed), publish/unpublish, delete
- Rust API: session auth (argon2 + HttpOnly cookie), login rate-limiting (8 fails / 15 min / IP), path-traversal-safe audio serving, HTTP range support for seeking

## Layout

```
frontend/   Astro 5 static site (npm run build → frontend/dist)
server/     Axum API + static file server (serves frontend/dist + /audio + /api)
deploy/     systemd unit + Caddyfile for music.rifqiurahman.com
```

## Local development

Prereqs: Node 20+, Rust 1.75+.

```sh
# terminal 1 — API on :8080
cd server
MUSIC_ADMIN_PASSWORD_HASH="$(cargo run -q --bin pwhash -- devpass)" cargo run

# terminal 2 — Astro dev server on :4321 (proxies /api + /audio to :8080)
cd frontend
npm install
npm run dev
```

Default dev login: `admin` / `devpass` (only if you exported the hash as above; otherwise the compiled-in placeholder accepts nothing).

## Production build

```sh
cd frontend && npm ci && npm run build
cd ../server && cargo build --release
```

`server/target/release/server` serves everything on `:8080` — frontend, API, and audio.

## Deployment (private server)

See `deploy/README.md` for the full step-by-step (systemd + Caddy + TLS for music.rifqiurahman.com).

## Configuration (env vars)

| Variable | Default | Notes |
|---|---|---|
| `MUSIC_PORT` | `8080` | |
| `MUSIC_DATA_DIR` | `./data` | SQLite DB + uploaded audio live here |
| `MUSIC_FRONTEND_DIR` | `../frontend/dist` | |
| `MUSIC_ADMIN_USER` | `admin` | |
| `MUSIC_ADMIN_PASSWORD_HASH` | (placeholder) | **Required.** Output of `cargo run --bin pwhash -- 'pass'` |
| `MUSIC_SESSION_TTL_HOURS` | `72` | |
| `MUSIC_MAX_UPLOAD_MB` | `50` | |
| `MUSIC_COOKIE_SECURE` | `false` | Set `true` behind HTTPS |

## Tests

```sh
cd server && cargo test   # 10 tests: auth flow, upload roundtrip, magic-byte sniffing, traversal
```
