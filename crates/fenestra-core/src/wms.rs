use crate::Error;
use serde::Deserialize;

/// Parsed WMS GetMap request parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct WmsGetMapRequest {
    pub layers: String,
    pub styles: String,
    pub crs: String,
    pub bbox: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
}

impl WmsGetMapRequest {
    /// Parse bbox string "minx,miny,maxx,maxy" into array.
    pub fn parse_bbox(&self) -> Result<[f64; 4], Error> {
        let parts: Vec<&str> = self.bbox.split(',').collect();
        if parts.len() != 4 {
            return Err(Error::InvalidRequest("bbox must have 4 values".to_string()));
        }
        let vals: Result<Vec<f64>, _> = parts.iter().map(|s| s.parse::<f64>()).collect();
        let vals = vals.map_err(|_| Error::InvalidRequest("invalid bbox values".to_string()))?;
        Ok([vals[0], vals[1], vals[2], vals[3]])
    }

    /// Parse layers string into individual layer names.
    pub fn layer_names(&self) -> Vec<&str> {
        self.layers.split(',').collect()
    }
}

/// WMS response (placeholder for actual image generation).
#[derive(Debug)]
pub struct WmsResponse {
    pub content_type: String,
    pub body: Vec<u8>,
}

impl WmsResponse {
    /// Create a placeholder PNG response (1x1 transparent pixel).
    pub fn placeholder(width: u32, height: u32) -> Self {
        // Minimal valid PNG: 1x1 transparent pixel header
        // In production this would render actual map tiles
        let _ = (width, height);
        Self {
            content_type: "image/png".to_string(),
            body: Vec::new(), // Placeholder
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bbox() {
        let req = WmsGetMapRequest {
            layers: "roads".to_string(),
            styles: "".to_string(),
            crs: "EPSG:4326".to_string(),
            bbox: "-180,-90,180,90".to_string(),
            width: 256,
            height: 256,
            format: "image/png".to_string(),
        };
        let bbox = req.parse_bbox().unwrap();
        assert_eq!(bbox, [-180.0, -90.0, 180.0, 90.0]);
    }

    #[test]
    fn test_layer_names() {
        let req = WmsGetMapRequest {
            layers: "roads,buildings,water".to_string(),
            styles: "".to_string(),
            crs: "EPSG:4326".to_string(),
            bbox: "0,0,1,1".to_string(),
            width: 256,
            height: 256,
            format: "image/png".to_string(),
        };
        assert_eq!(req.layer_names(), vec!["roads", "buildings", "water"]);
    }
}
