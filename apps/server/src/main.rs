mod api;
mod app_state;
mod appearance;
mod db;
mod demo;
mod events;
mod models;
mod office;
mod websocket;

use std::{env, path::Path};

use tokio::net::TcpListener;

use crate::app_state::AppState;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG")
                .unwrap_or_else(|_| "ai_company_server=info,tower_http=info".to_string()),
        )
        .init();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/ai_company.db?mode=rwc".to_string());
    ensure_database_directory(&database_url)?;
    let pool = db::initialize(&database_url).await?;
    let state = AppState::new(pool);

    let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("PORT").unwrap_or_else(|_| "3001".to_string());
    let address = format!("{host}:{port}");
    let listener = TcpListener::bind(&address).await?;
    tracing::info!(%address, "AI Company server listening");
    axum::serve(listener, api::router(state)).await?;
    Ok(())
}

fn ensure_database_directory(database_url: &str) -> Result<(), std::io::Error> {
    let Some(path_part) = database_url.strip_prefix("sqlite://") else {
        return Ok(());
    };
    let path = path_part.split('?').next().unwrap_or(path_part);
    if let Some(parent) = Path::new(path).parent() {
        if parent != Path::new("") {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}
