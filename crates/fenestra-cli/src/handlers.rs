//! Axum request handlers for WMS, WFS, WMTS, and OGC API Features.

use crate::render::{Crs, bbox_to_4326, build_layer, parse_crs, resolve_style};
use crate::source::Collection;
use crate::{AppState, metrics_counter};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use fenestra_core::renderer::render_map;
use fenestra_core::{
    BboxFilter, CollectionInfo, ConformanceDeclaration, Feature, FeatureCollection, LandingPage,
    Link, ServiceConfig, WmsGetMapRequest, WmtsGetTileRequest, paginate_features, parse_sld,
    wmts_capabilities_xml,
};
use serde::Serialize;
use std::collections::HashMap;

/// Upper bound on features pulled from the source per request.
const FETCH_CAP: usize = 100_000;

/// OGC KVP parameters with case-insensitive keys.
struct Kvp(HashMap<String, String>);

impl Kvp {
    fn new(raw: HashMap<String, String>) -> Self {
        Self(
            raw.into_iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), v))
                .collect(),
        )
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .get(key)
            .map(String::as_str)
            .filter(|s| !s.is_empty())
    }

    fn first(&self, keys: &[&str]) -> Option<&str> {
        keys.iter().find_map(|k| self.get(k))
    }
}

fn png_response(bytes: Vec<u8>) -> Response {
    ([("content-type", "image/png")], bytes).into_response()
}

fn xml_response(body: String) -> Response {
    ([("content-type", "application/xml")], body).into_response()
}

fn bad_request(msg: impl std::fmt::Display) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}

fn upstream_error(msg: impl std::fmt::Display) -> Response {
    (StatusCode::BAD_GATEWAY, msg.to_string()).into_response()
}

/// Normalize a bbox so component 0/1 are the minimums.
fn norm_bbox(b: [f64; 4]) -> BboxFilter {
    BboxFilter {
        min_x: b[0].min(b[2]),
        min_y: b[1].min(b[3]),
        max_x: b[0].max(b[2]),
        max_y: b[1].max(b[3]),
    }
}

async fn config_with_layers(state: &AppState) -> ServiceConfig {
    let mut config = ServiceConfig::default();
    if let Ok(collections) = state.source.collections().await {
        config.layers = collections
            .iter()
            .map(|c| fenestra_core::LayerConfig {
                name: c.id.clone(),
                title: c.title.clone(),
                srs: vec!["EPSG:4326".to_string(), "EPSG:3857".to_string()],
                bbox: [-180.0, -90.0, 180.0, 90.0],
                source: String::new(),
            })
            .collect();
    }
    config
}

// ─── WMS ─────────────────────────────────────────────────────────────────────

pub async fn wms(
    State(state): State<AppState>,
    Query(raw): Query<HashMap<String, String>>,
) -> Response {
    metrics_counter("fenestra_wms_requests");
    let kvp = Kvp::new(raw);
    match kvp.get("request").unwrap_or("GetCapabilities") {
        "GetCapabilities" => {
            let config = config_with_layers(&state).await;
            xml_response(fenestra_core::capabilities::wms_capabilities_xml(&config))
        }
        "GetMap" => match render_getmap(&state, &kvp).await {
            Ok(png) => png_response(png),
            Err(e) => e,
        },
        other => bad_request(format!("Unsupported WMS request: {other}")),
    }
}

async fn load_sld(kvp: &Kvp) -> Option<fenestra_core::StyledLayerDescriptor> {
    if let Some(body) = kvp.get("sld_body") {
        return parse_sld(body).ok();
    }
    if let Some(url) = kvp.get("sld") {
        let text = reqwest::get(url).await.ok()?.text().await.ok()?;
        return parse_sld(&text).ok();
    }
    None
}

