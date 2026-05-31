//! Prometheus metrics endpoint.

use axum::response::IntoResponse;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

static PROMETHEUS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Install the Prometheus metrics recorder. Call once at startup.
pub fn install() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder");
    PROMETHEUS_HANDLE.set(handle).ok();

    metrics::describe_counter!("fenestra_requests_total", "Total HTTP requests");
    metrics::describe_counter!("fenestra_wms_requests", "WMS requests");
    metrics::describe_counter!("fenestra_wfs_requests", "WFS requests");
    metrics::describe_histogram!(
        "fenestra_request_duration_seconds",
        "Request duration in seconds"
    );
}

/// Handler for GET /metrics — serves Prometheus text format.
pub async fn metrics_handler() -> impl IntoResponse {
    let output = match PROMETHEUS_HANDLE.get() {
        Some(handle) => handle.render(),
        None => "# HELP fenestra_up Server is running\nfenestra_up 1\n".to_string(),
    };
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        output,
    )
}
