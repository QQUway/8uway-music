use axum::Router;

use crate::config::Config;
use crate::state::AppState;

pub const TEST_USER: &str = "admin";
pub const TEST_PASSWORD: &str = "test-password";

pub fn test_config(data_dir: &std::path::Path) -> Config {
    Config {
        port: 0,
        data_dir: data_dir.to_path_buf(),
        frontend_dir: data_dir.join("frontend"),
        admin_user: TEST_USER.into(),
        admin_password_hash: hash_password(TEST_PASSWORD),
        session_ttl_hours: 1,
        max_upload_bytes: 5 * 1024 * 1024,
        cookie_secure: false,
    }
}

pub async fn test_app() -> AppState {
    let dir = tempdir();
    std::fs::create_dir_all(dir.join("audio")).unwrap();
    let config = test_config(&dir);
    let pool = crate::db::init_pool(&config.db_url()).await.unwrap();
    AppState {
        pool,
        config,
        logins: Default::default(),
    }
}

pub fn setup_router(state: AppState) -> Router {
    use axum::extract::connect_info::MockConnectInfo;
    crate::build_router(state).layer(MockConnectInfo(std::net::SocketAddr::from((
        [127, 0, 0, 1],
        40000,
    ))))
}

pub fn hash_password(password: &str) -> String {
    use rand_core::OsRng;
    use argon2::password_hash::{PasswordHasher, SaltString};
    use argon2::Argon2;

    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .unwrap()
        .to_string()
}

fn tempdir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("8uway-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    #[test]
    fn hash_parses_back() {
        use argon2::password_hash::PasswordHash;
        let h = super::hash_password("test-password");
        println!("HASH: {h}");
        match PasswordHash::new(&h) {
            Ok(_) => {}
            Err(e) => panic!("parse failed: {e:?}"),
        }
    }
}
