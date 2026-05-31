//! Layout templates for map printing.

use serde::{Deserialize, Serialize};

/// Standard page sizes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PageSize {
    A4,
    A3,
    A2,
    A1,
    A0,
    Letter,
    Tabloid,
    Custom { width_mm: f64, height_mm: f64 },
}

impl PageSize {
    /// Returns (width_mm, height_mm) for the page size.
    pub fn dimensions_mm(&self) -> (f64, f64) {
        match self {
            Self::A4 => (210.0, 297.0),
            Self::A3 => (297.0, 420.0),
            Self::A2 => (420.0, 594.0),
            Self::A1 => (594.0, 841.0),
            Self::A0 => (841.0, 1189.0),
            Self::Letter => (215.9, 279.4),
            Self::Tabloid => (279.4, 431.8),
            Self::Custom {
                width_mm,
                height_mm,
            } => (*width_mm, *height_mm),
        }
    }
}

/// Orientation of the printed page.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

/// A print layout template defining element placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutTemplate {
    /// Template name.
    pub name: String,
    /// Page size.
    pub page_size: PageSize,
    /// Page orientation.
    pub orientation: Orientation,
    /// DPI for raster rendering.
    pub dpi: u32,
    /// Elements placed on the page.
    pub elements: Vec<PrintElement>,
}

/// An element positioned on the print layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintElement {
    /// Element type.
    pub kind: ElementKind,
    /// X position from left edge (mm).
    pub x_mm: f64,
    /// Y position from top edge (mm).
    pub y_mm: f64,
    /// Width (mm).
    pub width_mm: f64,
    /// Height (mm).
    pub height_mm: f64,
}

/// Types of elements that can appear on a print layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElementKind {
    /// The main map viewport.
    Map {
        layers: Vec<String>,
        bbox: Option<[f64; 4]>,
        srid: u32,
    },
    /// Title text.
    Title { text: String, font_size: f32 },
    /// Legend (auto-generated from layer styles).
    Legend,
    /// Scale bar.
    ScaleBar,
    /// North arrow.
    NorthArrow,
    /// Overview/locator map inset.
    OverviewMap { layers: Vec<String> },
    /// Attribution / copyright text.
    Attribution { text: String },
    /// Arbitrary image (logo, watermark).
    Image { url: String },
}
