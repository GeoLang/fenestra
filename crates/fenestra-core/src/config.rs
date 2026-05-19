use serde::{Deserialize, Serialize};

/// Server-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub title: String,
    pub abstract_text: String,
    pub host: String,
    pub port: u16,
    pub layers: Vec<LayerConfig>,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            title: "Fenestra OGC Server".to_string(),
            abstract_text: "OGC WMS/WFS/WMTS services".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8080,
            layers: Vec::new(),
        }
    }
}

/// Configuration for a single map layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerConfig {
    pub name: String,
    pub title: String,
    pub srs: Vec<String>,
    pub bbox: [f64; 4],
    /// Path to data source (file, directory, or connection string).
    pub source: String,
}
