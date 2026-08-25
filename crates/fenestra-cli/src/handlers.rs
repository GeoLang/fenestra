//! Axum request handlers for WMS, WFS, WMTS, WCS, and OGC API Features.

use crate::coverage::{CoverageError, bbox_of, crop};
use crate::render::{Crs, bbox_to_4326, build_layer, parse_crs, resolve_style};
use crate::source::Collection;
use crate::{AppState, metrics_counter};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use fenestra_core::crs::{EPSG_3857, EPSG_4326};
use fenestra_core::renderer::render_map;
use fenestra_core::xml::{OWS_1_1_NAMESPACE, OWS_2_0_NAMESPACE};
use fenestra_core::{
    BboxFilter, CollectionInfo, ConformanceDeclaration, DESCRIBE_FEATURE_TYPE_FORMAT, Feature,
    FeatureTypeSchema, GEOJSON_OUTPUT_FORMAT, LandingPage, Link, ServiceConfig, WmsGetMapRequest,
    WmtsGetTileRequest, describe_feature_type_xml, features_bbox, paginate_features, parse_sld,
    sld_to_symbology, wfs_hits_xml, wmts_capabilities_xml,
};
use serde::Serialize;
use std::collections::HashMap;

/// Upper bound on features pulled from the source per request.
const FETCH_CAP: usize = 100_000;

/// Extent advertised for a collection whose features have no geometry.
const WORLD_BBOX: [f64; 4] = [-180.0, -90.0, 180.0, 90.0];

/// GetFeature output formats accepted as a request for GeoJSON.
const GEOJSON_OUTPUT_FORMAT_ALIASES: [&str; 4] = [
    GEOJSON_OUTPUT_FORMAT,
    "json",
    "geojson",
    "application/geo+json",
];

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

/// Fetch up to `limit` features per layer concurrently and reduce each result
/// with `map`, keeping the order of `layers`. A layer the source cannot serve
/// is mapped from an empty feature list.
async fn per_layer<T: Send + 'static>(
    state: &AppState,
    layers: &[String],
    limit: usize,
    map: fn(Vec<Feature>) -> T,
) -> Vec<T> {
    let mut tasks = tokio::task::JoinSet::new();
    for (index, layer) in layers.iter().enumerate() {
        let source = state.source.clone();
        let layer = layer.clone();
        tasks.spawn(async move {
            let features = source
                .features(&layer, Some(limit))
                .await
                .unwrap_or_default();
            (index, map(features))
        });
    }
    let mut results: Vec<Option<T>> = (0..layers.len()).map(|_| None).collect();
    while let Some(joined) = tasks.join_next().await {
        let (index, value) = joined.expect("layer fetch task");
        results[index] = Some(value);
    }
    results
        .into_iter()
        .map(|value| value.expect("every layer mapped"))
        .collect()
}

fn extent_of(features: Vec<Feature>) -> [f64; 4] {
    features_bbox(&features).unwrap_or(WORLD_BBOX)
}

fn first_of(features: Vec<Feature>) -> Option<Feature> {
    features.into_iter().next()
}

/// Build the capabilities view of the source: one layer per collection, with
/// the extent of its features.
async fn config_with_layers(state: &AppState) -> ServiceConfig {
    let mut config = ServiceConfig::default();
    let Ok(collections) = state.source.collections().await else {
        return config;
    };
    let names: Vec<String> = collections.iter().map(|c| c.id.clone()).collect();
    let extents = per_layer(state, &names, FETCH_CAP, extent_of).await;
    config.layers = collections
        .iter()
        .zip(extents)
        .map(|(collection, bbox)| fenestra_core::LayerConfig {
            name: collection.id.clone(),
            title: collection.title.clone(),
            srs: vec![EPSG_4326.to_string(), EPSG_3857.to_string()],
            bbox,
            source: String::new(),
        })
        .collect();
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
            xml_response(fenestra_core::capabilities::wms_capabilities_xml(
                &config,
                &state.base_url,
            ))
        }
        "GetMap" => render_getmap(&state, &kvp).await,
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

