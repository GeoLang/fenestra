use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use fenestra_cli::source::{Collection, FeatureSource, SourceError};
use fenestra_cli::{AppState, build_router};
use fenestra_core::Feature;
use std::sync::Arc;
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

async fn status(uri: &str, method: &str) -> StatusCode {
    let state = AppState {
        source: Arc::new(EmptySource),
        coverages: Arc::new(fenestra_cli::coverage::CoverageCatalog::new(
            "nonexistent-coverage-dir",
        )),
        base_url: "http://localhost:8080".into(),
    };
    build_router(state)
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn sld_symbology_needs_a_token_when_auth_is_on() {
    // cargo runs every test in this file in one process, so this env var reaches all of them
    unsafe { std::env::set_var("FENESTRA_JWT_SECRET", "test-secret") };

    assert_eq!(
        status("/sld/symbology", "POST").await,
        StatusCode::UNAUTHORIZED,
        "the conversion endpoint is behind the same middleware as the OGC services"
    );
    assert_eq!(
        status("/health", "GET").await,
        StatusCode::OK,
        "health stays public"
    );

    unsafe { std::env::remove_var("FENESTRA_JWT_SECRET") };
}
