use axum::Router;
use axum::routing::get;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use crate::config::Config;
use crate::datasource::geopackage::GeoPackageSource;
use crate::datasource::DataSource;
use crate::service::{wfs, wms};

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub datasources: Arc<HashMap<String, Arc<dyn DataSource>>>,
}

/// Build the application router.
pub fn build_router(config: Config) -> Result<(Router, String), String> {
    let bind_addr = format!("{}:{}", config.host(), config.port());

    // Initialize datasources
    let mut datasources: HashMap<String, Arc<dyn DataSource>> = HashMap::new();
    for ds_config in &config.datasources {
        match ds_config.ds_type.as_str() {
            "geopackage" => {
                let path = ds_config.path.as_ref().ok_or_else(|| {
                    format!("Datasource '{}' requires a 'path'", ds_config.name)
                })?;
                let source = GeoPackageSource::new(path)
                    .map_err(|e| format!("Failed to initialize datasource '{}': {}", ds_config.name, e))?;
                datasources.insert(ds_config.name.clone(), Arc::new(source));
            }
            other => {
                return Err(format!("Unsupported datasource type: {}", other));
            }
        }
    }

    let state = AppState {
        config: Arc::new(config),
        datasources: Arc::new(datasources),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/wms", get(wms::handle_wms))
        .route("/wfs", get(wfs::handle_wfs))
        .route("/health", get(health_check))
        .with_state(state)
        .layer(cors);

    Ok((app, bind_addr))
}

async fn health_check() -> &'static str {
    "OK"
}

impl Config {
    pub fn host(&self) -> &str {
        &self.server.host
    }

    pub fn port(&self) -> u16 {
        self.server.port
    }
}
