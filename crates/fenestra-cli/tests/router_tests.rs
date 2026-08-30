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

/// Status, content type and body, for the routes whose media type matters.
async fn get_typed(uri: &str) -> (StatusCode, String, Vec<u8>) {
    let response = app()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, content_type, bytes.to_vec())
}

fn links_by_rel(json: &serde_json::Value) -> HashMap<String, String> {
    json["links"]
        .as_array()
        .expect("links")
        .iter()
        .map(|l| {
            (
                l["rel"].as_str().unwrap().to_string(),
                l["href"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[tokio::test]
async fn ogc_item_returns_one_feature_with_links() {
    let (status, content_type, body) = get_typed("/ogc/collections/monaco_pois/items/2").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/geo+json");
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "Feature");
    assert_eq!(json["id"], "2");
    assert_eq!(json["properties"]["name"], "Palais Princier");

    let links = links_by_rel(&json);
    assert_eq!(
        links["self"],
        "http://localhost:8080/ogc/collections/monaco_pois/items/2"
    );
    assert_eq!(
        links["collection"],
        "http://localhost:8080/ogc/collections/monaco_pois"
    );
}

#[tokio::test]
async fn ogc_item_404s_for_an_id_the_collection_does_not_have() {
    let (status, _) = get("/ogc/collections/monaco_pois/items/does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ogc_items_of_an_unknown_collection_are_not_found() {
    let (status, _) = get("/ogc/collections/nope/items").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = get("/ogc/collections/nope/items/1").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ogc_items_links_page_forward_and_back() {
    let bbox = "7.4,43.7,7.5,43.8";
    let (status, body) = get(&format!(
        "/ogc/collections/monaco_pois/items?limit=1&offset=1&bbox={bbox}"
    ))
    .await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["numberMatched"], 3);
    assert_eq!(json["numberReturned"], 1);

    let links = links_by_rel(&json);
    let items = "http://localhost:8080/ogc/collections/monaco_pois/items";
    assert_eq!(
        links["self"],
        format!("{items}?limit=1&offset=1&bbox={bbox}")
    );
    assert_eq!(
        links["next"],
        format!("{items}?limit=1&offset=2&bbox={bbox}")
    );
    assert_eq!(
        links["prev"],
        format!("{items}?limit=1&offset=0&bbox={bbox}")
    );
    assert_eq!(
        links["collection"],
        "http://localhost:8080/ogc/collections/monaco_pois"
    );

    // the first page has nothing before it and the last nothing after it
    let (_, body) = get(&format!("{}?limit=2", "/ogc/collections/monaco_pois/items")).await;
    let first = links_by_rel(&serde_json::from_slice(&body).unwrap());
    assert!(!first.contains_key("prev"));
    assert!(first.contains_key("next"));

    let (_, body) = get("/ogc/collections/monaco_pois/items?limit=2&offset=2").await;
    let last = links_by_rel(&serde_json::from_slice(&body).unwrap());
    assert!(!last.contains_key("next"));
    assert_eq!(last["prev"], format!("{items}?limit=2&offset=0"));
}

/// Every route `build_router` registers, as axum spells it.
const REGISTERED_ROUTES: [&str; 18] = [
    "/health",
    "/healthz",
    "/readyz",
    "/metrics",
    "/wms",
    "/wcs",
    "/wfs",
    "/wmts",
    "/wmts/{layer}/{tms}/{matrix}/{row}/{col}",
    "/ogc/",
    "/ogc/api",
    "/ogc/conformance",
    "/ogc/collections",
    "/ogc/collections/{id}",
    "/ogc/collections/{id}/items",
    "/ogc/collections/{id}/items/{featureId}",
    "/ogc/collections/{id}/tiles/WebMercatorQuad/{tileMatrix}/{tileRow}/{tileCol}",
    "/sld/symbology",
];

#[tokio::test]
async fn ogc_api_describes_every_registered_route() {
    let (status, content_type, body) = get_typed("/ogc/api").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/vnd.oai.openapi+json;version=3.0");
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["openapi"], "3.0.3");
    assert_eq!(json["servers"][0]["url"], "http://localhost:8080");

    let paths = json["paths"].as_object().expect("paths");
    for route in REGISTERED_ROUTES {
        assert!(paths.contains_key(route), "{route} is not described");
    }
    assert_eq!(paths.len(), REGISTERED_ROUTES.len(), "described: {paths:?}");
    assert!(paths["/ogc/collections/{id}/items"]["get"]["responses"]["200"].is_object());
    assert!(paths["/sld/symbology"]["post"]["requestBody"].is_object());
}

/// The document links the landing page calls `service-desc`.
#[tokio::test]
async fn ogc_landing_page_points_at_the_openapi_document() {
    let (_, body) = get("/ogc/").await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let links = links_by_rel(&json);
    let (status, _, _) =
        get_typed(links["service-desc"].trim_start_matches("http://localhost:8080")).await;
    assert_eq!(status, StatusCode::OK);
}

fn read_varint(bytes: &[u8], position: &mut usize) -> u64 {
    let mut value = 0u64;
    let mut shift = 0;
    loop {
        let byte = bytes[*position];
        *position += 1;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
        shift += 7;
    }
}

struct VectorTileLayer {
    name: String,
    version: u64,
    extent: u64,
    features: usize,
}

/// Walk the protobuf fields of an MVT tile the spec gives numbers to: layers
/// (3) holding name (1), features (2), extent (5) and version (15).
fn decode_vector_tile(bytes: &[u8]) -> Vec<VectorTileLayer> {
    let mut layers = Vec::new();
    let mut position = 0;
    while position < bytes.len() {
        let key = read_varint(bytes, &mut position);
        assert_eq!(key >> 3, 3, "only the layers field is expected");
        assert_eq!(key & 7, 2, "layers are length delimited");
        let length = read_varint(bytes, &mut position) as usize;
        layers.push(decode_vector_tile_layer(
            &bytes[position..position + length],
        ));
        position += length;
    }
    layers
}

fn decode_vector_tile_layer(bytes: &[u8]) -> VectorTileLayer {
    let mut layer = VectorTileLayer {
        name: String::new(),
        version: 0,
        extent: 0,
        features: 0,
    };
    let mut position = 0;
    while position < bytes.len() {
        let key = read_varint(bytes, &mut position);
        let field = key >> 3;
        match key & 7 {
            0 => {
                let value = read_varint(bytes, &mut position);
                match field {
                    5 => layer.extent = value,
                    15 => layer.version = value,
                    _ => {}
                }
            }
            2 => {
                let length = read_varint(bytes, &mut position) as usize;
                let body = &bytes[position..position + length];
                position += length;
                match field {
                    1 => layer.name = String::from_utf8(body.to_vec()).unwrap(),
                    2 => layer.features += 1,
                    _ => {}
                }
            }
            other => panic!("unexpected wire type {other} on field {field}"),
        }
    }
    layer
}

/// WebMercatorQuad tile holding all three seeded points, as tileMatrix, tileRow, tileCol.
const MONACO_TILE: (u32, u32, u32) = (12, 1493, 2132);

#[tokio::test]
async fn ogc_tile_returns_a_vector_tile_of_the_collection() {
    let (matrix, row, col) = MONACO_TILE;
    let (status, content_type, body) = get_typed(&format!(
        "/ogc/collections/monaco_pois/tiles/WebMercatorQuad/{matrix}/{row}/{col}"
    ))
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type, "application/vnd.mapbox-vector-tile");

    let layers = decode_vector_tile(&body);
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].name, "monaco_pois");
    assert_eq!(layers[0].version, 2);
    assert_eq!(layers[0].extent, 4096);
    assert_eq!(layers[0].features, 3);
}

#[tokio::test]
async fn ogc_tile_outside_the_seeded_area_carries_no_features() {
    let (status, _, body) =
        get_typed("/ogc/collections/monaco_pois/tiles/WebMercatorQuad/12/1493/2000").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode_vector_tile(&body)[0].features, 0);
}