async fn render_getmap(state: &AppState, kvp: &Kvp) -> Result<Vec<u8>, Response> {
    let layers = kvp.get("layers").unwrap_or("").to_string();
    let bbox = kvp.get("bbox").unwrap_or("-180,-90,180,90").to_string();
    let width = kvp.get("width").and_then(|s| s.parse().ok()).unwrap_or(256);
    let height = kvp
        .get("height")
        .and_then(|s| s.parse().ok())
        .unwrap_or(256);
    let crs_str = kvp
        .first(&["crs", "srs"])
        .unwrap_or("EPSG:4326")
        .to_string();
    let crs = parse_crs(&crs_str);

    let request = WmsGetMapRequest {
        layers: layers.clone(),
        styles: kvp.get("styles").unwrap_or("").to_string(),
        crs: crs_str,
        bbox,
        width,
        height,
        format: kvp.get("format").unwrap_or("image/png").to_string(),
    };
    let bbox = request.parse_bbox().map_err(bad_request)?;
    let filter = norm_bbox(bbox_to_4326(bbox, crs));
    let sld = load_sld(kvp).await;

    let mut render_layers = Vec::new();
    for name in layers.split(',').filter(|s| !s.is_empty()) {
        let features = state
            .source
            .features(name, Some(FETCH_CAP))
            .await
            .map_err(upstream_error)?;
        let visible = filter.filter_features(&features);
        let style = resolve_style(sld.as_ref(), name);
        render_layers.push(build_layer(name, &visible, crs, style));
    }

    Ok(render_map(&request, &render_layers))
}

// ─── WFS ─────────────────────────────────────────────────────────────────────

pub async fn wfs(
    State(state): State<AppState>,
    Query(raw): Query<HashMap<String, String>>,
) -> Response {
    metrics_counter("fenestra_wfs_requests");
    let kvp = Kvp::new(raw);
    match kvp.get("request").unwrap_or("GetCapabilities") {
        "GetCapabilities" => {
            let config = config_with_layers(&state).await;
            xml_response(fenestra_core::capabilities::wfs_capabilities_xml(&config))
        }
        "GetFeature" => match get_feature(&state, &kvp).await {
            Ok(fc) => Json(fc).into_response(),
            Err(e) => e,
        },
        other => bad_request(format!("Unsupported WFS request: {other}")),
    }
}

async fn get_feature(state: &AppState, kvp: &Kvp) -> Result<FeatureCollection, Response> {
    let type_names = kvp
        .first(&["typenames", "typename", "type_names"])
        .unwrap_or("")
        .to_string();
    let count = kvp
        .first(&["count", "maxfeatures"])
        .and_then(|s| s.parse::<usize>().ok());
    let bbox_filter = kvp
        .get("bbox")
        .and_then(BboxFilter::parse)
        .map(|f| norm_bbox([f.min_x, f.min_y, f.max_x, f.max_y]));

    let mut collected: Vec<Feature> = Vec::new();
    for name in type_names.split(',').filter(|s| !s.is_empty()) {
        let features = state
            .source
            .features(name, count.or(Some(FETCH_CAP)))
            .await
            .map_err(upstream_error)?;
        let features = match &bbox_filter {
            Some(f) => f.filter_features(&features),
            None => features,
        };
        collected.extend(features);
    }
    if let Some(count) = count {
        collected.truncate(count);
    }
    Ok(FeatureCollection::new(collected))
}

// ─── WMTS ────────────────────────────────────────────────────────────────────

pub async fn wmts(
    State(state): State<AppState>,
    Query(raw): Query<HashMap<String, String>>,
) -> Response {
    metrics_counter("fenestra_wms_requests");
    let kvp = Kvp::new(raw);
    match kvp.get("request").unwrap_or("GetCapabilities") {
        "GetCapabilities" => {
            let names: Vec<String> = state
                .source
                .collections()
                .await
                .map(|c| c.into_iter().map(|c| c.id).collect())
                .unwrap_or_default();
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            xml_response(wmts_capabilities_xml(&refs, &state.base_url))
        }
        "GetTile" => {
            let layer = kvp.get("layer").unwrap_or("");
            let matrix = kvp.first(&["tilematrix", "tile_matrix"]).unwrap_or("0");
            let row = kvp
                .first(&["tilerow", "tile_row"])
                .and_then(|s| s.parse().ok());
            let col = kvp
                .first(&["tilecol", "tile_col"])
                .and_then(|s| s.parse().ok());
            match (row, col) {
                (Some(row), Some(col)) => render_tile(&state, layer, matrix, row, col).await,
                _ => bad_request("GetTile requires TILEROW and TILECOL"),
            }
        }
        other => bad_request(format!("Unsupported WMTS request: {other}")),
    }
}

pub async fn wmts_rest(
    State(state): State<AppState>,
    Path((layer, _tms, matrix, row, col)): Path<(String, String, String, u32, String)>,
) -> Response {
    metrics_counter("fenestra_wms_requests");
    let col = col.trim_end_matches(".png");
    match col.parse::<u32>() {
        Ok(col) => render_tile(&state, &layer, &matrix, row, col).await,
        Err(_) => bad_request("invalid tile column"),
    }
}

