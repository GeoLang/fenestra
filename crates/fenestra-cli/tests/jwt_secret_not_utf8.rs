#![cfg(unix)]

use fenestra_cli::coverage::CoverageCatalog;
use fenestra_cli::source::PtolemyFeatureSource;
use fenestra_cli::{AppState, build_router};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;

#[tokio::test]
#[should_panic(expected = "FENESTRA_JWT_SECRET is set but not UTF-8")]
async fn a_non_utf8_secret_refuses_to_build_the_router() {
    // FENESTRA_JWT_SECRET is process-wide and this binary has one test
    unsafe { std::env::set_var("FENESTRA_JWT_SECRET", OsStr::from_bytes(&[0xff])) };
    let _ = build_router(AppState {
        source: Arc::new(PtolemyFeatureSource::new("http://localhost:1")),
        coverages: Arc::new(CoverageCatalog::new("nonexistent-coverage-dir")),
        base_url: "http://localhost:8080".into(),
    });
}
