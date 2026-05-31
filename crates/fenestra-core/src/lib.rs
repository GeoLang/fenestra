//! Fenestra — OGC services gateway.
//!
//! Protocol implementations for WMS, WFS, WMTS, WCS, OGC API Features,
//! OGC API Tiles, and OGC API Processes.
//! Provides request parsing, capability document generation,
//! server-side map rendering, and response formatting.

pub mod capabilities;
mod config;
mod error;
pub mod mvt;
pub mod ogcapi;
pub mod processes;
pub mod renderer;
pub mod sld;
pub mod tiles;
pub mod wcs;
mod wfs;
pub mod wms;
mod wmts;

pub use capabilities::ServiceMetadata;
pub use config::{LayerConfig, ServiceConfig};
pub use error::Error;
pub use ogcapi::{
    BboxFilter, CollectionInfo, ConformanceDeclaration, Feature, FeatureCollection, Geometry,
    LandingPage, Link, paginate_features,
};
pub use sld::{
    Fill, Graphic, LineSymbolizer, Mark, NamedLayer, PointSymbolizer, PolygonSymbolizer, Rule,
    Stroke, Style, StyledLayerDescriptor, Symbolizer, TextSymbolizer, parse_sld,
};
pub use wfs::{WfsGetFeatureRequest, WfsResponse};
pub use wms::{WmsGetMapRequest, WmsResponse};
pub use wmts::{WmtsGetTileRequest, WmtsResponse, wmts_capabilities_xml};
