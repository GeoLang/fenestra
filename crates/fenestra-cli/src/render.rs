//! Projection helpers and render-layer assembly.
//!
//! Ptolemy serves features in EPSG:4326. WMS/WMTS requests may use 4326 or
//! 3857, so features get reprojected to match the request bbox before the
//! core renderer maps world coordinates to pixels. bbox axis order is treated
//! as minx,miny,maxx,maxy (lon/lat) for both CRS.

use fenestra_core::renderer::RenderLayer;
use fenestra_core::{Feature, Geometry, Style, StyledLayerDescriptor};

/// Supported coordinate reference systems.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Crs {
    Geographic,
    WebMercator,
}

pub fn parse_crs(s: &str) -> Crs {
    let s = s.to_ascii_uppercase();
    if s.contains("3857") || s.contains("900913") || s.contains("102100") {
        Crs::WebMercator
    } else {
        Crs::Geographic
    }
}

const EARTH_RADIUS: f64 = 6_378_137.0;

pub fn lonlat_to_merc(lon: f64, lat: f64) -> [f64; 2] {
    let x = EARTH_RADIUS * lon.to_radians();
    let lat = lat.clamp(-89.99, 89.99);
    let y = EARTH_RADIUS
        * (std::f64::consts::FRAC_PI_4 + lat.to_radians() / 2.0)
            .tan()
            .ln();
    [x, y]
}

pub fn merc_to_lonlat(x: f64, y: f64) -> [f64; 2] {
    let lon = (x / EARTH_RADIUS).to_degrees();
    let lat = (2.0 * (y / EARTH_RADIUS).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
    [lon, lat]
}

/// Convert a request bbox to EPSG:4326 for filtering 4326 features.
pub fn bbox_to_4326(bbox: [f64; 4], crs: Crs) -> [f64; 4] {
    match crs {
        Crs::Geographic => bbox,
        Crs::WebMercator => {
            let min = merc_to_lonlat(bbox[0], bbox[1]);
            let max = merc_to_lonlat(bbox[2], bbox[3]);
            [min[0], min[1], max[0], max[1]]
        }
    }
}

fn project_point(c: [f64; 2], crs: Crs) -> [f64; 2] {
    match crs {
        Crs::Geographic => c,
        Crs::WebMercator => lonlat_to_merc(c[0], c[1]),
    }
}

fn project_geometry(geom: &Geometry, crs: Crs) -> Geometry {
    if crs == Crs::Geographic {
        return geom.clone();
    }
    let p = |c: &[f64; 2]| project_point(*c, crs);
    match geom {
        Geometry::Point { coordinates } => Geometry::Point {
            coordinates: p(coordinates),
        },
        Geometry::MultiPoint { coordinates } => Geometry::MultiPoint {
            coordinates: coordinates.iter().map(p).collect(),
        },
        Geometry::LineString { coordinates } => Geometry::LineString {
            coordinates: coordinates.iter().map(p).collect(),
        },
        Geometry::MultiLineString { coordinates } => Geometry::MultiLineString {
            coordinates: coordinates
                .iter()
                .map(|ls| ls.iter().map(p).collect())
                .collect(),
        },
        Geometry::Polygon { coordinates } => Geometry::Polygon {
            coordinates: coordinates
                .iter()
                .map(|ring| ring.iter().map(p).collect())
                .collect(),
        },
        Geometry::MultiPolygon { coordinates } => Geometry::MultiPolygon {
            coordinates: coordinates
                .iter()
                .map(|poly| {
                    poly.iter()
                        .map(|ring| ring.iter().map(p).collect())
                        .collect()
                })
                .collect(),
        },
    }
}

fn project_feature(f: &Feature, crs: Crs) -> Feature {
    Feature {
        feature_type: f.feature_type.clone(),
        id: f.id.clone(),
        geometry: f.geometry.as_ref().map(|g| project_geometry(g, crs)),
        properties: f.properties.clone(),
    }
}

/// Build a render layer, reprojecting features into the request CRS.
pub fn build_layer(name: &str, features: &[Feature], crs: Crs, style: Style) -> RenderLayer {
    RenderLayer {
        name: name.to_string(),
        features: features.iter().map(|f| project_feature(f, crs)).collect(),
        style,
    }
}

/// Resolve a style for a layer from a parsed SLD, or an empty style (which the
/// renderer draws with sensible defaults).
pub fn resolve_style(sld: Option<&StyledLayerDescriptor>, layer: &str) -> Style {
    if let Some(sld) = sld {
        let named = sld
            .named_layers
            .iter()
            .find(|l| l.name == layer)
            .or_else(|| sld.named_layers.first());
        if let Some(style) = named.and_then(|l| l.styles.first()) {
            return style.clone();
        }
    }
    Style {
        name: None,
        rules: Vec::new(),
    }
}
