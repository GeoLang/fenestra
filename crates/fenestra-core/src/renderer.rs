//! Map renderer — server-side rasterization of geospatial features to PNG.
//!
//! Uses tiny-skia for CPU-based 2D rendering, producing WMS GetMap responses.

use crate::ogcapi::{Feature, Geometry};
use crate::sld::{Fill, Stroke, Style, Symbolizer};
use crate::wms::WmsGetMapRequest;

/// A map layer with features and styling.
pub struct RenderLayer {
    pub name: String,
    pub features: Vec<Feature>,
    pub style: Style,
}

/// Render features to a PNG image for WMS GetMap.
pub fn render_map(request: &WmsGetMapRequest, layers: &[RenderLayer]) -> Vec<u8> {
    let width = request.width;
    let height = request.height;
    let bbox = request.parse_bbox().unwrap_or([-180.0, -90.0, 180.0, 90.0]);

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .unwrap_or_else(|| tiny_skia::Pixmap::new(1, 1).unwrap());

    pixmap.fill(tiny_skia::Color::WHITE);

    let transform = MapTransform::new(width, height, &bbox);

    for layer in layers {
        render_layer(&mut pixmap, layer, &transform);
    }

    encode_png(pixmap.width(), pixmap.height(), pixmap.data())
}

/// Affine transform from world coordinates to pixel coordinates.
struct MapTransform {
    scale_x: f64,
    scale_y: f64,
    offset_x: f64,
    offset_y: f64,
}

impl MapTransform {
    fn new(width: u32, height: u32, bbox: &[f64; 4]) -> Self {
        let world_width = bbox[2] - bbox[0];
        let world_height = bbox[3] - bbox[1];
        Self {
            scale_x: width as f64 / world_width,
            scale_y: height as f64 / world_height,
            offset_x: bbox[0],
            offset_y: bbox[3],
        }
    }

    fn world_to_pixel(&self, x: f64, y: f64) -> (f32, f32) {
        let px = ((x - self.offset_x) * self.scale_x) as f32;
        let py = ((self.offset_y - y) * self.scale_y) as f32;
        (px, py)
    }
}

fn render_layer(pixmap: &mut tiny_skia::Pixmap, layer: &RenderLayer, transform: &MapTransform) {
    for rule in &layer.style.rules {
        for symbolizer in &rule.symbolizers {
            for feature in &layer.features {
                if let Some(geom) = &feature.geometry {
                    render_feature(pixmap, geom, symbolizer, transform);
                }
            }
        }
    }

    // If no rules, use defaults
    if layer.style.rules.is_empty() {
        let default_fill = tiny_skia::Color::from_rgba8(100, 149, 237, 128);
        let default_stroke = tiny_skia::Color::from_rgba8(0, 0, 0, 255);
        for feature in &layer.features {
            if let Some(geom) = &feature.geometry {
                render_feature_default(pixmap, geom, transform, default_fill, default_stroke);
            }
        }
    }
}

