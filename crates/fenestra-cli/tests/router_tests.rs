//! Integration tests driving the axum router with an in-memory feature source.

use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use fenestra_cli::source::{Collection, FeatureSource, SourceError};
use fenestra_cli::{AppState, build_router};
use fenestra_core::{Feature, Geometry};
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;

struct MemSource {
    collections: Vec<Collection>,
    features: HashMap<String, Vec<Feature>>,
}

impl MemSource {
    fn monaco() -> Self {
        let features = vec![
            Feature::new(
                Some("1".into()),
                Geometry::Point {
                    coordinates: [7.4278, 43.7392],
                },
                serde_json::json!({"name": "Casino de Monte-Carlo"}),
            ),
            Feature::new(
                Some("2".into()),
                Geometry::Point {
                    coordinates: [7.4206, 43.7314],
                },
                serde_json::json!({"name": "Palais Princier"}),
            ),
            Feature::new(
                Some("3".into()),
                Geometry::Point {
                    coordinates: [7.4262, 43.7351],
                },
                serde_json::json!({"name": "Port Hercule"}),
            ),
        ];
        let mut map = HashMap::new();
        map.insert("monaco_pois".to_string(), features);
        Self {
            collections: vec![Collection {
                id: "monaco_pois".into(),
                title: "monaco_pois".into(),
                geometry_type: "point".into(),
            }],
            features: map,
        }
    }
}

#[async_trait]
impl FeatureSource for MemSource {
    async fn collections(&self) -> Result<Vec<Collection>, SourceError> {
        Ok(self.collections.clone())
    }

    async fn features(
        &self,
        layer: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Feature>, SourceError> {
        let mut features = self
            .features
            .get(layer)
            .cloned()
            .ok_or_else(|| SourceError::NotFound(layer.to_string()))?;
        if let Some(limit) = limit {
            features.truncate(limit);
        }
        Ok(features)
    }
}

fn app() -> axum::Router {
    let state = AppState {
        source: Arc::new(MemSource::monaco()),
        coverages: Arc::new(fenestra_cli::coverage::CoverageCatalog::new(
            "nonexistent-coverage-dir",
        )),
        base_url: "http://localhost:8080".into(),
    };
    build_router(state)
}

async fn get(uri: &str) -> (StatusCode, Vec<u8>) {
    let response = app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, bytes.to_vec())
}

fn decode_png(bytes: &[u8]) -> Vec<u8> {
    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder.read_info().expect("valid png");
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("png frame");
    buf.truncate(info.buffer_size());
    buf
}

#[tokio::test]
async fn wms_getmap_returns_png_with_visible_points() {
    let (status, body) = get("/wms?request=GetMap&layers=monaco_pois&crs=EPSG:4326\
         &bbox=7.40,43.72,7.44,43.75&width=256&height=256&format=image/png")
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[0..4], &[137, 80, 78, 71], "PNG magic bytes");

    let pixels = decode_png(&body);
    // white background plus drawn points means pixels are not uniform
    assert!(
        pixels.iter().any(|&b| b != pixels[0]),
        "expected non-uniform pixels when features are present"
    );
}

#[tokio::test]
async fn wms_getmap_empty_bbox_is_blank() {
    let (status, body) = get("/wms?request=GetMap&layers=monaco_pois&crs=EPSG:4326\
         &bbox=0,0,1,1&width=64&height=64&format=image/png")
    .await;
    assert_eq!(status, StatusCode::OK);
    let pixels = decode_png(&body);
    // no features in this bbox: every pixel is the white background
    assert!(pixels.iter().all(|&b| b == pixels[0]));
}

#[tokio::test]
async fn wfs_getfeature_returns_seeded_features() {
    let (status, body) = get("/wfs?request=GetFeature&typenames=monaco_pois").await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "FeatureCollection");
    assert_eq!(json["features"].as_array().unwrap().len(), 3);
    assert_eq!(
        json["features"][0]["properties"]["name"],
        "Casino de Monte-Carlo"
    );
}

#[tokio::test]
async fn wfs_getfeature_respects_bbox_and_count() {
    let (_, body) =
        get("/wfs?request=GetFeature&typenames=monaco_pois&bbox=7.425,43.73,7.43,43.74").await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let feats = json["features"].as_array().unwrap();
    assert!(
        feats.len() < 3 && !feats.is_empty(),
        "bbox should filter some out"
    );

    let (_, body) = get("/wfs?request=GetFeature&typenames=monaco_pois&count=1").await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["features"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn wmts_tile_returns_png() {
    let (status, body) =
        get("/wmts?request=GetTile&layer=monaco_pois&tilematrix=0&tilerow=0&tilecol=0").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[0..4], &[137, 80, 78, 71]);
    // zoom 0 covers the whole world, so the points render
    let pixels = decode_png(&body);
    assert!(pixels.iter().any(|&b| b != pixels[0]));
}

#[tokio::test]
async fn wmts_rest_tile_returns_png() {
    let (status, body) = get("/wmts/monaco_pois/WebMercatorQuad/0/0/0.png").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[0..4], &[137, 80, 78, 71]);
}

#[tokio::test]
async fn ogc_collections_lists_datasets() {
    let (status, body) = get("/ogc/collections").await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let cols = json["collections"].as_array().unwrap();
    assert_eq!(cols.len(), 1);
    assert_eq!(cols[0]["id"], "monaco_pois");
}