async fn render_getmap(state: &AppState, kvp: &Kvp) -> Response {
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
    let bbox = match request.parse_bbox() {
        Ok(b) => b,
        Err(e) => return bad_request(e),
    };
    let filter = norm_bbox(bbox_to_4326(bbox, crs));
    let sld = load_sld(kvp).await;

    let mut render_layers = Vec::new();
    for name in layers.split(',').filter(|s| !s.is_empty()) {
        let features = match state.source.features(name, Some(FETCH_CAP)).await {
            Ok(f) => f,
            Err(e) => return upstream_error(e),
        };
        let visible = filter.filter_features(&features);
        let style = resolve_style(sld.as_ref(), name);
        render_layers.push(build_layer(name, &visible, crs, style));
    }

    png_response(render_map(&request, &render_layers))
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
            xml_response(fenestra_core::capabilities::wfs_capabilities_xml(
                &config,
                &state.base_url,
            ))
        }
        "DescribeFeatureType" => describe_feature_type(&state, &kvp).await,
        "GetFeature" => get_feature(&state, &kvp).await,
        other => wfs_exception(
            StatusCode::BAD_REQUEST,
            "OperationNotSupported",
            "request",
            format!("Unsupported WFS request: {other}"),
        ),
    }
}

/// OWS ExceptionReport response in the OWS Common version WFS 2.0 is bound to.
fn wfs_exception(
    status: StatusCode,
    code: &str,
    locator: &str,
    message: impl std::fmt::Display,
) -> Response {
    (
        status,
        [("content-type", "application/xml")],
        fenestra_core::ows_exception_xml(OWS_1_1_NAMESPACE, code, locator, &message.to_string()),
    )
        .into_response()
}

/// The type names a WFS request asks for, with any namespace prefix dropped.
fn requested_type_names(kvp: &Kvp) -> Vec<&str> {
    kvp.first(&["typenames", "typename", "type_names"])
        .unwrap_or("")
        .split(',')
        .map(|name| name.rsplit(':').next().unwrap_or(name).trim())
        .filter(|name| !name.is_empty())
        .collect()
}

async fn describe_feature_type(state: &AppState, kvp: &Kvp) -> Response {
    let collections = match state.source.collections().await {
        Ok(c) => c,
        Err(e) => return upstream_error(e),
    };
    let requested = requested_type_names(kvp);
    let selected: Vec<&Collection> = if requested.is_empty() {
        collections.iter().collect()
    } else {
        let mut selected = Vec::new();
        for name in requested {
            let Some(collection) = collections.iter().find(|c| c.id == name) else {
                return wfs_exception(
                    StatusCode::NOT_FOUND,
                    "InvalidParameterValue",
                    "typeNames",
                    format!("unknown feature type {name}"),
                );
            };
            selected.push(collection);
        }
        selected
    };

    let names: Vec<String> = selected.iter().map(|c| c.id.clone()).collect();
    let samples = per_layer(state, &names, 1, first_of).await;
    let schemas: Vec<FeatureTypeSchema> = selected
        .iter()
        .zip(&samples)
        .map(|(collection, sample)| {
            FeatureTypeSchema::derive(&collection.id, &collection.geometry_type, sample.as_ref())
        })
        .collect();
    (
        [("content-type", DESCRIBE_FEATURE_TYPE_FORMAT)],
        describe_feature_type_xml(&schemas),
    )
        .into_response()
}

async fn get_feature(state: &AppState, kvp: &Kvp) -> Response {
    if let Some(format) = kvp.first(&["outputformat", "output_format"])
        && !GEOJSON_OUTPUT_FORMAT_ALIASES
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(format))
    {
        return wfs_exception(
            StatusCode::BAD_REQUEST,
            "InvalidParameterValue",
            "outputFormat",
            format!("unsupported outputFormat {format}, only {GEOJSON_OUTPUT_FORMAT}"),
        );
    }
    if let Some(srs_name) = kvp.first(&["srsname", "srs_name"])
        && !crs_matches(EPSG_4326, srs_name)
    {
        return wfs_exception(
            StatusCode::BAD_REQUEST,
            "InvalidParameterValue",
            "srsName",
            format!("unsupported srsName {srs_name}, features are served in {EPSG_4326}"),
        );
    }
    let result_type = kvp
        .first(&["resulttype", "result_type"])
        .unwrap_or("results");
    let hits_only = result_type.eq_ignore_ascii_case("hits");
    let count = kvp
        .first(&["count", "maxfeatures"])
        .and_then(|s| s.parse::<usize>().ok());
    let start_index = kvp
        .first(&["startindex", "start_index"])
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let bbox_filter = kvp
        .get("bbox")
        .and_then(BboxFilter::parse)
        .map(|f| norm_bbox([f.min_x, f.min_y, f.max_x, f.max_y]));

    // numberMatched and resultType=hits both need the whole set, so the page
    // narrows the answer rather than the fetch
    let mut collected: Vec<Feature> = Vec::new();
    for name in requested_type_names(kvp) {
        let features = match state.source.features(name, Some(FETCH_CAP)).await {
            Ok(f) => f,
            Err(e) => return upstream_error(e),
        };
        let features = match &bbox_filter {
            Some(f) => f.filter_features(&features),
            None => features,
        };
        collected.extend(features);
    }

    if hits_only {
        let timestamp = chrono::Utc::now().to_rfc3339();
        return xml_response(wfs_hits_xml(collected.len(), &timestamp));
    }
    Json(paginate_features(
        &collected,
        start_index,
        count.unwrap_or(usize::MAX),
    ))
    .into_response()
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
            let config = config_with_layers(&state).await;
            xml_response(wmts_capabilities_xml(&config.layers, &state.base_url))
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

