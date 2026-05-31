//! Print job rendering and output.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A print request submitted by a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintRequest {
    /// Which layout template to use.
    pub template: String,
    /// Override DPI (if different from template default).
    pub dpi: Option<u32>,
    /// Map center [x, y] in the layer's CRS.
    pub center: Option<[f64; 2]>,
    /// Map scale denominator (e.g. 25000 for 1:25,000).
    pub scale: Option<f64>,
    /// Override bbox [west, south, east, north].
    pub bbox: Option<[f64; 4]>,
    /// Layers to render.
    pub layers: Vec<PrintLayer>,
    /// Title text override.
    pub title: Option<String>,
    /// Output format.
    pub format: OutputFormat,
}

/// A layer in a print request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintLayer {
    /// Layer name.
    pub name: String,
    /// Optional SLD style override.
    pub style: Option<String>,
    /// Opacity (0.0 - 1.0).
    pub opacity: f32,
}

/// Output format for print jobs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum OutputFormat {
    #[default]
    Pdf,
    Png,
    Svg,
}

/// A submitted print job.
#[derive(Debug, Clone)]
pub struct PrintJob {
    /// Unique job ID.
    pub id: String,
    /// Print request parameters.
    pub request: PrintRequest,
    /// Job status.
    pub status: JobStatus,
}

/// Status of a print job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Rendering,
    Complete,
    Failed(String),
}

/// Output of a completed print job.
pub struct PrintOutput {
    /// Output bytes (PDF, PNG, or SVG).
    pub data: Vec<u8>,
    /// MIME type.
    pub content_type: String,
    /// Suggested filename.
    pub filename: String,
}

impl PrintJob {
    pub fn new(request: PrintRequest) -> Self {
        let id = Uuid::new_v4().to_string();
        let _ext = match request.format {
            OutputFormat::Pdf => "pdf",
            OutputFormat::Png => "png",
            OutputFormat::Svg => "svg",
        };
        Self {
            id,
            request,
            status: JobStatus::Queued,
        }
    }
}
