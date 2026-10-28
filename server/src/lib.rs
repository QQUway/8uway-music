pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod routes;
pub mod state;
pub mod testutil;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, patch, post};
use axum::Router;
use state::AppState;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

pub fn build_router(state: AppState) -> Router {
    let max_upload = state.config.max_upload_bytes + 1024 * 1024;

    let api = Router::new()
        .route("/auth/login", post(routes::auth::login))
        .route("/auth/logout", post(routes::auth::logout))
        .route("/auth/me", get(routes::auth::me))
        .route("/demos", get(routes::demos::list_public).post(routes::demos::upload))
        .route("/demos/all", get(routes::demos::list_all))
        .route(
            "/demos/{id}",
            patch(routes::demos::toggle_published).delete(routes::demos::delete_demo),
        )
        .layer(DefaultBodyLimit::max(max_upload));

    let audio = ServeDir::new(state.config.audio_dir());

    let static_files = ServeDir::new(&state.config.frontend_dir)
        .not_found_service(ServeFile::new(state.config.frontend_dir.join("index.html")));

    Router::new()
        .nest("/api", api)
        .nest_service("/audio", audio)
        .fallback_service(static_files)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
