//! OGC Web Coverage Service (WCS) — request/response types for coverage data access.

use crate::Error;
use serde::{Deserialize, Serialize};

/// WCS GetCoverage request parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct WcsGetCoverageRequest {
    pub coverage_id: String,
    pub format: String,
    pub subset_x: Option<SubsetSpec>,
    pub subset_y: Option<SubsetSpec>,
    pub subset_time: Option<String>,
    pub scale_factor: Option<f64>,
    pub range_subset: Option<Vec<String>>,
    pub interpolation: Option<Interpolation>,
}

/// Axis subset specification (trim or slice).
#[derive(Debug, Clone, Deserialize)]
pub enum SubsetSpec {
    Trim { low: f64, high: f64 },
    Slice(f64),
}

/// Interpolation methods for resampling coverages.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum Interpolation {
    #[default]
    NearestNeighbor,
    Bilinear,
    Bicubic,
    Average,
}

/// WCS DescribeCoverage request.
#[derive(Debug, Clone, Deserialize)]
pub struct WcsDescribeCoverageRequest {
    pub coverage_ids: Vec<String>,
}

/// Coverage metadata returned by DescribeCoverage.
#[derive(Debug, Clone, Serialize)]
pub struct CoverageDescription {
    pub coverage_id: String,
    pub crs: String,
    pub bbox: [f64; 4],
    pub grid_size: [u32; 2],
    pub range_type: Vec<RangeField>,
    pub native_format: String,
}

/// A single field/band in a coverage's range type.
#[derive(Debug, Clone, Serialize)]
pub struct RangeField {
    pub name: String,
    pub data_type: String,
    pub uom: Option<String>,
    pub nil_values: Vec<f64>,
}

/// Parsed GetCoverage response metadata.
#[derive(Debug, Clone)]
pub struct CoverageResponse {
    pub content_type: String,
    pub data: Vec<u8>,
}

impl WcsGetCoverageRequest {
    /// Compute effective bounding box from subset parameters.
    pub fn effective_bbox(&self, full_bbox: &[f64; 4]) -> [f64; 4] {
        let min_x = match &self.subset_x {
            Some(SubsetSpec::Trim { low, .. }) => *low,
            Some(SubsetSpec::Slice(v)) => *v,
            None => full_bbox[0],
        };
        let max_x = match &self.subset_x {
            Some(SubsetSpec::Trim { high, .. }) => *high,
            Some(SubsetSpec::Slice(v)) => *v,
            None => full_bbox[2],
        };
        let min_y = match &self.subset_y {
            Some(SubsetSpec::Trim { low, .. }) => *low,
            Some(SubsetSpec::Slice(v)) => *v,
            None => full_bbox[1],
        };
        let max_y = match &self.subset_y {
            Some(SubsetSpec::Trim { high, .. }) => *high,
            Some(SubsetSpec::Slice(v)) => *v,
            None => full_bbox[3],
        };
        [min_x, min_y, max_x, max_y]
    }

    /// Validate the request parameters.
    pub fn validate(&self) -> Result<(), Error> {
        if self.coverage_id.is_empty() {
            return Err(Error::InvalidRequest("coverage_id is required".to_string()));
        }
        if let Some(sf) = self.scale_factor {
            if sf <= 0.0 || !sf.is_finite() {
                return Err(Error::InvalidRequest(
                    "scale_factor must be positive and finite".to_string(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_bbox_no_subset() {
        let req = WcsGetCoverageRequest {
            coverage_id: "dem".to_string(),
            format: "image/tiff".to_string(),
            subset_x: None,
            subset_y: None,
            subset_time: None,
            scale_factor: None,
            range_subset: None,
            interpolation: None,
        };
        let full = [-180.0, -90.0, 180.0, 90.0];
        assert_eq!(req.effective_bbox(&full), full);
    }

    #[test]
    fn test_effective_bbox_trim() {
        let req = WcsGetCoverageRequest {
            coverage_id: "dem".to_string(),
            format: "image/tiff".to_string(),
            subset_x: Some(SubsetSpec::Trim {
                low: 10.0,
                high: 20.0,
            }),
            subset_y: Some(SubsetSpec::Trim {
                low: 40.0,
                high: 50.0,
            }),
            subset_time: None,
            scale_factor: None,
            range_subset: None,
            interpolation: None,
        };
        let full = [-180.0, -90.0, 180.0, 90.0];
        assert_eq!(req.effective_bbox(&full), [10.0, 40.0, 20.0, 50.0]);
    }

    #[test]
    fn test_validate_empty_coverage_id() {
        let req = WcsGetCoverageRequest {
            coverage_id: "".to_string(),
            format: "image/tiff".to_string(),
            subset_x: None,
            subset_y: None,
            subset_time: None,
            scale_factor: None,
            range_subset: None,
            interpolation: None,
        };
        assert!(req.validate().is_err());
    }
}
