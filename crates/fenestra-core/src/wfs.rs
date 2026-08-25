use crate::Error;
use crate::ogcapi::Feature;
use crate::xml::{XSI_NAMESPACE, escape};
use serde::Deserialize;

pub const WFS_NAMESPACE: &str = "http://www.opengis.net/wfs/2.0";
pub const WFS_SCHEMA_LOCATION: &str = "http://schemas.opengis.net/wfs/2.0/wfs.xsd";
/// The only GetFeature output format this server produces.
pub const GEOJSON_OUTPUT_FORMAT: &str = "application/json";
/// Content type of the DescribeFeatureType schema.
pub const DESCRIBE_FEATURE_TYPE_FORMAT: &str = "application/gml+xml; version=3.2";

const GML_NAMESPACE: &str = "http://www.opengis.net/gml/3.2";
const GML_SCHEMA_LOCATION: &str = "http://schemas.opengis.net/gml/3.2.1/gml.xsd";
const XSD_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema";
const GML_ANY_GEOMETRY: &str = "gml:GeometryPropertyType";
/// Name of the geometry element in every generated feature type.
const GEOMETRY_ELEMENT: &str = "geometry";

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

/// The XML Schema view of one feature type, as DescribeFeatureType returns it.
#[derive(Debug, Clone)]
pub struct FeatureTypeSchema {
    name: String,
    geometry_property_type: &'static str,
    properties: Vec<(String, &'static str)>,
}

impl FeatureTypeSchema {
    /// Derive a schema from a collection's declared geometry type and one of
    /// its features. Property names and types come from that feature, so a
    /// collection with no features describes as geometry only.
    pub fn derive(name: &str, geometry_type: &str, sample: Option<&Feature>) -> Self {
        let declared = gml_property_type(geometry_type);
        let geometry_property_type = if declared != GML_ANY_GEOMETRY {
            declared
        } else {
            sample
                .and_then(|feature| feature.geometry.as_ref())
                .map(|geometry| gml_property_type(geometry.type_name()))
                .unwrap_or(GML_ANY_GEOMETRY)
        };
        let properties = sample
            .and_then(|feature| feature.properties.as_object())
            .map(|object| {
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), xsd_type(value)))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            name: name.to_string(),
            geometry_property_type,
            properties,
        }
    }
}

/// GML property type for a geometry type name, in either the GeoJSON spelling
/// (`MultiPolygon`) or the source's own (`multi_polygon`).
fn gml_property_type(geometry_type: &str) -> &'static str {
    let normalized = geometry_type.to_ascii_lowercase().replace('_', "");
    match normalized.as_str() {
        "point" => "gml:PointPropertyType",
        "multipoint" => "gml:MultiPointPropertyType",
        "linestring" | "curve" => "gml:CurvePropertyType",
        "multilinestring" | "multicurve" => "gml:MultiCurvePropertyType",
        "polygon" | "surface" => "gml:SurfacePropertyType",
        "multipolygon" | "multisurface" => "gml:MultiSurfacePropertyType",
        _ => GML_ANY_GEOMETRY,
    }
}

fn xsd_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Bool(_) => "xsd:boolean",
        serde_json::Value::Number(number) if number.is_f64() => "xsd:double",
        serde_json::Value::Number(_) => "xsd:integer",
        _ => "xsd:string",
    }
}

/// Generate a WFS 2.0 DescribeFeatureType XML Schema document.
pub fn describe_feature_type_xml(types: &[FeatureTypeSchema]) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(&format!(
        "<xsd:schema xmlns:xsd=\"{XSD_NAMESPACE}\" xmlns:gml=\"{GML_NAMESPACE}\" elementFormDefault=\"qualified\">\n"
    ));
    xml.push_str(&format!(
        "  <xsd:import namespace=\"{GML_NAMESPACE}\" schemaLocation=\"{GML_SCHEMA_LOCATION}\"/>\n"
    ));
    for feature_type in types {
        let name = escape(&feature_type.name);
        xml.push_str(&format!("  <xsd:complexType name=\"{name}Type\">\n"));
        xml.push_str("    <xsd:complexContent>\n");
        xml.push_str("      <xsd:extension base=\"gml:AbstractFeatureType\">\n");
        xml.push_str("        <xsd:sequence>\n");
        xml.push_str(&format!(
            "          <xsd:element name=\"{GEOMETRY_ELEMENT}\" type=\"{}\" minOccurs=\"0\" maxOccurs=\"1\" nillable=\"true\"/>\n",
            feature_type.geometry_property_type
        ));
        for (property, xsd) in &feature_type.properties {
            xml.push_str(&format!(
                "          <xsd:element name=\"{}\" type=\"{xsd}\" minOccurs=\"0\" maxOccurs=\"1\" nillable=\"true\"/>\n",
                escape(property)
            ));
        }
        xml.push_str("        </xsd:sequence>\n");
        xml.push_str("      </xsd:extension>\n");
        xml.push_str("    </xsd:complexContent>\n");
        xml.push_str("  </xsd:complexType>\n");
        xml.push_str(&format!(
            "  <xsd:element name=\"{name}\" type=\"{name}Type\" substitutionGroup=\"gml:AbstractFeature\"/>\n"
        ));
    }
    xml.push_str("</xsd:schema>\n");
    xml
}

/// Generate the `RESULTTYPE=hits` answer: the match count and no features.
pub fn wfs_hits_xml(number_matched: usize, timestamp: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            "\n",
            r#"<wfs:FeatureCollection xmlns:wfs="{}" xmlns:xsi="{}""#,
            r#" xsi:schemaLocation="{} {}""#,
            r#" timeStamp="{}" numberMatched="{}" numberReturned="0"/>"#,
            "\n",
        ),
        WFS_NAMESPACE,
        XSI_NAMESPACE,
        WFS_NAMESPACE,
        WFS_SCHEMA_LOCATION,
        escape(timestamp),
        number_matched
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ogcapi::Geometry;

    #[test]
    fn schema_takes_the_geometry_type_from_the_collection() {
        let schema = FeatureTypeSchema::derive("parcels", "polygon", None);
        assert_eq!(schema.geometry_property_type, "gml:SurfacePropertyType");
        assert!(schema.properties.is_empty());
    }

    #[test]
    fn schema_falls_back_to_the_sample_geometry() {
        let feature = Feature::new(
            None,
            Geometry::MultiPoint {
                coordinates: vec![[0.0, 0.0]],
            },
            serde_json::json!({}),
        );
        let schema = FeatureTypeSchema::derive("stops", "", Some(&feature));
        assert_eq!(schema.geometry_property_type, "gml:MultiPointPropertyType");
    }

    #[test]
    fn schema_types_properties_from_the_sample_feature() {
        let feature = Feature::new(
            None,
            Geometry::Point {
                coordinates: [0.0, 0.0],
            },
            serde_json::json!({"name": "a", "count": 3, "area": 1.5, "active": true}),
        );
        let schema = FeatureTypeSchema::derive("places", "point", Some(&feature));
        let types: std::collections::HashMap<&str, &str> = schema
            .properties
            .iter()
            .map(|(name, xsd)| (name.as_str(), *xsd))
            .collect();
        assert_eq!(types["name"], "xsd:string");
        assert_eq!(types["count"], "xsd:integer");
        assert_eq!(types["area"], "xsd:double");
        assert_eq!(types["active"], "xsd:boolean");
    }

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
