use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("layer not found: {0}")]
    LayerNotFound(String),

    #[error("unsupported CRS: {0}")]
    UnsupportedCrs(String),

    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("configuration error: {0}")]
    Config(String),
}
