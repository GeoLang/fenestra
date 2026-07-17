use clap::{Parser, Subcommand};
use fenestra_cli::coverage::CoverageCatalog;
use fenestra_cli::source::PtolemyFeatureSource;
use fenestra_cli::{AppState, build_router, metrics};
use fenestra_core::ServiceConfig;
use std::sync::Arc;

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

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Serve { host, port } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::from_default_env()
                        .add_directive("fenestra=info".parse().unwrap()),
                )
                .init();

            metrics::install();

            let source = Arc::new(PtolemyFeatureSource::from_env());
            let state = AppState {
                source,
                coverages: Arc::new(CoverageCatalog::from_env()),
                base_url: format!("http://{host}:{port}"),
            };
            let app = build_router(state);

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
