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

/// Spatial axis addressed by a KVP subset parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubsetAxis {
    X,
    Y,
}

/// Parse a WCS 2.0 KVP subset value: `x(10,20)`, `Lat(49.5)`, `x(*,20)`.
///
/// `*` means an open trim bound and maps to an infinite value; callers clamp
/// to the coverage extent. `x`/`y` are always accepted as easting/northing
/// grid axes, even for EPSG:4326 where the advertised labels are Lat/Long.
pub fn parse_subset(value: &str) -> Result<(SubsetAxis, SubsetSpec), Error> {
    let invalid = || Error::InvalidRequest(format!("invalid subset: {value}"));
    let open = value.find('(').ok_or_else(invalid)?;
    if !value.ends_with(')') {
        return Err(invalid());
    }
    let axis = match value[..open].trim().to_ascii_lowercase().as_str() {
        "x" | "long" | "lon" | "e" | "i" => SubsetAxis::X,
        "y" | "lat" | "n" | "j" => SubsetAxis::Y,
        other => return Err(Error::InvalidAxisLabel(other.to_string())),
    };
    let bounds: Vec<&str> = value[open + 1..value.len() - 1]
        .split(',')
        .map(str::trim)
        .collect();
    let trim_bound = |s: &str, open_value: f64| -> Result<f64, Error> {
        if s == "*" {
            Ok(open_value)
        } else {
            s.parse().map_err(|_| invalid())
        }
    };
    let spec = match bounds.as_slice() {
        [point] => SubsetSpec::Slice(point.parse().map_err(|_| invalid())?),
        [low, high] => SubsetSpec::Trim {
            low: trim_bound(low, f64::NEG_INFINITY)?,
            high: trim_bound(high, f64::INFINITY)?,
        },
        _ => return Err(invalid()),
    };
    Ok((axis, spec))
}

/// EPSG-style CRS string (`EPSG:4326`) to an OGC CRS URI for srsName.
fn crs_uri(crs: &str) -> String {
    match crs.strip_prefix("EPSG:") {
        Some(code) => format!("http://www.opengis.net/def/crs/EPSG/0/{code}"),
        None => crs.to_string(),
    }
}

/// Generate a WCS 2.0.1 GetCapabilities XML document.
pub fn wcs_capabilities_xml(title: &str, coverage_ids: &[&str]) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(concat!(
        r#"<wcs:Capabilities version="2.0.1" xmlns:wcs="http://www.opengis.net/wcs/2.0""#,
        r#" xmlns:ows="http://www.opengis.net/ows/2.0">"#,
    ));
    xml.push('\n');
    xml.push_str("  <ows:ServiceIdentification>\n");
    xml.push_str(&format!("    <ows:Title>{title}</ows:Title>\n"));
    xml.push_str("    <ows:ServiceType>OGC WCS</ows:ServiceType>\n");
    xml.push_str("    <ows:ServiceTypeVersion>2.0.1</ows:ServiceTypeVersion>\n");
    xml.push_str("  </ows:ServiceIdentification>\n");
    xml.push_str("  <wcs:ServiceMetadata>\n");
    xml.push_str("    <wcs:formatSupported>image/tiff</wcs:formatSupported>\n");
    xml.push_str("  </wcs:ServiceMetadata>\n");
    xml.push_str("  <wcs:Contents>\n");
    for id in coverage_ids {
        xml.push_str("    <wcs:CoverageSummary>\n");
        xml.push_str(&format!("      <wcs:CoverageId>{id}</wcs:CoverageId>\n"));
        xml.push_str("      <wcs:CoverageSubtype>RectifiedGridCoverage</wcs:CoverageSubtype>\n");
        xml.push_str("    </wcs:CoverageSummary>\n");
    }
    xml.push_str("  </wcs:Contents>\n");
    xml.push_str("</wcs:Capabilities>\n");
    xml
}