// ─── WCS ─────────────────────────────────────────────────────────────────────

fn tiff_response(bytes: Vec<u8>) -> Response {
    ([("content-type", "image/tiff")], bytes).into_response()
}

/// OWS ExceptionReport response, the WCS error convention.
fn wcs_exception(
    status: StatusCode,
    code: &str,
    locator: &str,
    message: impl std::fmt::Display,
) -> Response {
    (
        status,
        [("content-type", "application/xml")],
        fenestra_core::ows_exception_xml(OWS_2_0_NAMESPACE, code, locator, &message.to_string()),
    )
        .into_response()
}

fn coverage_error_response(err: CoverageError) -> Response {
    match err {
        CoverageError::NotFound(id) => wcs_exception(
            StatusCode::NOT_FOUND,
            "NoSuchCoverage",
            "coverageId",
            format!("coverage {id} not found"),
        ),
        other => wcs_exception(
            StatusCode::INTERNAL_SERVER_ERROR,
            "NoApplicableCode",
            "coverageId",
            other,
        ),
    }
}

/// True when `given` names the same CRS as native `EPSG:<code>`.
fn crs_matches(native: &str, given: &str) -> bool {
    let code = native.strip_prefix("EPSG:").unwrap_or(native);
    let given = given.to_ascii_lowercase();
    given == native.to_ascii_lowercase()
        || given == format!("http://www.opengis.net/def/crs/epsg/0/{code}")
        || given == format!("urn:ogc:def:crs:epsg::{code}")
}

// WCS KVP allows repeated SUBSET parameters, so this handler extracts query
// pairs instead of a map.
pub async fn wcs(
    State(state): State<AppState>,
    Query(pairs): Query<Vec<(String, String)>>,
) -> Response {
    metrics_counter("fenestra_wcs_requests");
    let subsets: Vec<String> = pairs
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("subset"))
        .map(|(_, v)| v.clone())
        .collect();
    let kvp = Kvp::new(pairs.into_iter().collect());
    match kvp.get("request").unwrap_or("GetCapabilities") {
        "GetCapabilities" => {
            let ids = state.coverages.ids();
            let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
            let title = fenestra_core::ServiceConfig::default().title;
            xml_response(fenestra_core::wcs_capabilities_xml(&title, &refs))
        }
        "DescribeCoverage" => describe_coverage(&state, &kvp),
        "GetCoverage" => get_coverage(&state, &kvp, &subsets),
        other => wcs_exception(
            StatusCode::BAD_REQUEST,
            "OperationNotSupported",
            "request",
            format!("Unsupported WCS request: {other}"),
        ),
    }
}

fn describe_coverage(state: &AppState, kvp: &Kvp) -> Response {
    let Some(ids) = kvp.get("coverageid") else {
        return wcs_exception(
            StatusCode::BAD_REQUEST,
            "MissingParameterValue",
            "coverageId",
            "COVERAGEID is required",
        );
    };
    let mut descriptions = Vec::new();
    for id in ids.split(',').filter(|s| !s.is_empty()) {
        match state.coverages.describe(id) {
            Ok(desc) => descriptions.push(desc),
            Err(e) => return coverage_error_response(e),
        }
    }
    xml_response(fenestra_core::describe_coverage_xml(&descriptions))
}

