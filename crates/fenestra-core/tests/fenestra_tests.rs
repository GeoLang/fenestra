// Comprehensive integration tests for fenestra-core.

use fenestra_core::*;

// ═══════════════════════════════════════════════════════════════════════════
// OGC API types
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_feature_creation() {
    let geom = Geometry::Point {
        coordinates: [-0.1278, 51.5074],
    };
    let props = serde_json::json!({"name": "London", "population": 9_000_000});
    let feature = Feature::new(Some("1".into()), geom, props);
    assert_eq!(feature.feature_type, "Feature");
    assert_eq!(feature.id, Some("1".into()));
}

#[test]
fn test_feature_collection() {
    let features = vec![
        Feature::new(
            Some("1".into()),
            Geometry::Point {
                coordinates: [0.0, 0.0],
            },
            serde_json::json!({}),
        ),
        Feature::new(
            Some("2".into()),
            Geometry::Point {
                coordinates: [1.0, 1.0],
            },
            serde_json::json!({}),
        ),
    ];
    let fc = FeatureCollection::new(features);
    assert_eq!(fc.collection_type, "FeatureCollection");
    assert_eq!(fc.number_matched, Some(2));
    assert_eq!(fc.number_returned, Some(2));
}

#[test]
fn test_feature_geojson_serialization() {
    let geom = Geometry::Polygon {
        coordinates: vec![vec![
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [0.0, 0.0],
        ]],
    };
    let feature = Feature::new(None, geom, serde_json::json!({"type": "building"}));
    let json = serde_json::to_string(&feature).unwrap();
    assert!(json.contains("Polygon"));
    assert!(json.contains("building"));
}

#[test]
fn test_geometry_variants_serialize() {
    let geometries = vec![
        Geometry::Point {
            coordinates: [1.0, 2.0],
        },
        Geometry::LineString {
            coordinates: vec![[0.0, 0.0], [1.0, 1.0]],
        },
        Geometry::MultiPoint {
            coordinates: vec![[0.0, 0.0], [1.0, 1.0]],
        },
    ];
    for geom in &geometries {
        let json = serde_json::to_string(geom).unwrap();
        let back: Geometry = serde_json::from_str(&json).unwrap();
        assert_eq!(&back, geom);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Landing page & conformance
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_landing_page() {
    let page = LandingPage::new("Test Server", "A test OGC service", "http://localhost:8080");
    assert_eq!(page.title, "Test Server");
    assert_eq!(page.links.len(), 4);
    assert!(page.links.iter().any(|l| l.rel == "self"));
    assert!(page.links.iter().any(|l| l.rel == "conformance"));
    assert!(page.links.iter().any(|l| l.rel == "data"));
}

#[test]
fn test_conformance_declaration() {
    let conf = ConformanceDeclaration::ogc_api_features_core();
    assert_eq!(conf.conforms_to.len(), 3);
    assert!(conf.conforms_to[0].contains("ogcapi-features-1"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Bbox filter
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_bbox_filter_parse_valid() {
    let bbox = BboxFilter::parse("-180,-90,180,90").unwrap();
    assert_eq!(bbox.min_x, -180.0);
    assert_eq!(bbox.min_y, -90.0);
    assert_eq!(bbox.max_x, 180.0);
    assert_eq!(bbox.max_y, 90.0);
}

#[test]
fn test_bbox_filter_parse_invalid() {
    assert!(BboxFilter::parse("1,2,3").is_none());
    assert!(BboxFilter::parse("not,valid,numbers,here").is_none());
}

// ═══════════════════════════════════════════════════════════════════════════
// Configuration
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_service_config_default() {
    let config = ServiceConfig::default();
    assert_eq!(config.port, 8080);
    assert!(config.layers.is_empty());
}

#[test]
fn test_service_config_serialization() {
    let config = ServiceConfig {
        title: "My Server".into(),
        abstract_text: "Geo services".into(),
        host: "0.0.0.0".into(),
        port: 9090,
        layers: vec![LayerConfig {
            name: "roads".into(),
            title: "Road Network".into(),
            srs: vec!["EPSG:4326".into(), "EPSG:3857".into()],
            bbox: [-180.0, -90.0, 180.0, 90.0],
            source: "/data/roads.gpkg".into(),
        }],
    };
    let json = serde_json::to_string(&config).unwrap();
    let back: ServiceConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.layers.len(), 1);
    assert_eq!(back.layers[0].name, "roads");
}

// ═══════════════════════════════════════════════════════════════════════════
// Tiles
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_web_mercator_quad_tile_matrix_set() {
    let tms = fenestra_core::tiles::web_mercator_quad();
    assert!(tms.crs.contains("3857"));
    assert!(!tms.tile_matrices.is_empty());
    // Zoom level 0 should have 1x1 matrix
    assert_eq!(tms.tile_matrices[0].matrix_width, 1);
    assert_eq!(tms.tile_matrices[0].matrix_height, 1);
}

#[test]
fn test_tile_matrix_zoom_progression() {
    let tms = fenestra_core::tiles::web_mercator_quad();
    // Each zoom doubles the matrix dimensions
    if tms.tile_matrices.len() >= 3 {
        assert_eq!(tms.tile_matrices[1].matrix_width, 2);
        assert_eq!(tms.tile_matrices[2].matrix_width, 4);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SLD parsing
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_parse_sld_simple_polygon() {
    let sld_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<StyledLayerDescriptor version="1.0.0">
  <NamedLayer>
    <Name>buildings</Name>
    <UserStyle>
      <Name>default</Name>
      <FeatureTypeStyle>
        <Rule>
          <PolygonSymbolizer>
            <Fill>
              <CssParameter name="fill">#ff0000</CssParameter>
            </Fill>
            <Stroke>
              <CssParameter name="stroke">#000000</CssParameter>
              <CssParameter name="stroke-width">1</CssParameter>
            </Stroke>
          </PolygonSymbolizer>
        </Rule>
      </FeatureTypeStyle>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

    let sld = parse_sld(sld_xml).unwrap();
    assert_eq!(sld.named_layers.len(), 1);
    assert_eq!(sld.named_layers[0].name, "buildings");
    assert!(!sld.named_layers[0].styles.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// Pagination
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_paginate_features() {
    let features: Vec<Feature> = (0..20)
        .map(|i| {
            Feature::new(
                Some(format!("{i}")),
                Geometry::Point {
                    coordinates: [i as f64, 0.0],
                },
                serde_json::json!({}),
            )
        })
        .collect();

    let page = paginate_features(&features, 0, 5);
    assert_eq!(page.features.len(), 5);
    assert_eq!(page.features[0].id, Some("0".into()));

    let page2 = paginate_features(&features, 10, 5);
    assert_eq!(page2.features.len(), 5);
    assert_eq!(page2.features[0].id, Some("10".into()));
}

#[test]
fn test_paginate_features_beyond_end() {
    let features: Vec<Feature> = (0..3)
        .map(|i| {
            Feature::new(
                Some(format!("{i}")),
                Geometry::Point {
                    coordinates: [0.0, 0.0],
                },
                serde_json::json!({}),
            )
        })
        .collect();

    let page = paginate_features(&features, 5, 10);
    assert!(page.features.is_empty());
}
