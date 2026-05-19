use crate::Error;
use serde::Deserialize;

/// Parsed WFS GetFeature request parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct WfsGetFeatureRequest {
    pub type_names: String,
    pub count: Option<u32>,
    pub bbox: Option<String>,
    pub output_format: Option<String>,
}

impl WfsGetFeatureRequest {
    pub fn type_name_list(&self) -> Vec<&str> {
        self.type_names.split(',').collect()
    }

    pub fn parse_bbox(&self) -> Result<Option<[f64; 4]>, Error> {
        match &self.bbox {
            None => Ok(None),
            Some(bbox_str) => {
                let parts: Vec<&str> = bbox_str.split(',').collect();
                if parts.len() < 4 {
                    return Err(Error::InvalidRequest(
                        "bbox must have at least 4 values".to_string(),
                    ));
                }
                let vals: Result<Vec<f64>, _> =
                    parts[..4].iter().map(|s| s.parse::<f64>()).collect();
                let vals =
                    vals.map_err(|_| Error::InvalidRequest("invalid bbox values".to_string()))?;
                Ok(Some([vals[0], vals[1], vals[2], vals[3]]))
            }
        }
    }
}

/// WFS response (GeoJSON or GML).
#[derive(Debug)]
pub struct WfsResponse {
    pub content_type: String,
    pub body: String,
}

impl WfsResponse {
    /// Create an empty GeoJSON FeatureCollection response.
    pub fn empty_geojson() -> Self {
        Self {
            content_type: "application/geo+json".to_string(),
            body: r#"{"type":"FeatureCollection","features":[]}"#.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_name_list() {
        let req = WfsGetFeatureRequest {
            type_names: "roads,buildings".to_string(),
            count: Some(10),
            bbox: None,
            output_format: None,
        };
        assert_eq!(req.type_name_list(), vec!["roads", "buildings"]);
    }

    #[test]
    fn test_parse_bbox_none() {
        let req = WfsGetFeatureRequest {
            type_names: "roads".to_string(),
            count: None,
            bbox: None,
            output_format: None,
        };
        assert_eq!(req.parse_bbox().unwrap(), None);
    }
}
