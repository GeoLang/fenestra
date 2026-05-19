use axum::{Router, extract::Query, response::IntoResponse, routing::get};
use clap::{Parser, Subcommand};
use fenestra_core::{
    ServiceConfig, WfsGetFeatureRequest, WfsResponse, WmsGetMapRequest, WmsResponse,
};
use serde::Deserialize;

#[derive(Parser)]
#[command(name = "fenestra", version, about = "OGC services gateway")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the OGC services HTTP server
    Serve {
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        /// Port to listen on
        #[arg(long, default_value_t = 8080)]
        port: u16,
    },
    /// Print default configuration
    Config,
}

#[derive(Deserialize)]
struct WmsQuery {
    #[allow(dead_code)]
    service: Option<String>,
    request: Option<String>,
    layers: Option<String>,
    styles: Option<String>,
    crs: Option<String>,
    bbox: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    format: Option<String>,
}

async fn wms_handler(Query(params): Query<WmsQuery>) -> impl IntoResponse {
    let request_type = params.request.as_deref().unwrap_or("GetCapabilities");
    match request_type {
        "GetCapabilities" => {
            let config = ServiceConfig::default();
            let xml = fenestra_core::capabilities::wms_capabilities_xml(&config);
            ([("content-type", "application/xml")], xml)
        }
        "GetMap" => {
            let _req = WmsGetMapRequest {
                layers: params.layers.unwrap_or_default(),
                styles: params.styles.unwrap_or_default(),
                crs: params.crs.unwrap_or_else(|| "EPSG:4326".to_string()),
                bbox: params.bbox.unwrap_or_else(|| "0,0,1,1".to_string()),
                width: params.width.unwrap_or(256),
                height: params.height.unwrap_or(256),
                format: params.format.unwrap_or_else(|| "image/png".to_string()),
            };
            let _response = WmsResponse::placeholder(256, 256);
            (
                [("content-type", "text/plain")],
                "GetMap placeholder — no renderer configured".to_string(),
            )
        }
        _ => (
            [("content-type", "text/plain")],
            format!("Unsupported WMS request: {request_type}"),
        ),
    }
}

#[derive(Deserialize)]
struct WfsQuery {
    request: Option<String>,
    type_names: Option<String>,
    count: Option<u32>,
    bbox: Option<String>,
    output_format: Option<String>,
}

async fn wfs_handler(Query(params): Query<WfsQuery>) -> impl IntoResponse {
    let request_type = params.request.as_deref().unwrap_or("GetCapabilities");
    match request_type {
        "GetCapabilities" => {
            let config = ServiceConfig::default();
            let xml = fenestra_core::capabilities::wfs_capabilities_xml(&config);
            ([("content-type", "application/xml")], xml)
        }
        "GetFeature" => {
            let _req = WfsGetFeatureRequest {
                type_names: params.type_names.unwrap_or_default(),
                count: params.count,
                bbox: params.bbox,
                output_format: params.output_format,
            };
            let response = WfsResponse::empty_geojson();
            ([("content-type", "application/geo+json")], response.body)
        }
        _ => (
            [("content-type", "text/plain")],
            format!("Unsupported WFS request: {request_type}"),
        ),
    }
}

async fn health() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Serve { host, port } => {
            let app = Router::new()
                .route("/health", get(health))
                .route("/wms", get(wms_handler))
                .route("/wfs", get(wfs_handler));

            let addr = format!("{host}:{port}");
            println!("Fenestra OGC server listening on {addr}");
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        }
        Commands::Config => {
            let config = ServiceConfig::default();
            println!(
                "{}",
                serde_json::to_string_pretty(&config).unwrap_or_else(|_| format!("{config:?}"))
            );
        }
    }
}
