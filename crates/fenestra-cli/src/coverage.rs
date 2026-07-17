//! Coverage catalog backed by a directory of GeoTIFF files.
//!
//! Each `.tif`/`.tiff` file is one coverage, id = file stem. Files are read
//! lazily; parsed descriptions are cached. A missing directory yields an
//! empty catalog.

use fenestra_core::{CoverageDescription, RangeField};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use terrano_core::{GeoTiffMetadata, Raster, read_geotiff};

#[derive(Debug)]
pub enum CoverageError {
    NotFound(String),
    Io(String),
    Invalid(String),
}

impl std::fmt::Display for CoverageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoverageError::NotFound(s) => write!(f, "coverage not found: {s}"),
            CoverageError::Io(s) => write!(f, "io error: {s}"),
            CoverageError::Invalid(s) => write!(f, "invalid coverage: {s}"),
        }
    }
}

impl std::error::Error for CoverageError {}

/// Catalog of GeoTIFF coverages in a single directory.
pub struct CoverageCatalog {
    dir: PathBuf,
    descriptions: Mutex<HashMap<String, CoverageDescription>>,
}

impl CoverageCatalog {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            descriptions: Mutex::new(HashMap::new()),
        }
    }

    /// Build from `COVERAGE_DIR`, defaulting to `./coverages`.
    pub fn from_env() -> Self {
        let dir = std::env::var("COVERAGE_DIR").unwrap_or_else(|_| "coverages".to_string());
        Self::new(dir)
    }

    /// Sorted coverage ids (file stems of `.tif`/`.tiff` files).
    pub fn ids(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let ext = path.extension()?.to_str()?.to_ascii_lowercase();
                if ext == "tif" || ext == "tiff" {
                    Some(path.file_stem()?.to_str()?.to_string())
                } else {
                    None
                }
            })
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    fn path_for(&self, id: &str) -> Result<PathBuf, CoverageError> {
        let not_found = || CoverageError::NotFound(id.to_string());
        if id.contains('/') || id.contains('\\') || id.contains("..") {
            return Err(not_found());
        }
        ["tif", "tiff"]
            .iter()
            .map(|ext| self.dir.join(format!("{id}.{ext}")))
            .find(|path| path.is_file())
            .ok_or_else(not_found)
    }

    /// Read the full raster and georeferencing for a coverage.
    pub fn read(&self, id: &str) -> Result<(Raster, GeoTiffMetadata), CoverageError> {
        let path = self.path_for(id)?;
        let bytes = std::fs::read(&path).map_err(|e| CoverageError::Io(e.to_string()))?;
        read_geotiff(&bytes).map_err(|e| CoverageError::Invalid(e.to_string()))
    }

    /// Describe a coverage, caching the result.
    pub fn describe(&self, id: &str) -> Result<CoverageDescription, CoverageError> {
        if let Some(desc) = self.descriptions.lock().unwrap().get(id) {
            return Ok(desc.clone());
        }
        let (raster, meta) = self.read(id)?;
        let desc = CoverageDescription {
            coverage_id: id.to_string(),
            crs: crs_string(meta.epsg),
            bbox: bbox_of(&raster, &meta),
            grid_size: [raster.width() as u32, raster.height() as u32],
            range_type: vec![RangeField {
                name: "band1".to_string(),
                data_type: "float64".to_string(),
                uom: None,
                nil_values: vec![raster.nodata],
            }],
            native_format: "image/tiff".to_string(),
        };
        self.descriptions
            .lock()
            .unwrap()
            .insert(id.to_string(), desc.clone());
        Ok(desc)
    }
}

/// terrano reports epsg 0 when the file lacks a CRS geokey; declare WGS84 then.
pub fn crs_string(epsg: u16) -> String {
    format!("EPSG:{}", if epsg == 0 { 4326 } else { u32::from(epsg) })
}

/// Native-CRS extent of a raster: [min_x, min_y, max_x, max_y].
pub fn bbox_of(raster: &Raster, meta: &GeoTiffMetadata) -> [f64; 4] {
    [
        meta.origin_x,
        meta.origin_y - meta.pixel_height * raster.height() as f64,
        meta.origin_x + meta.pixel_width * raster.width() as f64,
        meta.origin_y,
    ]
}

/// Crop a raster to the pixel window covering `bbox` (native CRS).
///
/// The window snaps outward to pixel boundaries and clamps to the grid, so a
/// slice (zero-extent bbox) still yields one pixel.
pub fn crop(
    raster: &Raster,
    meta: &GeoTiffMetadata,
    bbox: [f64; 4],
) -> Result<(Raster, GeoTiffMetadata), CoverageError> {
    let [min_x, min_y, max_x, max_y] = bbox;
    if min_x > max_x || min_y > max_y {
        return Err(CoverageError::Invalid(
            "subset low exceeds high".to_string(),
        ));
    }
    let full = bbox_of(raster, meta);
    if max_x < full[0] || min_x > full[2] || max_y < full[1] || min_y > full[3] {
        return Err(CoverageError::Invalid(
            "subset does not intersect coverage".to_string(),
        ));
    }

    let width = raster.width() as i64;
    let height = raster.height() as i64;
    let col0 = (((min_x - meta.origin_x) / meta.pixel_width).floor() as i64).clamp(0, width - 1);
    let col1 = (((max_x - meta.origin_x) / meta.pixel_width).ceil() as i64).clamp(col0 + 1, width);
    let row0 = (((meta.origin_y - max_y) / meta.pixel_height).floor() as i64).clamp(0, height - 1);
    let row1 =
        (((meta.origin_y - min_y) / meta.pixel_height).ceil() as i64).clamp(row0 + 1, height);

    let (col0, row0) = (col0 as usize, row0 as usize);
    let out_width = col1 as usize - col0;
    let out_height = row1 as usize - row0;
    let mut out = Raster::new(out_width, out_height, raster.cell_size, raster.nodata);
    for row in 0..out_height {
        for col in 0..out_width {
            if let Some(value) = raster.get(row0 + row, col0 + col) {
                out.set(row, col, value);
            }
        }
    }
    let out_meta = GeoTiffMetadata {
        origin_x: meta.origin_x + col0 as f64 * meta.pixel_width,
        origin_y: meta.origin_y - row0 as f64 * meta.pixel_height,
        pixel_width: meta.pixel_width,
        pixel_height: meta.pixel_height,
        epsg: meta.epsg,
    };
    Ok((out, out_meta))
}
