use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub data_dir: PathBuf,
    pub frontend_dir: PathBuf,
    pub admin_user: String,
    pub admin_password_hash: String,
    pub session_ttl_hours: i64,
    pub max_upload_bytes: usize,
    pub cookie_secure: bool,
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

impl Config {
    /// Loads configuration from the environment, applying sane defaults.
    pub fn from_env() -> anyhow::Result<Self> {
        let port = env_or("MUSIC_PORT", "8080").parse::<u16>()?;
        let data_dir = PathBuf::from(env_or("MUSIC_DATA_DIR", "./data"));
        let frontend_dir = PathBuf::from(env_or(
            "MUSIC_FRONTEND_DIR",
            "../frontend/dist",
        ));
        let admin_user = env_or("MUSIC_ADMIN_USER", "admin");
        let admin_password_hash = env_or(
            "MUSIC_ADMIN_PASSWORD_HASH",
            // Default: "changeme" — MUST be overridden in production.
            "$argon2id$v=19$m=19456,t=2,p=1$ZGV2LW9ubHktZGVmYXVsdA$c29tZXRoaW5nc2hvdWxkY2hhbmdl",
        );
        let session_ttl_hours = env_or("MUSIC_SESSION_TTL_HOURS", "72").parse::<i64>()?;
        let max_upload_bytes = env_or("MUSIC_MAX_UPLOAD_MB", "50").parse::<usize>()? * 1024 * 1024;
        let cookie_secure = env_or("MUSIC_COOKIE_SECURE", "false") == "true";

        Ok(Self {
            port,
            data_dir,
            frontend_dir,
            admin_user,
            admin_password_hash,
            session_ttl_hours,
            max_upload_bytes,
            cookie_secure,
        })
    }

    pub fn audio_dir(&self) -> PathBuf {
        self.data_dir.join("audio")
    }

    pub fn db_url(&self) -> String {
        format!("sqlite://{}/8uway.db?mode=rwc", self.data_dir.display())
    }
}
