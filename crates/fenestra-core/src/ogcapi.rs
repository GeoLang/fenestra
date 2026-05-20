use serde::{Deserialize, Serialize};

/// GeoJSON geometry types supported by OGC API Features.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum Geometry {
    Point {
        coordinates: [f64; 2],
    },
    LineString {
        coordinates: Vec<[f64; 2]>,
    },
    Polygon {
        coordinates: Vec<Vec<[f64; 2]>>,
    },
    MultiPoint {
        coordinates: Vec<[f64; 2]>,
    },
    MultiLineString {
        coordinates: Vec<Vec<[f64; 2]>>,
    },
    MultiPolygon {
        coordinates: Vec<Vec<Vec<[f64; 2]>>>,
    },
}

/// A GeoJSON Feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    #[serde(rename = "type")]
    pub feature_type: String,
    pub id: Option<String>,
    pub geometry: Option<Geometry>,
    pub properties: serde_json::Value,
}

impl Feature {
    pub fn new(id: Option<String>, geometry: Geometry, properties: serde_json::Value) -> Self {
        Self {
            feature_type: "Feature".to_string(),
            id,
            geometry: Some(geometry),
            properties,
        }
    }
}

/// A GeoJSON FeatureCollection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureCollection {
    #[serde(rename = "type")]
    pub collection_type: String,
    pub features: Vec<Feature>,
    #[serde(rename = "numberMatched", skip_serializing_if = "Option::is_none")]
    pub number_matched: Option<usize>,
    #[serde(rename = "numberReturned", skip_serializing_if = "Option::is_none")]
    pub number_returned: Option<usize>,
}

impl FeatureCollection {
    pub fn new(features: Vec<Feature>) -> Self {
        let count = features.len();
        Self {
            collection_type: "FeatureCollection".to_string(),
            features,
            number_matched: Some(count),
            number_returned: Some(count),
        }
    }
}

/// OGC API Features collection metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(rename = "itemType")]
    pub item_type: String,
    pub crs: Vec<String>,
    pub links: Vec<Link>,
}

/// HATEOAS link for OGC API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub href: String,
    pub rel: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// OGC API Features landing page response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandingPage {
    pub title: String,
    pub description: String,
    pub links: Vec<Link>,
}

impl LandingPage {
    pub fn new(title: &str, description: &str, base_url: &str) -> Self {
        Self {
            title: title.to_string(),
            description: description.to_string(),
            links: vec![
                Link {
                    href: format!("{base_url}/"),
                    rel: "self".to_string(),
                    media_type: Some("application/json".to_string()),
                    title: Some("This document".to_string()),
                },
                Link {
                    href: format!("{base_url}/conformance"),
                    rel: "conformance".to_string(),
                    media_type: Some("application/json".to_string()),
                    title: Some("Conformance classes".to_string()),
                },
                Link {
                    href: format!("{base_url}/collections"),
                    rel: "data".to_string(),
                    media_type: Some("application/json".to_string()),
                    title: Some("Feature collections".to_string()),
                },
                Link {
                    href: format!("{base_url}/api"),
                    rel: "service-desc".to_string(),
                    media_type: Some("application/vnd.oai.openapi+json;version=3.0".to_string()),
                    title: Some("OpenAPI definition".to_string()),
                },
            ],
        }
    }
}

/// OGC API conformance declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceDeclaration {
    #[serde(rename = "conformsTo")]
    pub conforms_to: Vec<String>,
}

impl ConformanceDeclaration {
    pub fn ogc_api_features_core() -> Self {
        Self {
            conforms_to: vec![
                "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/core".to_string(),
                "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/geojson".to_string(),
                "http://www.opengis.net/spec/ogcapi-features-1/1.0/conf/oas30".to_string(),
            ],
        }
    }
}

/// Bounding box query parameter for spatial filtering.
#[derive(Debug, Clone)]
pub struct BboxFilter {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl BboxFilter {
    /// Parse a bbox query parameter string "minx,miny,maxx,maxy".
    pub fn parse(bbox_str: &str) -> Option<Self> {
        let parts: Vec<f64> = bbox_str
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if parts.len() == 4 {
            Some(Self {
                min_x: parts[0],
                min_y: parts[1],
                max_x: parts[2],
                max_y: parts[3],
            })
        } else {
            None
        }
    }

    /// Check if a point is within this bounding box.
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    /// Filter features by bounding box.
    pub fn filter_features(&self, features: &[Feature]) -> Vec<Feature> {
        features
            .iter()
            .filter(|f| self.feature_intersects(f))
            .cloned()
            .collect()
    }