#[tokio::test]
async fn ogc_tile_rejects_coordinates_outside_the_matrix() {
    let (status, _) = get("/ogc/collections/monaco_pois/tiles/WebMercatorQuad/1/2/0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = get("/ogc/collections/monaco_pois/tiles/WebMercatorQuad/25/0/0").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ogc_tile_404s_for_an_unknown_collection() {
    let (status, _) = get("/ogc/collections/nope/tiles/WebMercatorQuad/0/0/0").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn ogc_collection_links_to_itself_its_items_and_its_tiles() {
    let (status, body) = get("/ogc/collections/monaco_pois").await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let links = links_by_rel(&json);
    assert_eq!(
        links["self"],
        "http://localhost:8080/ogc/collections/monaco_pois"
    );
    assert_eq!(
        links["items"],
        "http://localhost:8080/ogc/collections/monaco_pois/items"
    );
    assert_eq!(
        links["tiles"],
        "http://localhost:8080/ogc/collections/monaco_pois/tiles/WebMercatorQuad/{tileMatrix}/{tileRow}/{tileCol}"
    );
}

const WMS_NAMESPACE: &str = "http://www.opengis.net/wms";
const WFS_NAMESPACE: &str = "http://www.opengis.net/wfs/2.0";
const OWS_1_1_NAMESPACE: &str = "http://www.opengis.net/ows/1.1";
const XSD_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema";

/// Extent of the three seeded Monaco points.
const MONACO_EXTENT: [f64; 4] = [7.4206, 43.7314, 7.4278, 43.7392];

fn descendants<'a>(
    node: roxmltree::Node<'a, 'a>,
    namespace: &str,
    name: &str,
) -> Vec<roxmltree::Node<'a, 'a>> {
    node.descendants()
        .filter(|n| n.tag_name().name() == name && n.tag_name().namespace() == Some(namespace))
        .collect()
}

fn descendant<'a>(
    node: roxmltree::Node<'a, 'a>,
    namespace: &str,
    name: &str,
) -> roxmltree::Node<'a, 'a> {
    descendants(node, namespace, name)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("{name} in {namespace}"))
}

