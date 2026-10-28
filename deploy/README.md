# Deploy — music.rifqiurahman.com

Target: private server, Caddy reverse proxy → Axum app on `127.0.0.1:8080`.

## 1. Server prereqs

```sh
# Debian/Ubuntu example
sudo apt install -y caddy sqlite3
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Node (for building frontend) — or build locally and rsync dist/
```

## 2. Put the repo on the server

```sh
sudo mkdir -p /srv/8uway && sudo chown $USER /srv/8uway
git clone <your-repo> /srv/8uway   # or rsync -a --exclude node_modules --exclude target . server:/srv/8uway/
```

## 3. Build

```sh
cd /srv/8uway/frontend
npm ci && npm run build

cd /srv/8uway/server
cargo build --release
```

## 4. Admin password

```sh
cd /srv/8uway/server
cargo run -q --bin pwhash -- 'YOUR-STRONG-PASSWORD'
# copies as: $argon2id$v=19$m=19456,t=2,p=1$...
```

## 5. Data dir

```sh
sudo mkdir -p /srv/8uway/data
sudo chown www-data:www-data /srv/8uway/data
```

## 6. systemd

```sh
sudo cp /srv/8uway/deploy/8uway.service /etc/systemd/system/
sudoedit /etc/systemd/system/8uway.service   # set MUSIC_ADMIN_PASSWORD_HASH
sudo systemctl daemon-reload
sudo systemctl enable --now 8uway
curl -s http://127.0.0.1:8080/api/demos      # → []
```

## 7. Caddy

```sh
sudo cp /srv/8uway/deploy/Caddyfile /etc/caddy/Caddyfile   # or import it
sudo systemctl reload caddy
```

Point DNS `music.rifqiurahman.com` A/AAAA record at the server. Caddy obtains the TLS cert automatically on first request.

## 8. Verify

- `https://music.rifqiurahman.com` — home page renders
- `/demos` — empty list, fine
- `/admin` — login with your password, upload an mp3, confirm it appears on `/demos` and plays
- Audio seeking works (HTTP range requests)

## Restore / migrate

All state lives in `MUSIC_DATA_DIR` (`/srv/8uway/data`):

- `8uway.db` — SQLite (demos + sessions)
- `audio/` — uploaded files

Backup: `sqlite3 data/8uway.db '.backup backup.db' && tar czf 8uway-data.tgz backup.db audio/`.
Restore: stop service, untar into data dir, start service.

## Updating

```sh
cd /srv/8uway && git pull
cd frontend && npm ci && npm run build
cd ../server && cargo build --release
sudo systemctl restart 8uway
```