    fn feature_intersects(&self, feature: &Feature) -> bool {
        match &feature.geometry {
            Some(Geometry::Point { coordinates }) => {
                self.contains_point(coordinates[0], coordinates[1])
            }
            Some(Geometry::LineString { coordinates }) => {
                coordinates.iter().any(|c| self.contains_point(c[0], c[1]))
            }
            Some(Geometry::Polygon { coordinates }) => coordinates
                .iter()
                .any(|ring| ring.iter().any(|c| self.contains_point(c[0], c[1]))),
            Some(Geometry::MultiPoint { coordinates }) => {
                coordinates.iter().any(|c| self.contains_point(c[0], c[1]))
            }
            Some(Geometry::MultiLineString { coordinates }) => coordinates
                .iter()
                .any(|ls| ls.iter().any(|c| self.contains_point(c[0], c[1]))),
            Some(Geometry::MultiPolygon { coordinates }) => coordinates.iter().any(|poly| {
                poly.iter()
                    .any(|ring| ring.iter().any(|c| self.contains_point(c[0], c[1])))
            }),
            None => false,
        }
    }
}

/// Paginate a feature list.
pub fn paginate_features(features: &[Feature], offset: usize, limit: usize) -> FeatureCollection {
    let total = features.len();
    let page: Vec<Feature> = features.iter().skip(offset).take(limit).cloned().collect();
    let returned = page.len();
    FeatureCollection {
        collection_type: "FeatureCollection".to_string(),
        features: page,
        number_matched: Some(total),
        number_returned: Some(returned),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_feature_serialization() {
        let feature = Feature::new(
            Some("1".to_string()),
            Geometry::Point {
                coordinates: [1.0, 2.0],
            },
            json!({"name": "test"}),
        );
        let json = serde_json::to_string(&feature).unwrap();
        assert!(json.contains("\"type\":\"Feature\""));
        assert!(json.contains("\"type\":\"Point\""));
        assert!(json.contains("[1.0,2.0]"));
    }

    #[test]
    fn test_feature_collection() {
        let features = vec![
            Feature::new(
                Some("1".to_string()),
                Geometry::Point {
                    coordinates: [0.0, 0.0],
                },
                json!({}),
            ),
            Feature::new(
                Some("2".to_string()),
                Geometry::Point {
                    coordinates: [1.0, 1.0],
                },
                json!({}),
            ),
        ];
        let fc = FeatureCollection::new(features);
        assert_eq!(fc.number_matched, Some(2));
        assert_eq!(fc.features.len(), 2);
    }

    #[test]
    fn test_bbox_filter() {
        let bbox = BboxFilter::parse("0,0,10,10").unwrap();
        assert!(bbox.contains_point(5.0, 5.0));
        assert!(!bbox.contains_point(11.0, 5.0));

        let features = vec![
            Feature::new(
                Some("in".to_string()),
                Geometry::Point {
                    coordinates: [5.0, 5.0],
                },
                json!({}),
            ),
            Feature::new(
                Some("out".to_string()),
                Geometry::Point {
                    coordinates: [15.0, 15.0],
                },
                json!({}),
            ),
        ];
        let filtered = bbox.filter_features(&features);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, Some("in".to_string()));
    }

    #[test]
    fn test_landing_page() {
        let page = LandingPage::new("Test API", "A test", "http://localhost:8080");
        assert_eq!(page.links.len(), 4);
        assert!(page.links[0].href.ends_with('/'));
    }

    #[test]
    fn test_conformance() {
        let conf = ConformanceDeclaration::ogc_api_features_core();
        assert_eq!(conf.conforms_to.len(), 3);
        assert!(conf.conforms_to[0].contains("core"));
    }

    #[test]
    fn test_pagination() {
        let features: Vec<Feature> = (0..10)
            .map(|i| {
                Feature::new(
                    Some(i.to_string()),
                    Geometry::Point {
                        coordinates: [i as f64, 0.0],
                    },
                    json!({}),
                )
            })
            .collect();

        let page1 = paginate_features(&features, 0, 3);
        assert_eq!(page1.number_matched, Some(10));
        assert_eq!(page1.number_returned, Some(3));
        assert_eq!(page1.features[0].id, Some("0".to_string()));

        let page2 = paginate_features(&features, 3, 3);
        assert_eq!(page2.features[0].id, Some("3".to_string()));
    }

    #[test]
    fn test_geometry_polygon_deserialization() {
        let json_str = r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#;
        let geom: Geometry = serde_json::from_str(json_str).unwrap();
        match geom {
            Geometry::Polygon { coordinates } => {
                assert_eq!(coordinates.len(), 1);
                assert_eq!(coordinates[0].len(), 5);
            }
            _ => panic!("expected Polygon"),
        }
    }
}
