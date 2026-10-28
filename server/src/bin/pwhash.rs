use anyhow::bail;

fn main() -> anyhow::Result<()> {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;

    let password = match std::env::args().nth(1) {
        Some(p) if !p.is_empty() => p,
        _ => {
            bail!("usage: pwhash <password>\n\nPrints an Argon2id PHC string. Set it via MUSIC_ADMIN_PASSWORD_HASH.");
        }
    };

    if password.len() < 8 {
        bail!("password must be at least 8 characters");
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing failed: {e}"))?;
    println!("{hash}");
    Ok(())
}
