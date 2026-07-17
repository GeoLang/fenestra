//! Fenestra HTTP server: router assembly and shared state.

pub mod auth;
pub mod coverage;
pub mod handlers;
pub mod metrics;
pub mod render;
pub mod source;

use axum::routing::get;
use axum::{Router, middleware};
use coverage::CoverageCatalog;
use source::FeatureSource;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

/// Shared application state passed to every handler.
#[derive(Clone)]
pub struct AppState {
    pub source: Arc<dyn FeatureSource>,
    pub coverages: Arc<CoverageCatalog>,
    pub base_url: String,
}

/// Increment a named request counter.
pub fn metrics_counter(name: &'static str) {
    ::metrics::counter!(name).increment(1);
}

async fn health() -> &'static str {
    "OK"
}

async fn liveness() -> &'static str {
    "ok"
}

async fn readiness() -> &'static str {
    "ready"
}

/// Build the full OGC services router.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/healthz", get(liveness))
        .route("/readyz", get(readiness))
        .route("/metrics", get(metrics::metrics_handler))
        .route("/wms", get(handlers::wms))
        .route("/wcs", get(handlers::wcs))
        .route("/wfs", get(handlers::wfs))
        .route("/wmts", get(handlers::wmts))
        .route(
            "/wmts/{layer}/{tms}/{matrix}/{row}/{col}",
            get(handlers::wmts_rest),
        )
        .route("/ogc/", get(handlers::landing))
        .route("/ogc/conformance", get(handlers::conformance))
        .route("/ogc/collections", get(handlers::collections))
        .route("/ogc/collections/{id}", get(handlers::collection))
        .route("/ogc/collections/{id}/items", get(handlers::items))
        .with_state(state)
        .layer(middleware::from_fn(auth::auth_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
}