fn render_feature(
    pixmap: &mut tiny_skia::Pixmap,
    geom: &Geometry,
    symbolizer: &Symbolizer,
    transform: &MapTransform,
) {
    match symbolizer {
        Symbolizer::Point(ps) => {
            let size = ps.graphic.size.unwrap_or(8.0) as f32;
            let color = ps
                .graphic
                .mark
                .as_ref()
                .and_then(|m| m.fill.as_ref())
                .and_then(|f| f.color.as_deref())
                .map(parse_color)
                .unwrap_or(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
            render_points(pixmap, geom, transform, size, color);
        }
        Symbolizer::Line(ls) => {
            let color = stroke_color(&ls.stroke);
            let width = ls.stroke.width.unwrap_or(1.0) as f32;
            render_lines(pixmap, geom, transform, color, width);
        }
        Symbolizer::Polygon(ps) => {
            let fill = ps
                .fill
                .as_ref()
                .map(fill_color)
                .unwrap_or(tiny_skia::Color::from_rgba8(200, 200, 200, 128));
            let stroke = ps
                .stroke
                .as_ref()
                .map(stroke_color)
                .unwrap_or(tiny_skia::Color::BLACK);
            let width = ps.stroke.as_ref().and_then(|s| s.width).unwrap_or(1.0) as f32;
            render_polygons(pixmap, geom, transform, fill, stroke, width);
        }
        Symbolizer::Text(_) => {} // Text rendering requires font rasterization
    }
}

fn render_feature_default(
    pixmap: &mut tiny_skia::Pixmap,
    geom: &Geometry,
    transform: &MapTransform,
    fill: tiny_skia::Color,
    stroke: tiny_skia::Color,
) {
    match geom {
        Geometry::Point { .. } | Geometry::MultiPoint { .. } => {
            render_points(pixmap, geom, transform, 6.0, fill);
        }
        Geometry::LineString { .. } | Geometry::MultiLineString { .. } => {
            render_lines(pixmap, geom, transform, stroke, 1.5);
        }
        Geometry::Polygon { .. } | Geometry::MultiPolygon { .. } => {
            render_polygons(pixmap, geom, transform, fill, stroke, 1.0);
        }
    }
}

fn render_points(
    pixmap: &mut tiny_skia::Pixmap,
    geom: &Geometry,
    transform: &MapTransform,
    size: f32,
    color: tiny_skia::Color,
) {
    let coords = match geom {
        Geometry::Point { coordinates } => vec![*coordinates],
        Geometry::MultiPoint { coordinates } => coordinates.clone(),
        _ => return,
    };
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    for [x, y] in coords {
        let (px, py) = transform.world_to_pixel(x, y);
        if let Some(path) = {
            let mut pb = tiny_skia::PathBuilder::new();
            pb.push_circle(px, py, size / 2.0);
            pb.finish()
        } {
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
}

fn render_lines(
    pixmap: &mut tiny_skia::Pixmap,
    geom: &Geometry,
    transform: &MapTransform,
    color: tiny_skia::Color,
    width: f32,
) {
    let rings: Vec<&Vec<[f64; 2]>> = match geom {
        Geometry::LineString { coordinates } => vec![coordinates],
        Geometry::MultiLineString { coordinates } => coordinates.iter().collect(),
        _ => return,
    };
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(color);
    paint.anti_alias = true;
    let stroke = tiny_skia::Stroke {
        width,
        ..Default::default()
    };
    for ring in rings {
        if ring.len() < 2 {
            continue;
        }
        let mut pb = tiny_skia::PathBuilder::new();
        let (px, py) = transform.world_to_pixel(ring[0][0], ring[0][1]);
        pb.move_to(px, py);
        for coord in &ring[1..] {
            let (px, py) = transform.world_to_pixel(coord[0], coord[1]);
            pb.line_to(px, py);
        }
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(
                &path,
                &paint,
                &stroke,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
}

fn render_polygons(
    pixmap: &mut tiny_skia::Pixmap,
    geom: &Geometry,
    transform: &MapTransform,
    fill: tiny_skia::Color,
    stroke_col: tiny_skia::Color,
    stroke_width: f32,
) {
    let rings: Vec<&Vec<Vec<[f64; 2]>>> = match geom {
        Geometry::Polygon { coordinates } => vec![coordinates],
        Geometry::MultiPolygon { coordinates } => coordinates.iter().collect(),
        _ => return,
    };
    for polygon_rings in rings {
        if polygon_rings.is_empty() || polygon_rings[0].len() < 3 {
            continue;
        }
        let exterior = &polygon_rings[0];
        let mut pb = tiny_skia::PathBuilder::new();
        let (px, py) = transform.world_to_pixel(exterior[0][0], exterior[0][1]);
        pb.move_to(px, py);
        for coord in &exterior[1..] {
            let (px, py) = transform.world_to_pixel(coord[0], coord[1]);
            pb.line_to(px, py);
        }
        pb.close();
        if let Some(path) = pb.finish() {
            let mut paint = tiny_skia::Paint {
                anti_alias: true,
                ..Default::default()
            };
            // Fill
            paint.set_color(fill);
            pixmap.fill_path(
                &path,
                &paint,
                tiny_skia::FillRule::EvenOdd,
                tiny_skia::Transform::identity(),
                None,
            );
            // Stroke
            if stroke_width > 0.0 {
                paint.set_color(stroke_col);
                let s = tiny_skia::Stroke {
                    width: stroke_width,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &s, tiny_skia::Transform::identity(), None);
            }
        }
    }
}

fn fill_color(f: &Fill) -> tiny_skia::Color {
    f.color
        .as_deref()
        .map(parse_color)
        .unwrap_or(tiny_skia::Color::from_rgba8(200, 200, 200, 128))
}

fn stroke_color(s: &Stroke) -> tiny_skia::Color {
    s.color
        .as_deref()
        .map(parse_color)
        .unwrap_or(tiny_skia::Color::BLACK)
}

fn parse_color(hex: &str) -> tiny_skia::Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        let a = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16).unwrap_or(255)
        } else {
            255
        };
        tiny_skia::Color::from_rgba8(r, g, b, a)
    } else {
        tiny_skia::Color::BLACK
    }
}

fn encode_png(width: u32, height: u32, rgba_data: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("PNG header");
        writer.write_image_data(rgba_data).expect("PNG data");
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ogcapi::Feature;

    #[test]
    fn test_render_empty_map() {
        let request = WmsGetMapRequest {
            layers: "test".to_string(),
            styles: "".to_string(),
            crs: "EPSG:4326".to_string(),
            bbox: "-180,-90,180,90".to_string(),
            width: 256,
            height: 256,
            format: "image/png".to_string(),
        };
        let png = render_map(&request, &[]);
        assert!(!png.is_empty());
        assert_eq!(&png[0..4], &[137, 80, 78, 71]); // PNG magic
    }

    #[test]
    fn test_render_with_points() {
        let request = WmsGetMapRequest {
            layers: "points".to_string(),
            styles: "".to_string(),
            crs: "EPSG:4326".to_string(),
            bbox: "0,0,10,10".to_string(),
            width: 256,
            height: 256,
            format: "image/png".to_string(),
        };
        let layer = RenderLayer {
            name: "points".to_string(),
            features: vec![Feature::new(
                Some("1".to_string()),
                Geometry::Point {
                    coordinates: [5.0, 5.0],
                },
                serde_json::json!({}),
            )],
            style: Style {
                name: None,
                rules: vec![],
            },
        };
        let png = render_map(&request, &[layer]);
        assert!(png.len() > 100);
    }

    #[test]
    fn test_parse_color() {
        let c = parse_color("#ff0000");
        assert_eq!(c, tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let c2 = parse_color("#00ff0080");
        assert_eq!(c2, tiny_skia::Color::from_rgba8(0, 255, 0, 128));
    }
}