/// Generate a WCS 2.0.1 DescribeCoverage XML document.
pub fn describe_coverage_xml(descriptions: &[CoverageDescription]) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(concat!(
        r#"<wcs:CoverageDescriptions xmlns:wcs="http://www.opengis.net/wcs/2.0""#,
        r#" xmlns:gml="http://www.opengis.net/gml/3.2""#,
        r#" xmlns:swe="http://www.opengis.net/swe/2.0">"#,
    ));
    xml.push('\n');
    for desc in descriptions {
        let id = &desc.coverage_id;
        let [min_x, min_y, max_x, max_y] = desc.bbox;
        // EPSG:4326 mandates latitude-first axis order, so the envelope
        // corners swap; the bbox itself stays x/y ordered internally.
        let (axis_labels, lower, upper) = if desc.crs == "EPSG:4326" {
            ("Lat Long", (min_y, min_x), (max_y, max_x))
        } else {
            ("x y", (min_x, min_y), (max_x, max_y))
        };
        xml.push_str(&format!("  <wcs:CoverageDescription gml:id=\"{id}\">\n"));
        xml.push_str("    <gml:boundedBy>\n");
        xml.push_str(&format!(
            "      <gml:Envelope srsName=\"{}\" axisLabels=\"{axis_labels}\" srsDimension=\"2\">\n",
            crs_uri(&desc.crs)
        ));
        xml.push_str(&format!(
            "        <gml:lowerCorner>{} {}</gml:lowerCorner>\n",
            lower.0, lower.1
        ));
        xml.push_str(&format!(
            "        <gml:upperCorner>{} {}</gml:upperCorner>\n",
            upper.0, upper.1
        ));
        xml.push_str("      </gml:Envelope>\n");
        xml.push_str("    </gml:boundedBy>\n");
        xml.push_str(&format!("    <wcs:CoverageId>{id}</wcs:CoverageId>\n"));
        xml.push_str("    <gml:domainSet>\n");
        xml.push_str(&format!(
            "      <gml:RectifiedGrid dimension=\"2\" gml:id=\"{id}-grid\">\n"
        ));
        xml.push_str("        <gml:limits>\n");
        xml.push_str("          <gml:GridEnvelope>\n");
        xml.push_str("            <gml:low>0 0</gml:low>\n");
        xml.push_str(&format!(
            "            <gml:high>{} {}</gml:high>\n",
            desc.grid_size[0].saturating_sub(1),
            desc.grid_size[1].saturating_sub(1)
        ));
        xml.push_str("          </gml:GridEnvelope>\n");
        xml.push_str("        </gml:limits>\n");
        xml.push_str("      </gml:RectifiedGrid>\n");
        xml.push_str("    </gml:domainSet>\n");
        xml.push_str("    <gml:rangeType>\n");
        xml.push_str("      <swe:DataRecord>\n");
        for field in &desc.range_type {
            xml.push_str(&format!("        <swe:field name=\"{}\">\n", field.name));
            xml.push_str(&format!(
                "          <swe:Quantity><swe:description>{}</swe:description></swe:Quantity>\n",
                field.data_type
            ));
            xml.push_str("        </swe:field>\n");
        }
        xml.push_str("      </swe:DataRecord>\n");
        xml.push_str("    </gml:rangeType>\n");
        xml.push_str("    <wcs:ServiceParameters>\n");
        xml.push_str("      <wcs:CoverageSubtype>RectifiedGridCoverage</wcs:CoverageSubtype>\n");
        xml.push_str(&format!(
            "      <wcs:nativeFormat>{}</wcs:nativeFormat>\n",
            desc.native_format
        ));
        xml.push_str("    </wcs:ServiceParameters>\n");
        xml.push_str("  </wcs:CoverageDescription>\n");
    }
    xml.push_str("</wcs:CoverageDescriptions>\n");
    xml
}

/// Generate an OWS 2.0 ExceptionReport XML document.
pub fn ows_exception_xml(code: &str, locator: &str, message: &str) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(concat!(
        r#"<ows:ExceptionReport version="2.0.0""#,
        r#" xmlns:ows="http://www.opengis.net/ows/2.0">"#,
    ));
    xml.push('\n');
    xml.push_str(&format!(
        "  <ows:Exception exceptionCode=\"{code}\" locator=\"{locator}\">\n"
    ));
    xml.push_str(&format!(
        "    <ows:ExceptionText>{message}</ows:ExceptionText>\n"
    ));
    xml.push_str("  </ows:Exception>\n");
    xml.push_str("</ows:ExceptionReport>\n");
    xml
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
    fn test_parse_subset_trim_and_slice() {
        let (axis, spec) = parse_subset("x(10,20)").unwrap();
        assert_eq!(axis, SubsetAxis::X);
        assert!(matches!(spec, SubsetSpec::Trim { low, high } if low == 10.0 && high == 20.0));

        let (axis, spec) = parse_subset("Lat(49.5)").unwrap();
        assert_eq!(axis, SubsetAxis::Y);
        assert!(matches!(spec, SubsetSpec::Slice(v) if v == 49.5));

        let (_, spec) = parse_subset("x(*,20)").unwrap();
        assert!(
            matches!(spec, SubsetSpec::Trim { low, high } if low.is_infinite() && high == 20.0)
        );
    }

    #[test]
    fn test_parse_subset_errors() {
        assert!(matches!(
            parse_subset("time(1,2)"),
            Err(Error::InvalidAxisLabel(_))
        ));
        assert!(parse_subset("x(1,2,3)").is_err());
        assert!(parse_subset("x").is_err());
        assert!(parse_subset("x(abc)").is_err());
    }

    #[test]
    fn test_wcs_capabilities_lists_coverages() {
        let xml = wcs_capabilities_xml("Test", &["dem", "temp"]);
        assert!(xml.contains("<wcs:CoverageId>dem</wcs:CoverageId>"));
        assert!(xml.contains("<wcs:CoverageId>temp</wcs:CoverageId>"));
        assert!(xml.contains("version=\"2.0.1\""));
    }

    #[test]
    fn test_describe_coverage_envelope_axis_order() {
        let desc = |crs: &str| CoverageDescription {
            coverage_id: "dem".to_string(),
            crs: crs.to_string(),
            bbox: [10.0, 48.5, 12.0, 50.0],
            grid_size: [4, 3],
            range_type: Vec::new(),
            native_format: "image/tiff".to_string(),
        };

        // EPSG:4326 is latitude-first
        let xml = describe_coverage_xml(&[desc("EPSG:4326")]);
        assert!(xml.contains("axisLabels=\"Lat Long\""));
        assert!(xml.contains("<gml:lowerCorner>48.5 10</gml:lowerCorner>"));
        assert!(xml.contains("<gml:upperCorner>50 12</gml:upperCorner>"));

        // projected CRS stays x/y
        let xml = describe_coverage_xml(&[desc("EPSG:32632")]);
        assert!(xml.contains("axisLabels=\"x y\""));
        assert!(xml.contains("<gml:lowerCorner>10 48.5</gml:lowerCorner>"));
        assert!(xml.contains("<gml:upperCorner>12 50</gml:upperCorner>"));
    }

    #[test]
    fn test_ows_exception_xml() {
        let xml = ows_exception_xml("NoSuchCoverage", "coverageId", "coverage dem not found");
        assert!(xml.contains("exceptionCode=\"NoSuchCoverage\""));
        assert!(xml.contains("coverage dem not found"));
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
