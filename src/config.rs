use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub services: ServicesConfig,
    pub datasources: Vec<DatasourceConfig>,
    pub layers: Vec<LayerConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    8080
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServicesConfig {
    #[serde(default)]
    pub wms: ServiceConfig,
    #[serde(default)]
    pub wfs: ServiceConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServiceConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_service_title")]
    pub title: String,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            title: "CartoRS Service".to_string(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_service_title() -> String {
    "CartoRS Service".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatasourceConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub ds_type: String,
    pub path: Option<String>,
    pub connection_string: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LayerConfig {
    pub name: String,
    pub title: String,
    pub datasource: String,
    pub table: String,
    #[serde(default = "default_srs")]
    pub srs: String,
    #[serde(default = "default_geometry_column")]
    pub geometry_column: String,
    #[serde(default = "default_bbox")]
    pub bbox: [f64; 4],
}

fn default_srs() -> String {
    "EPSG:4326".to_string()
}

fn default_geometry_column() -> String {
    "geom".to_string()
}

fn default_bbox() -> [f64; 4] {
    [-180.0, -90.0, 180.0, 90.0]
}

impl Config {
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(path.display().to_string(), e))?;
        let config: Config = serde_yaml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(path.display().to_string(), e))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), ConfigError> {
        for layer in &self.layers {
            if !self.datasources.iter().any(|ds| ds.name == layer.datasource) {
                return Err(ConfigError::ValidationError(format!(
                    "Layer '{}' references unknown datasource '{}'",
                    layer.name, layer.datasource
                )));
            }
        }
        for ds in &self.datasources {
            match ds.ds_type.as_str() {
                "geopackage" => {
                    if ds.path.is_none() {
                        return Err(ConfigError::ValidationError(format!(
                            "Datasource '{}' of type 'geopackage' requires a 'path'",
                            ds.name
                        )));
                    }
                }
                other => {
                    return Err(ConfigError::ValidationError(format!(
                        "Unsupported datasource type '{}' for datasource '{}'",
                        other, ds.name
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to read config file '{0}': {1}")]
    ReadError(String, std::io::Error),
    #[error("Failed to parse config file '{0}': {1}")]
    ParseError(String, serde_yaml::Error),
    #[error("Configuration validation error: {0}")]
    ValidationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_config() {
        let yaml = r#"
server:
  host: "127.0.0.1"
  port: 9090
services:
  wms:
    enabled: true
    title: "Test WMS"
  wfs:
    enabled: false
    title: "Test WFS"
datasources:
  - name: "testdata"
    type: "geopackage"
    path: "/data/test.gpkg"
layers:
  - name: "countries"
    title: "World Countries"
    datasource: "testdata"
    table: "countries"
    srs: "EPSG:4326"
    geometry_column: "geom"
    bbox: [-180.0, -90.0, 180.0, 90.0]
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 9090);
        assert!(config.services.wms.enabled);
        assert!(!config.services.wfs.enabled);
        assert_eq!(config.datasources.len(), 1);
        assert_eq!(config.layers.len(), 1);
        assert_eq!(config.layers[0].name, "countries");
    }

    #[test]
    fn test_config_defaults() {
        let yaml = r#"
server: {}
services: {}
datasources: []
layers: []
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 8080);
        assert!(config.services.wms.enabled);
        assert!(config.services.wfs.enabled);
    }

    #[test]
    fn test_validation_unknown_datasource() {
        let yaml = r#"
server: {}
services: {}
datasources: []
layers:
  - name: "test"
    title: "Test"
    datasource: "nonexistent"
    table: "t"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown datasource"));
    }

    #[test]
    fn test_validation_geopackage_requires_path() {
        let yaml = r#"
server: {}
services: {}
datasources:
  - name: "ds"
    type: "geopackage"
layers: []
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        let result = config.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("requires a 'path'"));
    }
}