fn text(node: roxmltree::Node<'_, '_>) -> String {
    node.text().unwrap_or_default().trim().to_string()
}

#[tokio::test]
async fn wms_getcapabilities_gives_each_layer_the_extent_of_its_features() {
    let (status, body) = get("/wms?request=GetCapabilities").await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body).unwrap();
    let document = roxmltree::Document::parse(&xml).expect("well formed capabilities");
    let root = document.root_element();
    assert_eq!(root.tag_name().namespace(), Some(WMS_NAMESPACE));

    let named = descendants(root, WMS_NAMESPACE, "Layer")
        .into_iter()
        .find(|layer| {
            descendants(*layer, WMS_NAMESPACE, "Name")
                .first()
                .is_some_and(|name| text(*name) == "monaco_pois")
        })
        .expect("monaco_pois layer");
    let geographic = descendant(named, WMS_NAMESPACE, "EX_GeographicBoundingBox");
    let bound = |name: &str| {
        text(descendant(geographic, WMS_NAMESPACE, name))
            .parse::<f64>()
            .unwrap()
    };
    assert_eq!(bound("westBoundLongitude"), MONACO_EXTENT[0]);
    assert_eq!(bound("southBoundLatitude"), MONACO_EXTENT[1]);
    assert_eq!(bound("eastBoundLongitude"), MONACO_EXTENT[2]);
    assert_eq!(bound("northBoundLatitude"), MONACO_EXTENT[3]);
}

#[tokio::test]
async fn wfs_getcapabilities_gives_each_feature_type_the_extent_of_its_features() {
    let (status, body) = get("/wfs?request=GetCapabilities").await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body).unwrap();
    let document = roxmltree::Document::parse(&xml).expect("well formed capabilities");
    let feature_type = descendant(document.root_element(), WFS_NAMESPACE, "FeatureType");
    assert_eq!(
        text(descendant(feature_type, WFS_NAMESPACE, "Name")),
        "monaco_pois"
    );
    let bounding_box = descendant(feature_type, OWS_1_1_NAMESPACE, "WGS84BoundingBox");
    assert_eq!(
        text(descendant(bounding_box, OWS_1_1_NAMESPACE, "LowerCorner")),
        "7.4206 43.7314"
    );
    assert_eq!(
        text(descendant(bounding_box, OWS_1_1_NAMESPACE, "UpperCorner")),
        "7.4278 43.7392"
    );
}

