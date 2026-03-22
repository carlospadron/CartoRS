mod config;
mod datasource;
mod error;
mod rendering;
mod server;
mod service;

use clap::Parser;
use std::path::PathBuf;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "cartors")]
#[command(about = "A high-performance, headless GIS engine built in Rust")]
struct Cli {
    /// Path to the YAML configuration file
    #[arg(short, long, default_value = "config.yaml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    info!("CartoRS starting up");
    info!("Loading configuration from: {}", cli.config.display());

    let config = match config::Config::from_file(&cli.config) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    info!(
        "Configuration loaded: {} datasource(s), {} layer(s)",
        config.datasources.len(),
        config.layers.len()
    );

    let (app, bind_addr) = match server::build_router(config) {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to initialize server: {}", e);
            std::process::exit(1);
        }
    };

    info!("Starting server on {}", bind_addr);
    info!("WMS endpoint: http://{}/wms", bind_addr);
    info!("WFS endpoint: http://{}/wfs", bind_addr);
    info!("Health check: http://{}/health", bind_addr);

    let listener = match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind to {}: {}", bind_addr, e);
            std::process::exit(1);
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        error!("Server error: {}", e);
        std::process::exit(1);
    }
}