fn get_coverage(state: &AppState, kvp: &Kvp, subsets: &[String]) -> Response {
    let Some(coverage_id) = kvp.get("coverageid") else {
        return wcs_exception(
            StatusCode::BAD_REQUEST,
            "MissingParameterValue",
            "coverageId",
            "COVERAGEID is required",
        );
    };
    if let Some(format) = kvp.get("format")
        && !format.eq_ignore_ascii_case("image/tiff")
    {
        return wcs_exception(
            StatusCode::BAD_REQUEST,
            "InvalidParameterValue",
            "format",
            format!("unsupported format {format}, only image/tiff"),
        );
    }
    for unsupported in ["scalefactor", "scaleaxes", "scalesize", "interpolation"] {
        if kvp.get(unsupported).is_some() {
            return wcs_exception(
                StatusCode::BAD_REQUEST,
                "InvalidParameterValue",
                unsupported,
                format!("{unsupported} is not supported"),
            );
        }
    }

    let (raster, meta) = match state.coverages.read(coverage_id) {
        Ok(v) => v,
        Err(e) => return coverage_error_response(e),
    };
    let native_crs = crate::coverage::crs_string(meta.epsg);
    for crs_param in ["subsettingcrs", "outputcrs"] {
        if let Some(crs) = kvp.get(crs_param)
            && !crs_matches(&native_crs, crs)
        {
            return wcs_exception(
                StatusCode::BAD_REQUEST,
                "InvalidParameterValue",
                crs_param,
                format!("unsupported CRS {crs}, native CRS is {native_crs}"),
            );
        }
    }

    let mut request = fenestra_core::WcsGetCoverageRequest {
        coverage_id: coverage_id.to_string(),
        format: "image/tiff".to_string(),
        subset_x: None,
        subset_y: None,
        subset_time: None,
        scale_factor: None,
        range_subset: None,
        interpolation: None,
    };
    for subset in subsets {
        let (axis, spec) = match fenestra_core::parse_subset(subset) {
            Ok(parsed) => parsed,
            Err(fenestra_core::Error::InvalidAxisLabel(label)) => {
                return wcs_exception(
                    StatusCode::NOT_FOUND,
                    "InvalidAxisLabel",
                    "subset",
                    format!("unknown axis {label}"),
                );
            }
            Err(e) => {
                return wcs_exception(StatusCode::NOT_FOUND, "InvalidSubsetting", "subset", e);
            }
        };
        let slot = match axis {
            fenestra_core::SubsetAxis::X => &mut request.subset_x,
            fenestra_core::SubsetAxis::Y => &mut request.subset_y,
        };
        if slot.is_some() {
            return wcs_exception(
                StatusCode::NOT_FOUND,
                "InvalidSubsetting",
                "subset",
                "duplicate subset axis",
            );
        }
        *slot = Some(spec);
    }
    if let Err(e) = request.validate() {
        return wcs_exception(
            StatusCode::BAD_REQUEST,
            "InvalidParameterValue",
            "request",
            e,
        );
    }

    let bbox = request.effective_bbox(&bbox_of(&raster, &meta));
    let (out_raster, out_meta) = match crop(&raster, &meta, bbox) {
        Ok(v) => v,
        Err(e) => return wcs_exception(StatusCode::NOT_FOUND, "InvalidSubsetting", "subset", e),
    };
    let mut bytes = Vec::new();
    match terrano_core::write_geotiff(&out_raster, &out_meta, &mut bytes) {
        Ok(()) => tiff_response(bytes),
        Err(e) => wcs_exception(
            StatusCode::INTERNAL_SERVER_ERROR,
            "NoApplicableCode",
            "coverageId",
            e,
        ),
    }
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

// ─── SLD conversion ──────────────────────────────────────────────────────────

/// Convert a posted SLD document into viewer symbology. `layer` and `style`
/// pick one of several in the document; without them the first of each is used.
pub async fn sld_symbology(Query(raw): Query<HashMap<String, String>>, body: String) -> Response {
    metrics_counter("fenestra_sld_symbology_requests");
    let kvp = Kvp::new(raw);

    let sld = match parse_sld(&body) {
        Ok(sld) => sld,
        Err(e) => return bad_request(e),
    };
    match sld_to_symbology(&sld, kvp.get("layer"), kvp.get("style")) {
        Ok(conversion) => Json(conversion).into_response(),
        Err(e) => bad_request(e),
    }
}
