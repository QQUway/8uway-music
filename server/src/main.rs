use uway_server::{build_router, config::Config, db, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    std::fs::create_dir_all(config.audio_dir())?;

    let pool = db::init_pool(&config.db_url()).await?;

    let state = AppState {
        pool,
        config: config.clone(),
        logins: Default::default(),
    };

    let app = build_router(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!(
        "listening on http://{addr} (frontend: {})",
        config.frontend_dir.display()
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
