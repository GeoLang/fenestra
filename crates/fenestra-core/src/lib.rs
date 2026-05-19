//! Fenestra — OGC services gateway.
//!
//! Protocol implementations for WMS, WFS, WMTS, and WCS.
//! Provides request parsing, capability document generation,
//! and response formatting.

pub mod capabilities;
mod config;
mod error;
mod wfs;
mod wms;

pub use capabilities::ServiceMetadata;
pub use config::{LayerConfig, ServiceConfig};
pub use error::Error;
pub use wfs::{WfsGetFeatureRequest, WfsResponse};
pub use wms::{WmsGetMapRequest, WmsResponse};
