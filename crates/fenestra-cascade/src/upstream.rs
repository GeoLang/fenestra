//! Upstream service configuration and health tracking.

use serde::{Deserialize, Serialize};

/// Type of upstream OGC service.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UpstreamType {
    Wms,
    Wmts,
    Wfs,
    OgcApiFeatures,
    OgcApiTiles,
}

/// Configuration for an upstream service to proxy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamService {
    /// Unique name for this upstream.
    pub name: String,
    /// Base URL of the upstream OGC service.
    pub url: String,
    /// Type of service.
    pub service_type: UpstreamType,
    /// Mapping of local layer names to remote layer names.
    /// Vec<(local_name, remote_name)>
    pub layer_mappings: Vec<(String, String)>,
    /// Cache TTL for responses from this upstream (seconds).
    pub cache_ttl_secs: u64,
    /// Request timeout (seconds).
    pub timeout_secs: u64,
    /// Maximum concurrent requests to this upstream.
    pub max_concurrent: u32,
    /// Optional authentication header value.
    pub auth_header: Option<String>,
}

/// Health status of an upstream service.
#[derive(Debug, Clone)]
pub struct UpstreamHealth {
    pub name: String,
    pub is_healthy: bool,
    pub last_check_ms: u64,
    pub response_time_ms: Option<u64>,
    pub error: Option<String>,
    pub consecutive_failures: u32,
}

impl UpstreamHealth {
    pub fn healthy(name: String, response_time_ms: u64) -> Self {
        Self {
            name,
            is_healthy: true,
            last_check_ms: 0,
            response_time_ms: Some(response_time_ms),
            error: None,
            consecutive_failures: 0,
        }
    }

    pub fn unhealthy(name: String, error: String, consecutive_failures: u32) -> Self {
        Self {
            name,
            is_healthy: false,
            last_check_ms: 0,
            response_time_ms: None,
            error: Some(error),
            consecutive_failures,
        }
    }
}