#[tokio::test]
async fn ogc_items_returns_features() {
    let (status, body) = get("/ogc/collections/monaco_pois/items?limit=2").await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "FeatureCollection");
    assert_eq!(json["features"].as_array().unwrap().len(), 2);
    assert_eq!(json["numberMatched"], 3);
}

#[tokio::test]
async fn wms_getcapabilities_lists_layers() {
    let (status, body) = get("/wms?request=GetCapabilities").await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("<Name>monaco_pois</Name>"));
}

async fn post_sld(uri: &str, sld: &str) -> (StatusCode, serde_json::Value) {
    let response = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/xml")
                .body(Body::from(sld.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

const CATEGORIZED_SLD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<StyledLayerDescriptor version="1.0.0" xmlns:ogc="http://www.opengis.net/ogc">
  <NamedLayer>
    <Name>landuse</Name>
    <UserStyle>
      <Name>by-type</Name>
      <Rule>
        <ogc:Filter>
          <ogc:PropertyIsEqualTo>
            <ogc:PropertyName>type</ogc:PropertyName>
            <ogc:Literal>forest</ogc:Literal>
          </ogc:PropertyIsEqualTo>
        </ogc:Filter>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#1B7837</CssParameter></Fill>
        </PolygonSymbolizer>
      </Rule>
      <Rule>
        <ogc:Filter>
          <ogc:PropertyIsEqualTo>
            <ogc:PropertyName>type</ogc:PropertyName>
            <ogc:Literal>water</ogc:Literal>
          </ogc:PropertyIsEqualTo>
        </ogc:Filter>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#2166AC</CssParameter></Fill>
          <Stroke><CssParameter name="stroke-width">2</CssParameter></Stroke>
        </PolygonSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

#[tokio::test]
async fn sld_symbology_converts_a_categorized_style() {
    let (status, json) = post_sld("/sld/symbology", CATEGORIZED_SLD).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["layer"], "landuse");
    assert_eq!(json["style"], "by-type");
    assert_eq!(json["symbology"]["kind"], "categorized");
    assert_eq!(json["symbology"]["field"], "type");
    assert_eq!(
        json["symbology"]["categories"],
        serde_json::json!([
            {"value": "forest", "color": "#1B7837"},
            {"value": "water", "color": "#2166AC"},
        ])
    );

    let lost = json["unsupported"].as_array().unwrap();
    assert_eq!(
        lost.len(),
        1,
        "only the stroke width is left behind: {lost:?}"
    );
    assert_eq!(lost[0]["construct"], "Stroke");
}

#[tokio::test]
async fn sld_symbology_reports_what_it_cannot_carry() {
    let sld = r#"<StyledLayerDescriptor version="1.0.0" xmlns:ogc="http://www.opengis.net/ogc">
  <NamedLayer>
    <Name>places</Name>
    <UserStyle>
      <Name>labelled</Name>
      <Rule>
        <Name>capitals</Name>
        <MaxScaleDenominator>250000</MaxScaleDenominator>
        <ogc:Filter>
          <ogc:Or>
            <ogc:PropertyIsEqualTo>
              <ogc:PropertyName>kind</ogc:PropertyName>
              <ogc:Literal>capital</ogc:Literal>
            </ogc:PropertyIsEqualTo>
            <ogc:PropertyIsEqualTo>
              <ogc:PropertyName>kind</ogc:PropertyName>
              <ogc:Literal>city</ogc:Literal>
            </ogc:PropertyIsEqualTo>
          </ogc:Or>
        </ogc:Filter>
        <PointSymbolizer>
          <Graphic>
            <Mark>
              <WellKnownName>star</WellKnownName>
              <Fill><CssParameter name="fill">#333333</CssParameter></Fill>
            </Mark>
            <Size>14</Size>
          </Graphic>
        </PointSymbolizer>
        <TextSymbolizer>
          <Label><ogc:PropertyName>name</ogc:PropertyName></Label>
        </TextSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

    let (status, json) = post_sld("/sld/symbology", sld).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        json["symbology"].is_null(),
        "an Or filter leaves no rule to apply: {json}"
    );

    let lost = json["unsupported"].as_array().unwrap();
    let constructs: Vec<&str> = lost
        .iter()
        .map(|entry| entry["construct"].as_str().unwrap())
        .collect();
    assert!(constructs.contains(&"Or"), "{constructs:?}");
    assert!(constructs.contains(&"TextSymbolizer"), "{constructs:?}");
    assert!(constructs.contains(&"Graphic"), "{constructs:?}");
    assert!(constructs.contains(&"ScaleDenominator"), "{constructs:?}");

    let or = lost.iter().find(|e| e["construct"] == "Or").unwrap();
    assert_eq!(or["rule_name"], "capitals");
    assert_eq!(or["rule_index"], 0);
}

#[tokio::test]
async fn sld_symbology_rejects_a_document_with_no_style() {
    let (status, _) = post_sld("/sld/symbology", "not an sld").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn sld_symbology_selects_a_layer_and_style_by_name() {
    let (status, json) = post_sld(
        "/sld/symbology?layer=landuse&style=by-type",
        CATEGORIZED_SLD,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["symbology"]["kind"], "categorized");

    let (status, _) = post_sld("/sld/symbology?layer=missing", CATEGORIZED_SLD).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