#[tokio::test]
async fn wfs_describefeaturetype_types_the_geometry_and_properties() {
    let (status, body) = get("/wfs?request=DescribeFeatureType&typenames=monaco_pois").await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body).unwrap();
    let document = roxmltree::Document::parse(&xml).expect("well formed schema");
    let root = document.root_element();
    assert_eq!(root.tag_name().name(), "schema");

    let element = descendants(root, XSD_NAMESPACE, "element")
        .into_iter()
        .find(|node| node.attribute("name") == Some("monaco_pois"))
        .expect("top level element");
    assert_eq!(
        element.attribute("substitutionGroup"),
        Some("gml:AbstractFeature")
    );
    let properties: HashMap<&str, &str> = descendants(
        descendant(root, XSD_NAMESPACE, "sequence"),
        XSD_NAMESPACE,
        "element",
    )
    .into_iter()
    .map(|node| {
        (
            node.attribute("name").unwrap(),
            node.attribute("type").unwrap(),
        )
    })
    .collect();
    assert_eq!(properties["geometry"], "gml:PointPropertyType");
    assert_eq!(properties["name"], "xsd:string");
}

#[tokio::test]
async fn wfs_describefeaturetype_without_a_typename_describes_every_type() {
    let (status, body) = get("/wfs?request=DescribeFeatureType").await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body).unwrap();
    let document = roxmltree::Document::parse(&xml).unwrap();
    let types = descendants(document.root_element(), XSD_NAMESPACE, "complexType");
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].attribute("name"), Some("monaco_poisType"));
}

#[tokio::test]
async fn wfs_describefeaturetype_rejects_an_unknown_type() {
    let (status, body) = get("/wfs?request=DescribeFeatureType&typenames=nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let xml = String::from_utf8(body).unwrap();
    let document = roxmltree::Document::parse(&xml).expect("well formed exception report");
    let root = document.root_element();
    assert_eq!(root.tag_name().name(), "ExceptionReport");
    assert_eq!(root.tag_name().namespace(), Some(OWS_1_1_NAMESPACE));
    let exception = descendant(root, OWS_1_1_NAMESPACE, "Exception");
    assert_eq!(exception.attribute("locator"), Some("typeNames"));
}

#[tokio::test]
async fn wfs_getfeature_hits_counts_without_returning_features() {
    let (status, body) = get("/wfs?request=GetFeature&typenames=monaco_pois&resulttype=hits").await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body).unwrap();
    let document = roxmltree::Document::parse(&xml).expect("well formed hits answer");
    let root = document.root_element();
    assert_eq!(root.tag_name().name(), "FeatureCollection");
    assert_eq!(root.tag_name().namespace(), Some(WFS_NAMESPACE));
    assert_eq!(root.attribute("numberMatched"), Some("3"));
    assert_eq!(root.attribute("numberReturned"), Some("0"));
    assert!(root.attribute("timeStamp").is_some());
}

#[tokio::test]
async fn wfs_getfeature_pages_with_startindex() {
    let (_, body) = get("/wfs?request=GetFeature&typenames=monaco_pois&startindex=1&count=1").await;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["numberMatched"], 3);
    assert_eq!(json["numberReturned"], 1);
    assert_eq!(json["features"][0]["properties"]["name"], "Palais Princier");
}

#[tokio::test]
async fn wfs_getfeature_accepts_the_advertised_output_format() {
    let (status, _) =
        get("/wfs?request=GetFeature&typenames=monaco_pois&outputformat=application/json").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn wfs_getfeature_rejects_an_output_format_it_cannot_produce() {
    let (status, body) =
        get("/wfs?request=GetFeature&typenames=monaco_pois&outputformat=text/xml").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let xml = String::from_utf8(body).unwrap();
    let document = roxmltree::Document::parse(&xml).unwrap();
    let exception = descendant(document.root_element(), OWS_1_1_NAMESPACE, "Exception");
    assert_eq!(exception.attribute("locator"), Some("outputFormat"));
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
