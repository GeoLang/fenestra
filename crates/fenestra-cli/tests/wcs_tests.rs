//! Integration tests driving the WCS routes against a generated GeoTIFF fixture.

use async_trait::async_trait;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use fenestra_cli::coverage::CoverageCatalog;
use fenestra_cli::source::{Collection, FeatureSource, SourceError};
use fenestra_cli::{AppState, build_router};
use fenestra_core::Feature;
use std::sync::Arc;
use terrano_core::{GeoTiffMetadata, Raster, read_geotiff, write_geotiff};
use tower::ServiceExt;

struct EmptySource;

#[async_trait]
impl FeatureSource for EmptySource {
    async fn collections(&self) -> Result<Vec<Collection>, SourceError> {
        Ok(Vec::new())
    }

    async fn features(
        &self,
        layer: &str,
        _limit: Option<usize>,
    ) -> Result<Vec<Feature>, SourceError> {
        Err(SourceError::NotFound(layer.to_string()))
    }
}

/// 4x3 grid over [10, 48.5, 12, 50] in EPSG:4326, 0.5 degree pixels,
/// value = row * 10 + col.
fn write_fixture(dir: &std::path::Path) {
    let mut raster = Raster::new(4, 3, 0.5, -9999.0);
    for row in 0..3 {
        for col in 0..4 {
            raster.set(row, col, (row * 10 + col) as f64);
        }
    }
    let meta = GeoTiffMetadata {
        origin_x: 10.0,
        origin_y: 50.0,
        pixel_width: 0.5,
        pixel_height: 0.5,
        epsg: 4326,
    };
    let mut file = std::fs::File::create(dir.join("dem.tif")).unwrap();
    write_geotiff(&raster, &meta, &mut file).unwrap();
}

fn app(coverage_dir: &std::path::Path) -> axum::Router {
    let state = AppState {
        source: Arc::new(EmptySource),
        coverages: Arc::new(CoverageCatalog::new(coverage_dir)),
        base_url: "http://localhost:8080".into(),
    };
    build_router(state)
}

async fn get(dir: &std::path::Path, uri: &str) -> (StatusCode, Vec<u8>) {
    let response = app(dir)
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, bytes.to_vec())
}

#[tokio::test]
async fn wcs_getcapabilities_lists_coverages() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(dir.path(), "/wcs?service=WCS&request=GetCapabilities").await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("<wcs:CoverageId>dem</wcs:CoverageId>"));
    assert!(xml.contains("version=\"2.0.1\""));
}

#[tokio::test]
async fn wcs_getcapabilities_empty_dir_is_valid() {
    let dir = tempfile::tempdir().unwrap();
    let (status, body) = get(dir.path(), "/wcs?service=WCS&request=GetCapabilities").await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("<wcs:Contents>"));
    assert!(!xml.contains("CoverageSummary"));
}

#[tokio::test]
async fn wcs_describecoverage_reports_envelope_and_grid() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(
        dir.path(),
        "/wcs?service=WCS&request=DescribeCoverage&coverageId=dem",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("axisLabels=\"Lat Long\""));
    assert!(xml.contains("<gml:lowerCorner>48.5 10</gml:lowerCorner>"));
    assert!(xml.contains("<gml:upperCorner>50 12</gml:upperCorner>"));
    assert!(xml.contains("<gml:high>3 2</gml:high>"));
    assert!(xml.contains("http://www.opengis.net/def/crs/EPSG/0/4326"));
    assert!(xml.contains("<swe:field name=\"band1\">"));
}

#[tokio::test]
async fn wcs_getcoverage_full_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(
        dir.path(),
        "/wcs?service=WCS&request=GetCoverage&coverageId=dem",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (raster, meta) = read_geotiff(&body).unwrap();
    assert_eq!(raster.width(), 4);
    assert_eq!(raster.height(), 3);
    assert_eq!(meta.epsg, 4326);
    assert!((meta.origin_x - 10.0).abs() < 1e-9);
    assert!((meta.origin_y - 50.0).abs() < 1e-9);
    assert_eq!(raster.get(2, 3), Some(23.0));
}

#[tokio::test]
async fn wcs_getcoverage_bbox_subset_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(
        dir.path(),
        "/wcs?service=WCS&request=GetCoverage&coverageId=dem\
         &subset=x(10.5,11.5)&subset=y(49.0,50.0)",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (raster, meta) = read_geotiff(&body).unwrap();
    // cols 1..3, rows 0..2 of the fixture
    assert_eq!(raster.width(), 2);
    assert_eq!(raster.height(), 2);
    assert!((meta.origin_x - 10.5).abs() < 1e-9);
    assert!((meta.origin_y - 50.0).abs() < 1e-9);
    assert_eq!(raster.get(0, 0), Some(1.0));
    assert_eq!(raster.get(1, 1), Some(12.0));
}

#[tokio::test]
async fn wcs_getcoverage_unknown_coverage_is_exception() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(
        dir.path(),
        "/wcs?service=WCS&request=GetCoverage&coverageId=nope",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("exceptionCode=\"NoSuchCoverage\""));
}

#[tokio::test]
async fn wcs_getcoverage_disjoint_bbox_is_exception() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(
        dir.path(),
        "/wcs?service=WCS&request=GetCoverage&coverageId=dem&subset=x(100,110)",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("exceptionCode=\"InvalidSubsetting\""));
}

#[tokio::test]
async fn wcs_getcoverage_reversed_bbox_is_exception() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(
        dir.path(),
        "/wcs?service=WCS&request=GetCoverage&coverageId=dem&subset=x(11.5,10.5)",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("exceptionCode=\"InvalidSubsetting\""));
}

#[tokio::test]
async fn wcs_getcoverage_bad_axis_is_exception() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(
        dir.path(),
        "/wcs?service=WCS&request=GetCoverage&coverageId=dem&subset=time(1,2)",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("exceptionCode=\"InvalidAxisLabel\""));
}

#[tokio::test]
async fn wcs_getcoverage_unsupported_format_and_crs_are_exceptions() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(
        dir.path(),
        "/wcs?service=WCS&request=GetCoverage&coverageId=dem&format=image/png",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("exceptionCode=\"InvalidParameterValue\""));

    let (status, body) = get(
        dir.path(),
        "/wcs?service=WCS&request=GetCoverage&coverageId=dem&outputCrs=EPSG:3857",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("exceptionCode=\"InvalidParameterValue\""));
}

#[tokio::test]
async fn wcs_describecoverage_missing_param_is_exception() {
    let dir = tempfile::tempdir().unwrap();
    write_fixture(dir.path());
    let (status, body) = get(dir.path(), "/wcs?service=WCS&request=DescribeCoverage").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let xml = String::from_utf8(body).unwrap();
    assert!(xml.contains("exceptionCode=\"MissingParameterValue\""));
}
