use std::collections::HashMap;
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Instant;

use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub config: Config,
    pub logins: std::sync::Arc<Mutex<HashMap<IpAddr, AttemptWindow>>>,
}

#[derive(Default)]
pub struct AttemptWindow {
    pub fails: u32,
    pub window_start: Option<Instant>,
}

impl AttemptWindow {
    /// 8 failed attempts per 15 minutes per IP.
    pub const MAX_FAILS: u32 = 8;
    pub const WINDOW: std::time::Duration = std::time::Duration::from_secs(15 * 60);

    pub fn register_fail(&mut self) {
        let now = Instant::now();
        match self.window_start {
            Some(t) if now.duration_since(t) > Self::WINDOW => {
                self.window_start = Some(now);
                self.fails = 1;
            }
            Some(_) => self.fails += 1,
            None => {
                self.window_start = Some(now);
                self.fails = 1;
            }
        }
    }

    pub fn blocked(&self) -> bool {
        match self.window_start {
            Some(t) => {
                Instant::now().duration_since(t) <= Self::WINDOW
                    && self.fails >= Self::MAX_FAILS
            }
            None => false,
        }
    }
}

pub fn audio_path(config: &Config, filename: &str) -> Option<PathBuf> {
    // Defense against path traversal: only a bare filename may pass.
    let name = PathBuf::from(filename);
    if name.components().count() != 1 {
        return None;
    }
    let name = name.to_str()?;
    if name.starts_with('.') {
        return None;
    }
    Some(config.audio_dir().join(name))
}
