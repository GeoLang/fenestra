//! Spatial constraints for geofencing.

use serde::{Deserialize, Serialize};

/// A spatial constraint that limits data visibility to a geographic area.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialConstraint {
    /// WKT representation of the allowed area.
    pub wkt: String,
    /// SRID of the constraint geometry.
    pub srid: u32,
    /// Whether to clip features to the constraint boundary (vs. just filter).
    pub clip: bool,
}

impl SpatialConstraint {
    /// Creates a new bounding-box spatial constraint.
    pub fn from_bbox(west: f64, south: f64, east: f64, north: f64, srid: u32) -> Self {
        let wkt = format!(
            "POLYGON(({w} {s},{e} {s},{e} {n},{w} {n},{w} {s}))",
            w = west,
            s = south,
            e = east,
            n = north
        );
        Self {
            wkt,
            srid,
            clip: false,
        }
    }
}
