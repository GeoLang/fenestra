//! OGC API Tiles — types for serving tiled map data.
//!
//! Implements tile matrix sets, tile addressing, and request/response types
//! per OGC Two Dimensional Tile Matrix Set and Tileset Metadata standard.

use serde::{Deserialize, Serialize};

/// A tile matrix set defines the tiling scheme.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileMatrixSet {
    pub id: String,
    pub title: Option<String>,
    pub crs: String,
    pub well_known_scale_set: Option<String>,
    pub tile_matrices: Vec<TileMatrix>,
}

/// A single zoom level within a tile matrix set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileMatrix {
    pub id: String,
    pub scale_denominator: f64,
    pub cell_size: f64,
    pub top_left_corner: [f64; 2],
    pub tile_width: u32,
    pub tile_height: u32,
    pub matrix_width: u32,
    pub matrix_height: u32,
}

/// Request for a specific tile.
#[derive(Debug, Clone, Deserialize)]
pub struct TileRequest {
    pub tile_matrix_set_id: String,
    pub tile_matrix: String,
    pub tile_row: u32,
    pub tile_col: u32,
    pub collections: Option<Vec<String>>,
    pub format: Option<String>,
}

/// Tileset metadata (returned by /tiles endpoint).
#[derive(Debug, Clone, Serialize)]
pub struct TileSetMetadata {
    pub title: Option<String>,
    pub tile_matrix_set_id: String,
    pub data_type: TileDataType,
    pub crs: String,
    pub links: Vec<TileLink>,
    pub bounds: Option<[f64; 4]>,
    pub center_point: Option<[f64; 2]>,
    pub min_tile_matrix: Option<String>,
    pub max_tile_matrix: Option<String>,
}

/// Type of data in tiles.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TileDataType {
    #[serde(rename = "map")]
    Map,
    #[serde(rename = "vector")]
    Vector,
    #[serde(rename = "coverage")]
    Coverage,
}

/// Link within tile metadata.
#[derive(Debug, Clone, Serialize)]
pub struct TileLink {
    pub href: String,
    pub rel: String,
    pub r#type: Option<String>,
}

/// Well-known tile matrix set: WebMercatorQuad (EPSG:3857, Google/OSM scheme).
pub fn web_mercator_quad() -> TileMatrixSet {
    let mut matrices = Vec::with_capacity(25);
    for z in 0..25u32 {
        let scale_denominator = 559_082_264.028_717_3 / f64::from(2u32.pow(z));
        let n = 2u32.pow(z);
        matrices.push(TileMatrix {
            id: z.to_string(),
            scale_denominator,
            cell_size: scale_denominator * 0.000_28,
            top_left_corner: [-20_037_508.342_789_244, 20_037_508.342_789_244],
            tile_width: 256,
            tile_height: 256,
            matrix_width: n,
            matrix_height: n,
        });
    }
    TileMatrixSet {
        id: "WebMercatorQuad".to_string(),
        title: Some("Google Maps Compatible for the World".to_string()),
        crs: "http://www.opengis.net/def/crs/EPSG/0/3857".to_string(),
        well_known_scale_set: Some(
            "http://www.opengis.net/def/wkss/OGC/1.0/GoogleMapsCompatible".to_string(),
        ),
        tile_matrices: matrices,
    }
}

/// Well-known tile matrix set: WorldCRS84Quad (EPSG:4326 / CRS84).
pub fn world_crs84_quad() -> TileMatrixSet {
    let mut matrices = Vec::with_capacity(25);
    for z in 0..25u32 {
        let scale_denominator = 279_541_132.014_358_7 / f64::from(2u32.pow(z));
        let n_cols = 2u32.pow(z + 1);
        let n_rows = 2u32.pow(z);
        matrices.push(TileMatrix {
            id: z.to_string(),
            scale_denominator,
            cell_size: scale_denominator * 0.000_28,
            top_left_corner: [-180.0, 90.0],
            tile_width: 256,
            tile_height: 256,
            matrix_width: n_cols,
            matrix_height: n_rows,
        });
    }
    TileMatrixSet {
        id: "WorldCRS84Quad".to_string(),
        title: Some("CRS84 for the World".to_string()),
        crs: "http://www.opengis.net/def/crs/OGC/1.3/CRS84".to_string(),
        well_known_scale_set: Some(
            "http://www.opengis.net/def/wkss/OGC/1.0/GoogleCRS84Quad".to_string(),
        ),
        tile_matrices: matrices,
    }
}

impl TileRequest {
    /// Get zoom level from tile_matrix.
    pub fn zoom(&self) -> Option<u32> {
        self.tile_matrix.parse().ok()
    }

    /// Compute the bounding box of this tile in web mercator.
    pub fn bbox_web_mercator(&self) -> [f64; 4] {
        let z = self.tile_matrix.parse::<u32>().unwrap_or(0);
        let n = f64::from(2u32.pow(z));
        let extent = 20_037_508.342_789_244;
        let tile_size = 2.0 * extent / n;
        let min_x = -extent + f64::from(self.tile_col) * tile_size;
        let max_x = min_x + tile_size;
        let max_y = extent - f64::from(self.tile_row) * tile_size;
        let min_y = max_y - tile_size;
        [min_x, min_y, max_x, max_y]
    }

    /// Compute bounding box of tile in CRS84 (lat/lon).
    pub fn bbox_crs84(&self) -> [f64; 4] {
        let z = self.tile_matrix.parse::<u32>().unwrap_or(0);
        let n_cols = f64::from(2u32.pow(z + 1));
        let n_rows = f64::from(2u32.pow(z));
        let tile_width = 360.0 / n_cols;
        let tile_height = 180.0 / n_rows;
        let min_x = -180.0 + f64::from(self.tile_col) * tile_width;
        let max_y = 90.0 - f64::from(self.tile_row) * tile_height;
        [min_x, max_y - tile_height, min_x + tile_width, max_y]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_web_mercator_quad() {
        let tms = web_mercator_quad();
        assert_eq!(tms.tile_matrices.len(), 25);
        assert_eq!(tms.tile_matrices[0].matrix_width, 1);
        assert_eq!(tms.tile_matrices[1].matrix_width, 2);
    }

    #[test]
    fn test_tile_bbox_web_mercator() {
        let req = TileRequest {
            tile_matrix_set_id: "WebMercatorQuad".to_string(),
            tile_matrix: "0".to_string(),
            tile_row: 0,
            tile_col: 0,
            collections: None,
            format: None,
        };
        let bbox = req.bbox_web_mercator();
        assert!((bbox[0] - (-20_037_508.342_789_244)).abs() < 1.0);
        assert!((bbox[2] - 20_037_508.342_789_244).abs() < 1.0);
    }

    #[test]
    fn test_tile_bbox_crs84() {
        let req = TileRequest {
            tile_matrix_set_id: "WorldCRS84Quad".to_string(),
            tile_matrix: "0".to_string(),
            tile_row: 0,
            tile_col: 0,
            collections: None,
            format: None,
        };
        let bbox = req.bbox_crs84();
        assert!((bbox[0] - (-180.0)).abs() < 0.001);
        assert!((bbox[3] - 90.0).abs() < 0.001);
    }
}