async fn render_tile(state: &AppState, layer: &str, matrix: &str, row: u32, col: u32) -> Response {
    let tile = match WmtsGetTileRequest::parse(
        layer,
        "default",
        "WebMercatorQuad",
        matrix,
        row,
        col,
        "image/png",
    ) {
        Ok(t) => t,
        Err(e) => return bad_request(e),
    };
    let (min_x, min_y, max_x, max_y) = tile.tile_bounds();
    let request = WmsGetMapRequest {
        layers: layer.to_string(),
        styles: String::new(),
        crs: "EPSG:3857".to_string(),
        bbox: format!("{min_x},{min_y},{max_x},{max_y}"),
        width: 256,
        height: 256,
        format: "image/png".to_string(),
    };
    let filter = norm_bbox(bbox_to_4326([min_x, min_y, max_x, max_y], Crs::WebMercator));
    let features = match state.source.features(layer, Some(FETCH_CAP)).await {
        Ok(f) => f,
        Err(e) => return upstream_error(e),
    };
    let visible = filter.filter_features(&features);
    let render_layers = vec![build_layer(
        layer,
        &visible,
        Crs::WebMercator,
        resolve_style(None, layer),
    )];
    png_response(render_map(&request, &render_layers))
}

// ─── OGC API Features ────────────────────────────────────────────────────────

#[derive(Serialize)]
struct CollectionsResponse {
    collections: Vec<CollectionInfo>,
    links: Vec<Link>,
}

fn collection_info(collection: &Collection, base_url: &str) -> CollectionInfo {
    CollectionInfo {
        id: collection.id.clone(),
        title: collection.title.clone(),
        description: format!("Ptolemy dataset {}", collection.id),
        item_type: "feature".to_string(),
        crs: vec![
            "http://www.opengis.net/def/crs/OGC/1.3/CRS84".to_string(),
            "http://www.opengis.net/def/crs/EPSG/0/3857".to_string(),
        ],
        links: vec![Link {
            href: format!("{base_url}/ogc/collections/{}/items", collection.id),
            rel: "items".to_string(),
            media_type: Some("application/geo+json".to_string()),
            title: Some(collection.title.clone()),
        }],
    }
}

pub async fn landing(State(state): State<AppState>) -> Response {
    let page = LandingPage::new(
        "Fenestra OGC API",
        "OGC API Features backed by Ptolemy",
        &format!("{}/ogc", state.base_url),
    );
    Json(page).into_response()
}

pub async fn conformance() -> Response {
    Json(ConformanceDeclaration::ogc_api_features_core()).into_response()
}

pub async fn collections(State(state): State<AppState>) -> Response {
    match state.source.collections().await {
        Ok(cols) => {
            let collections = cols
                .iter()
                .map(|c| collection_info(c, &state.base_url))
                .collect();
            Json(CollectionsResponse {
                collections,
                links: vec![Link {
                    href: format!("{}/ogc/collections", state.base_url),
                    rel: "self".to_string(),
                    media_type: Some("application/json".to_string()),
                    title: None,
                }],
            })
            .into_response()
        }
        Err(e) => upstream_error(e),
    }
}

pub async fn collection(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.source.collections().await {
        Ok(cols) => match cols.iter().find(|c| c.id == id) {
            Some(c) => Json(collection_info(c, &state.base_url)).into_response(),
            None => (StatusCode::NOT_FOUND, format!("collection {id} not found")).into_response(),
        },
        Err(e) => upstream_error(e),
    }
}

pub async fn items(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(raw): Query<HashMap<String, String>>,
) -> Response {
    let kvp = Kvp::new(raw);
    let limit = kvp
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10);
    let offset = kvp
        .get("offset")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let bbox_filter = kvp
        .get("bbox")
        .and_then(BboxFilter::parse)
        .map(|f| norm_bbox([f.min_x, f.min_y, f.max_x, f.max_y]));

    let features = match state.source.features(&id, Some(FETCH_CAP)).await {
        Ok(f) => f,
        Err(e) => return upstream_error(e),
    };
    let features = match &bbox_filter {
        Some(f) => f.filter_features(&features),
        None => features,
    };
    Json(paginate_features(&features, offset, limit)).into_response()
}
