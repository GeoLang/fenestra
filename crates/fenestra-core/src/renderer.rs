//! Map renderer — server-side rasterization of geospatial features to PNG.
//!
//! Supports two rendering backends:
//! - **CPU (default)**: `tiny-skia` — pure-Rust software rasterizer, works headless
//! - **GPU (feature `vello`)**: `jung-vello` — Vello + wgpu GPU-accelerated path rendering
//!
//! The GPU backend is preferred when available (10-100x faster for complex scenes).
//! Falls back to CPU when no GPU is present or for headless CI/server environments.

use crate::ogcapi::{Feature, Geometry};
use crate::sld::{ComparisonOp, Fill, Filter, Rule, Stroke, Style, Symbolizer, TextSymbolizer};
use crate::wms::WmsGetMapRequest;
use std::sync::OnceLock;

/// Bundled Caladea (Apache-2.0) so GetMap labels do not need a system font.
const LABEL_FONT: &[u8] = include_bytes!("../fonts/Caladea-Regular.ttf");

fn label_font() -> &'static fontdue::Font {
    static FONT: OnceLock<fontdue::Font> = OnceLock::new();
    FONT.get_or_init(|| {
        fontdue::Font::from_bytes(LABEL_FONT, fontdue::FontSettings::default())
            .expect("bundled Caladea is a valid font")
    })
}

/// A map layer with features and styling.
pub struct RenderLayer {
    pub name: String,
    pub features: Vec<Feature>,
    pub style: Style,
}

/// Rendering backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// CPU-based rendering via tiny-skia (default, always available).
    Cpu,
    /// GPU-accelerated rendering via Vello/wgpu (requires `vello` feature + GPU).
    Gpu,
}

impl Default for Backend {
    fn default() -> Self {
        if cfg!(feature = "vello") {
            Self::Gpu
        } else {
            Self::Cpu
        }
    }
}

/// Render features to a PNG image for WMS GetMap using the default backend.
pub fn render_map(request: &WmsGetMapRequest, layers: &[RenderLayer]) -> Vec<u8> {
    render_map_with_backend(request, layers, Backend::default())
}

/// Render features to a PNG image using a specific backend.
pub fn render_map_with_backend(
    request: &WmsGetMapRequest,
    layers: &[RenderLayer],
    backend: Backend,
) -> Vec<u8> {
    match backend {
        Backend::Cpu => cpu::render(request, layers),
        Backend::Gpu => {
            #[cfg(feature = "vello")]
            {
                gpu::render(request, layers)
            }
            #[cfg(not(feature = "vello"))]
            {
                // Fall back to CPU if vello feature not enabled
                cpu::render(request, layers)
            }
        }
    }
}

fn scale_denominator(bbox: &[f64; 4], width: u32) -> f64 {
    let map_width = (bbox[2] - bbox[0]).abs();
    map_width / (width as f64 * 0.000_28)
}

fn rule_applies_at_scale(rule: &Rule, denom: f64) -> bool {
    rule.min_scale.is_none_or(|min| denom >= min) && rule.max_scale.is_none_or(|max| denom < max)
}

fn property_text(feature: &Feature, property: &str) -> Option<String> {
    match feature.properties.get(property)? {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        other => Some(other.to_string()),
    }
}

fn compare_literals(left: &str, op: ComparisonOp, right: &str) -> bool {
    let ord = match (left.parse::<f64>(), right.parse::<f64>()) {
        (Ok(l), Ok(r)) => l.partial_cmp(&r),
        _ => Some(left.cmp(right)),
    };
    match ord {
        Some(std::cmp::Ordering::Equal) => matches!(
            op,
            ComparisonOp::EqualTo
                | ComparisonOp::LessThanOrEqualTo
                | ComparisonOp::GreaterThanOrEqualTo
        ),
        Some(std::cmp::Ordering::Less) => matches!(
            op,
            ComparisonOp::NotEqualTo | ComparisonOp::LessThan | ComparisonOp::LessThanOrEqualTo
        ),
        Some(std::cmp::Ordering::Greater) => matches!(
            op,
            ComparisonOp::NotEqualTo
                | ComparisonOp::GreaterThan
                | ComparisonOp::GreaterThanOrEqualTo
        ),
        None => false,
    }
}

fn feature_matches_filter(feature: &Feature, filter: &Filter, already_matched: bool) -> bool {
    match filter {
        Filter::Else => !already_matched,
        Filter::Unsupported(_) => false,
        Filter::Comparison {
            property,
            op,
            value,
        } => property_text(feature, property).is_some_and(|got| compare_literals(&got, *op, value)),
        Filter::Between {
            property,
            lower,
            upper,
        } => property_text(feature, property).is_some_and(|got| {
            match (
                got.parse::<f64>(),
                lower.parse::<f64>(),
                upper.parse::<f64>(),
            ) {
                (Ok(v), Ok(lo), Ok(hi)) => v >= lo && v <= hi,
                _ => got.as_str() >= lower.as_str() && got.as_str() <= upper.as_str(),
            }
        }),
    }
}

fn matching_rule<'a>(feature: &Feature, rules: &'a [Rule], denom: f64) -> Option<&'a Rule> {
    matching_rule_index(feature, rules, denom).map(|i| &rules[i])
}

fn matching_rule_index(feature: &Feature, rules: &[Rule], denom: f64) -> Option<usize> {
    let mut else_idx = None;
    for (i, rule) in rules.iter().enumerate() {
        if !rule_applies_at_scale(rule, denom) {
            continue;
        }
        match rule.filter.as_ref() {
            Some(Filter::Else) => {
                if else_idx.is_none() && feature_matches_filter(feature, &Filter::Else, false) {
                    else_idx = Some(i);
                }
            }
            Some(filter) => {
                if feature_matches_filter(feature, filter, false) {
                    return Some(i);
                }
            }
            None => return Some(i),
        }
    }
    else_idx
}

// ─── CPU Backend (tiny-skia) ─────────────────────────────────────────────────

mod cpu {
    use super::*;

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

    pub(super) fn render(request: &WmsGetMapRequest, layers: &[RenderLayer]) -> Vec<u8> {
        let width = request.width;
        let height = request.height;
        let bbox = request.parse_bbox().unwrap_or([-180.0, -90.0, 180.0, 90.0]);

        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .unwrap_or_else(|| tiny_skia::Pixmap::new(1, 1).unwrap());

        pixmap.fill(tiny_skia::Color::WHITE);

        let transform = MapTransform::new(width, height, &bbox);
        let denom = scale_denominator(&bbox, width);

        for layer in layers {
            render_layer(&mut pixmap, layer, &transform, denom);
        }

        encode_png(pixmap.width(), pixmap.height(), pixmap.data())
    }

    fn render_layer(
        pixmap: &mut tiny_skia::Pixmap,
        layer: &RenderLayer,
        transform: &MapTransform,
        denom: f64,
    ) {
        if layer.style.rules.is_empty() {
            let default_fill = tiny_skia::Color::from_rgba8(100, 149, 237, 128);
            let default_stroke = tiny_skia::Color::from_rgba8(0, 0, 0, 255);
            for feature in &layer.features {
                if let Some(geom) = &feature.geometry {
                    render_feature_default(pixmap, geom, transform, default_fill, default_stroke);
                }
            }
            return;
        }

        for feature in &layer.features {
            let Some(geom) = &feature.geometry else {
                continue;
            };
            let Some(rule) = matching_rule(feature, &layer.style.rules, denom) else {
                continue;
            };
            for symbolizer in &rule.symbolizers {
                render_feature(pixmap, feature, geom, symbolizer, transform);
            }
        }
    }

    fn render_feature(
        pixmap: &mut tiny_skia::Pixmap,
        feature: &Feature,
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
            Symbolizer::Text(ts) => {
                render_text(pixmap, feature, geom, ts, transform);
            }
        }
    }

    fn render_text(
        pixmap: &mut tiny_skia::Pixmap,
        feature: &Feature,
        geom: &Geometry,
        ts: &TextSymbolizer,
        transform: &MapTransform,
    ) {
        let Some(text) = property_text(feature, &ts.label_property) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let Some((wx, wy)) = label_anchor(geom) else {
            return;
        };
        let size = ts.font_size.unwrap_or(10.0) as f32;
        if size <= 0.0 {
            return;
        }
        let color = ts
            .fill
            .as_ref()
            .map(fill_color)
            .unwrap_or(tiny_skia::Color::BLACK);
        let (px, py) = transform.world_to_pixel(wx, wy);
        blit_label(pixmap, &text, px, py, size, color);
    }

    fn label_anchor(geom: &Geometry) -> Option<(f64, f64)> {
        match geom {
            Geometry::Point { coordinates } => Some((coordinates[0], coordinates[1])),
            Geometry::MultiPoint { coordinates } => coordinates.first().map(|c| (c[0], c[1])),
            Geometry::LineString { coordinates } => mean_coord(coordinates),
            Geometry::MultiLineString { coordinates } => {
                coordinates.first().and_then(|line| mean_coord(line))
            }
            Geometry::Polygon { coordinates } => {
                coordinates.first().and_then(|ring| mean_coord(ring))
            }
            Geometry::MultiPolygon { coordinates } => coordinates
                .first()
                .and_then(|poly| poly.first().and_then(|ring| mean_coord(ring))),
        }
    }

    fn mean_coord(coords: &[[f64; 2]]) -> Option<(f64, f64)> {
        let n = coords.len();
        if n == 0 {
            return None;
        }
        let end = if n >= 2 && coords[0] == coords[n - 1] {
            n - 1
        } else {
            n
        };
        if end == 0 {
            return None;
        }
        let sx: f64 = coords[..end].iter().map(|c| c[0]).sum();
        let sy: f64 = coords[..end].iter().map(|c| c[1]).sum();
        Some((sx / end as f64, sy / end as f64))
    }

    fn blit_label(
        pixmap: &mut tiny_skia::Pixmap,
        text: &str,
        px: f32,
        py: f32,
        size: f32,
        color: tiny_skia::Color,
    ) {
        let font = super::label_font();
        let width: f32 = text
            .chars()
            .map(|ch| font.metrics(ch, size).advance_width)
            .sum();
        let mut pen_x = px - width / 2.0;
        let baseline = py + size * 0.35;
        for ch in text.chars() {
            let (metrics, bitmap) = font.rasterize(ch, size);
            let origin_x = (pen_x + metrics.xmin as f32).round() as i32;
            let origin_y = (baseline - metrics.height as f32 - metrics.ymin as f32).round() as i32;
            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let coverage = bitmap[row * metrics.width + col];
                    blend_coverage(
                        pixmap,
                        origin_x + col as i32,
                        origin_y + row as i32,
                        color,
                        coverage,
                    );
                }
            }
            pen_x += metrics.advance_width;
        }
    }

    fn blend_coverage(
        pixmap: &mut tiny_skia::Pixmap,
        x: i32,
        y: i32,
        color: tiny_skia::Color,
        coverage: u8,
    ) {
        if x < 0 || y < 0 {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        if x >= pixmap.width() || y >= pixmap.height() {
            return;
        }
        let src = color.to_color_u8();
        let src_a = (u16::from(src.alpha()) * u16::from(coverage) / 255) as u8;
        if src_a == 0 {
            return;
        }
        let i = (y * pixmap.width() + x) as usize;
        let pixels = pixmap.pixels_mut();
        let dst = pixels[i];
        let inv = 255 - u16::from(src_a);
        let pr = u16::from(src.red()) * u16::from(src_a) / 255;
        let pg = u16::from(src.green()) * u16::from(src_a) / 255;
        let pb = u16::from(src.blue()) * u16::from(src_a) / 255;
        let out_a = (u16::from(src_a) + u16::from(dst.alpha()) * inv / 255) as u8;
        let out_r = ((pr + u16::from(dst.red()) * inv / 255) as u8).min(out_a);
        let out_g = ((pg + u16::from(dst.green()) * inv / 255) as u8).min(out_a);
        let out_b = ((pb + u16::from(dst.blue()) * inv / 255) as u8).min(out_a);
        if let Some(pixel) = tiny_skia::PremultipliedColorU8::from_rgba(out_r, out_g, out_b, out_a)
        {
            pixels[i] = pixel;
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
                paint.set_color(fill);
                pixmap.fill_path(
                    &path,
                    &paint,
                    tiny_skia::FillRule::EvenOdd,
                    tiny_skia::Transform::identity(),
                    None,
                );
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

    pub(super) fn parse_color(hex: &str) -> tiny_skia::Color {
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

    pub(super) fn encode_png(width: u32, height: u32, rgba_data: &[u8]) -> Vec<u8> {
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
}

// ─── GPU Backend (Vello via jung-vello) ──────────────────────────────────────

#[cfg(feature = "vello")]
mod gpu {
    use super::*;
    use std::collections::HashMap;

    pub(super) fn render(request: &WmsGetMapRequest, layers: &[RenderLayer]) -> Vec<u8> {
        match try_render_gpu(request, layers) {
            Ok(png) => png,
            Err(_) => super::cpu::render(request, layers),
        }
    }

    fn try_render_gpu(
        request: &WmsGetMapRequest,
        layers: &[RenderLayer],
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        use jung_core::renderer::BBox;
        use jung_vello::SceneBuilder;
        use vello::wgpu;

        let bbox_arr = request.parse_bbox().unwrap_or([-180.0, -90.0, 180.0, 90.0]);
        let bbox = BBox {
            min_x: bbox_arr[0],
            min_y: bbox_arr[1],
            max_x: bbox_arr[2],
            max_y: bbox_arr[3],
        };
        let scale_denom = scale_denominator(&bbox_arr, request.width);

        let builder = SceneBuilder::new(request.width, request.height, bbox);
        let mut scene = vello::Scene::new();
        for layer in layers {
            paint_gpu_layer(&builder, &mut scene, layer, scale_denom);
        }

        let params = vello::RenderParams {
            base_color: vello::peniko::Color::WHITE,
            width: request.width,
            height: request.height,
            antialiasing_method: vello::AaConfig::Msaa16,
        };

        let (device, queue) = pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .map_err(|e| format!("adapter error: {e}"))?;
            adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .map_err(|e| format!("device error: {e}"))
        })?;

        let mut renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::all(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map_err(|e| format!("vello renderer error: {e}"))?;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("wms_render"),
            size: wgpu::Extent3d {
                width: request.width,
                height: request.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        renderer
            .render_to_texture(&device, &queue, &scene, &view, &params)
            .map_err(|e| format!("render error: {e}"))?;

        // Read pixels back from GPU
        let buffer_size = (request.width * request.height * 4) as u64;
        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("wms_output"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * request.width),
                    rows_per_image: Some(request.height),
                },
            },
            wgpu::Extent3d {
                width: request.width,
                height: request.height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        rx.recv()??;

        let data = buffer_slice.get_mapped_range();
        let pixels = data.to_vec();
        drop(data);
        output_buffer.unmap();

        Ok(super::cpu::encode_png(
            request.width,
            request.height,
            &pixels,
        ))
    }

    fn paint_gpu_layer(
        builder: &jung_vello::SceneBuilder,
        scene: &mut vello::Scene,
        layer: &RenderLayer,
        scale_denom: f64,
    ) {
        if layer.style.rules.is_empty() {
            use jung_style::Color;
            let jung_layer = make_layer(
                Some(Color::rgba(100, 149, 237, 128)),
                Some(Color::rgba(0, 0, 0, 255)),
                Some(1.0),
                Some(4.0),
            );
            let jung_features = convert_features(&layer.features);
            let part = builder.build_layer(&jung_layer, &jung_features);
            scene.append(&part, None);
            return;
        }

        let mut buckets: Vec<Vec<&Feature>> = vec![Vec::new(); layer.style.rules.len()];
        for feature in &layer.features {
            if let Some(i) = matching_rule_index(feature, &layer.style.rules, scale_denom) {
                buckets[i].push(feature);
            }
        }
        for (rule, feats) in layer.style.rules.iter().zip(buckets) {
            if feats.is_empty() {
                continue;
            }
            let jung_features = convert_features(feats);
            for symbolizer in &rule.symbolizers {
                let jung_layer = jung_layer_for_symbolizer(symbolizer);
                let part = builder.build_layer(&jung_layer, &jung_features);
                scene.append(&part, None);
            }
        }
    }

    /// Convert fenestra OGC features to jung-core geometry features.
    fn convert_features<'a>(
        features: impl IntoIterator<Item = &'a Feature>,
    ) -> Vec<jung_core::geometry::Feature> {
        use jung_core::geometry::{Geometry as JungGeom, Point, PolygonGeom};

        features
            .into_iter()
            .filter_map(|f| {
                let geom = f.geometry.as_ref()?;
                let jung_geom = match geom {
                    Geometry::Point { coordinates } => JungGeom::Point(Point {
                        x: coordinates[0],
                        y: coordinates[1],
                    }),
                    Geometry::MultiPoint { coordinates } => JungGeom::MultiPoint(
                        coordinates
                            .iter()
                            .map(|c| Point { x: c[0], y: c[1] })
                            .collect(),
                    ),
                    Geometry::LineString { coordinates } => JungGeom::LineString(
                        coordinates
                            .iter()
                            .map(|c| Point { x: c[0], y: c[1] })
                            .collect(),
                    ),
                    Geometry::MultiLineString { coordinates } => JungGeom::MultiLineString(
                        coordinates
                            .iter()
                            .map(|ring| ring.iter().map(|c| Point { x: c[0], y: c[1] }).collect())
                            .collect(),
                    ),
                    Geometry::Polygon { coordinates } => {
                        let exterior = coordinates
                            .first()
                            .map(|ring| ring.iter().map(|c| Point { x: c[0], y: c[1] }).collect())
                            .unwrap_or_default();
                        let holes: Vec<Vec<Point>> = coordinates
                            .iter()
                            .skip(1)
                            .map(|ring| ring.iter().map(|c| Point { x: c[0], y: c[1] }).collect())
                            .collect();
                        JungGeom::Polygon { exterior, holes }
                    }
                    Geometry::MultiPolygon { coordinates } => {
                        let polys = coordinates
                            .iter()
                            .map(|polygon_rings| {
                                let exterior = polygon_rings
                                    .first()
                                    .map(|ring| {
                                        ring.iter().map(|c| Point { x: c[0], y: c[1] }).collect()
                                    })
                                    .unwrap_or_default();
                                let holes: Vec<Vec<Point>> = polygon_rings
                                    .iter()
                                    .skip(1)
                                    .map(|ring| {
                                        ring.iter().map(|c| Point { x: c[0], y: c[1] }).collect()
                                    })
                                    .collect();
                                PolygonGeom { exterior, holes }
                            })
                            .collect();
                        JungGeom::MultiPolygon(polys)
                    }
                };
                // Convert serde_json properties to jung PropertyValue
                let properties = convert_properties(&f.properties);
                Some(jung_core::geometry::Feature {
                    geometry: jung_geom,
                    properties,
                })
            })
            .collect()
    }

    /// Convert serde_json::Value properties to jung PropertyValue map.
    fn convert_properties(value: &serde_json::Value) -> HashMap<String, jung_style::PropertyValue> {
        use jung_style::PropertyValue;

        let mut map = HashMap::new();
        if let Some(obj) = value.as_object() {
            for (k, v) in obj {
                let pv = match v {
                    serde_json::Value::String(s) => PropertyValue::String(s.clone()),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            PropertyValue::Integer(i)
                        } else {
                            PropertyValue::Number(n.as_f64().unwrap_or(0.0))
                        }
                    }
                    serde_json::Value::Bool(b) => PropertyValue::Boolean(*b),
                    _ => PropertyValue::Null,
                };
                map.insert(k.clone(), pv);
            }
        }
        map
    }

    fn jung_layer_for_symbolizer(sym: &Symbolizer) -> jung_style::Layer {
        use jung_style::{Color, StyleValue};

        match sym {
            Symbolizer::Point(ps) => {
                let size = ps.graphic.size.unwrap_or(8.0);
                let color = ps
                    .graphic
                    .mark
                    .as_ref()
                    .and_then(|m| m.fill.as_ref())
                    .and_then(|f| f.color.as_deref())
                    .map(parse_hex_to_jung_color)
                    .unwrap_or(Color::rgba(255, 0, 0, 255));
                make_layer(Some(color), None, None, Some(size as f32 / 2.0))
            }
            Symbolizer::Line(ls) => {
                let color = ls
                    .stroke
                    .color
                    .as_deref()
                    .map(parse_hex_to_jung_color)
                    .unwrap_or(Color::rgba(0, 0, 0, 255));
                let width = ls.stroke.width.unwrap_or(1.0) as f32;
                make_layer(None, Some(color), Some(width), None)
            }
            Symbolizer::Polygon(ps) => {
                let fill = ps
                    .fill
                    .as_ref()
                    .and_then(|f| f.color.as_deref())
                    .map(parse_hex_to_jung_color)
                    .unwrap_or(Color::rgba(200, 200, 200, 128));
                let stroke = ps
                    .stroke
                    .as_ref()
                    .and_then(|s| s.color.as_deref())
                    .map(parse_hex_to_jung_color)
                    .unwrap_or(Color::rgba(0, 0, 0, 255));
                let width = ps.stroke.as_ref().and_then(|s| s.width).unwrap_or(1.0) as f32;
                make_layer(Some(fill), Some(stroke), Some(width), None)
            }
            Symbolizer::Text(ts) => {
                let color = ts
                    .fill
                    .as_ref()
                    .and_then(|f| f.color.as_deref())
                    .map(parse_hex_to_jung_color)
                    .unwrap_or(Color::rgba(0, 0, 0, 255));
                let mut layer = make_layer(None, None, None, None);
                layer.text_field = Some(StyleValue::Literal(format!("{{{}}}", ts.label_property)));
                layer.font_size = Some(StyleValue::Literal(ts.font_size.unwrap_or(10.0) as f32));
                layer.font_family = ts.font_family.clone();
                layer.text_color = Some(StyleValue::Literal(color));
                layer
            }
        }
    }

    fn make_layer(
        fill: Option<jung_style::Color>,
        stroke: Option<jung_style::Color>,
        stroke_width: Option<f32>,
        point_radius: Option<f32>,
    ) -> jung_style::Layer {
        use jung_style::{LineCap, LineJoin, StyleValue};

        jung_style::Layer {
            id: String::new(),
            source: None,
            fill_color: fill.map(StyleValue::Literal),
            stroke_color: stroke.map(StyleValue::Literal),
            stroke_width: stroke_width.map(StyleValue::Literal),
            line_cap: LineCap::default(),
            line_join: LineJoin::default(),
            line_dasharray: None,
            line_offset: None,
            line_opacity: None,
            point_radius: point_radius.map(StyleValue::Literal),
            icon_image: None,
            icon_size: None,
            font_family: None,
            font_size: None,
            text_field: None,
            text_color: None,
        }
    }

    fn parse_hex_to_jung_color(hex: &str) -> jung_style::Color {
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
            jung_style::Color { r, g, b, a }
        } else {
            jung_style::Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }
        }
    }
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
        let c = cpu::parse_color("#ff0000");
        assert_eq!(c, tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let c2 = cpu::parse_color("#00ff0080");
        assert_eq!(c2, tiny_skia::Color::from_rgba8(0, 255, 0, 128));
    }

    #[test]
    fn test_backend_selection() {
        // Without vello feature, GPU should still work (falls back to CPU)
        let request = WmsGetMapRequest {
            layers: "test".to_string(),
            styles: "".to_string(),
            crs: "EPSG:4326".to_string(),
            bbox: "-180,-90,180,90".to_string(),
            width: 64,
            height: 64,
            format: "image/png".to_string(),
        };
        let png = render_map_with_backend(&request, &[], Backend::Gpu);
        assert!(!png.is_empty());
    }

    fn typed_feature(id: &str, r#type: &str, geom: Geometry) -> Feature {
        Feature::new(
            Some(id.to_string()),
            geom,
            serde_json::json!({"type": r#type}),
        )
    }

    fn square(minx: f64, miny: f64, maxx: f64, maxy: f64) -> Geometry {
        Geometry::Polygon {
            coordinates: vec![vec![
                [minx, miny],
                [maxx, miny],
                [maxx, maxy],
                [minx, maxy],
                [minx, miny],
            ]],
        }
    }

    fn eq_type(value: &str) -> Filter {
        Filter::Comparison {
            property: "type".into(),
            op: ComparisonOp::EqualTo,
            value: value.into(),
        }
    }

    fn fill_rule(filter: Option<Filter>, color: &str, min_scale: Option<f64>) -> Rule {
        Rule {
            name: None,
            filter,
            min_scale,
            max_scale: None,
            symbolizers: vec![Symbolizer::Polygon(crate::sld::PolygonSymbolizer {
                fill: Some(Fill {
                    color: Some(color.into()),
                    opacity: None,
                }),
                stroke: Some(Stroke {
                    color: Some(color.into()),
                    width: Some(0.0),
                    opacity: None,
                    dash_array: None,
                }),
            })],
        }
    }

    fn map_request(bbox: &str, width: u32, height: u32) -> WmsGetMapRequest {
        WmsGetMapRequest {
            layers: "test".to_string(),
            styles: "".to_string(),
            crs: "EPSG:4326".to_string(),
            bbox: bbox.to_string(),
            width,
            height,
            format: "image/png".to_string(),
        }
    }

    fn rgba_at(png: &[u8], x: u32, y: u32) -> [u8; 4] {
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let i = (y * info.width + x) as usize * 4;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn equal_to_filter_matches_property() {
        let a = typed_feature("1", "A", square(0.0, 0.0, 1.0, 1.0));
        let b = typed_feature("2", "B", square(1.0, 0.0, 2.0, 1.0));
        let filter_a = eq_type("A");
        let filter_b = eq_type("B");
        assert!(feature_matches_filter(&a, &filter_a, false));
        assert!(!feature_matches_filter(&a, &filter_b, false));
        assert!(feature_matches_filter(&b, &filter_b, false));
        assert!(!feature_matches_filter(&b, &filter_a, false));
    }

    #[test]
    fn else_filter_only_when_unmatched() {
        let a = typed_feature("1", "A", square(0.0, 0.0, 1.0, 1.0));
        assert!(feature_matches_filter(&a, &Filter::Else, false));
        assert!(!feature_matches_filter(&a, &Filter::Else, true));
    }

    #[test]
    fn unsupported_filter_matches_nothing() {
        let a = typed_feature("1", "A", square(0.0, 0.0, 1.0, 1.0));
        assert!(!feature_matches_filter(
            &a,
            &Filter::Unsupported("PropertyIsLike".into()),
            false
        ));
    }

    #[test]
    fn between_filter_is_inclusive() {
        let feature = Feature::new(
            Some("1".into()),
            square(0.0, 0.0, 1.0, 1.0),
            serde_json::json!({"pop": 500}),
        );
        let filter = Filter::Between {
            property: "pop".into(),
            lower: "0".into(),
            upper: "999".into(),
        };
        assert!(feature_matches_filter(&feature, &filter, false));
        let high = Feature::new(
            Some("2".into()),
            square(0.0, 0.0, 1.0, 1.0),
            serde_json::json!({"pop": 1000}),
        );
        assert!(!feature_matches_filter(&high, &filter, false));
    }

    #[test]
    fn first_equal_to_rule_claims_feature() {
        let a = typed_feature("1", "A", square(0.0, 0.0, 1.0, 1.0));
        let b = typed_feature("2", "B", square(1.0, 0.0, 2.0, 1.0));
        let rules = vec![
            fill_rule(Some(eq_type("A")), "#FF0000", None),
            fill_rule(Some(eq_type("B")), "#0000FF", None),
        ];
        assert_eq!(matching_rule_index(&a, &rules, 1.0), Some(0));
        assert_eq!(matching_rule_index(&b, &rules, 1.0), Some(1));
    }

    #[test]
    fn else_rule_catches_unmatched_features() {
        let a = typed_feature("1", "A", square(0.0, 0.0, 1.0, 1.0));
        let b = typed_feature("2", "B", square(1.0, 0.0, 2.0, 1.0));
        let rules = vec![
            fill_rule(Some(eq_type("A")), "#FF0000", None),
            fill_rule(Some(Filter::Else), "#00FF00", None),
        ];
        assert_eq!(matching_rule_index(&a, &rules, 1.0), Some(0));
        assert_eq!(matching_rule_index(&b, &rules, 1.0), Some(1));
    }

    #[test]
    fn min_scale_skips_rule() {
        let a = typed_feature("1", "A", square(0.0, 0.0, 1.0, 1.0));
        let rules = vec![fill_rule(Some(eq_type("A")), "#FF0000", Some(10_000.0))];
        assert_eq!(matching_rule_index(&a, &rules, 139.0), None);
        assert_eq!(matching_rule_index(&a, &rules, 10_000.0), Some(0));
        assert_eq!(matching_rule_index(&a, &rules, 9_999.0), None);
    }

    #[test]
    fn max_scale_is_exclusive() {
        let a = typed_feature("1", "A", square(0.0, 0.0, 1.0, 1.0));
        let mut rule = fill_rule(Some(eq_type("A")), "#FF0000", None);
        rule.max_scale = Some(500.0);
        let rules = vec![rule];
        assert_eq!(matching_rule_index(&a, &rules, 499.0), Some(0));
        assert_eq!(matching_rule_index(&a, &rules, 500.0), None);
    }

    #[test]
    fn scale_denominator_uses_ogc_pixel_size() {
        let denom = scale_denominator(&[0.0, 0.0, 256.0 * 0.000_28, 1.0], 256);
        assert!((denom - 1.0).abs() < 1e-9);
    }

    #[test]
    fn render_map_applies_categorized_equal_to_filters() {
        let request = map_request("0,0,20,10", 200, 100);
        let layer = RenderLayer {
            name: "landuse".to_string(),
            features: vec![
                typed_feature("1", "A", square(0.0, 0.0, 10.0, 10.0)),
                typed_feature("2", "B", square(10.0, 0.0, 20.0, 10.0)),
            ],
            style: Style {
                name: Some("by-type".into()),
                rules: vec![
                    fill_rule(Some(eq_type("A")), "#FF0000", None),
                    fill_rule(Some(eq_type("B")), "#0000FF", None),
                ],
            },
        };
        let png = render_map(&request, &[layer]);
        assert_eq!(rgba_at(&png, 50, 50), [255, 0, 0, 255]);
        assert_eq!(rgba_at(&png, 150, 50), [0, 0, 255, 255]);
    }

    #[test]
    fn render_map_else_filter_draws_unmatched_features() {
        let request = map_request("0,0,20,10", 200, 100);
        let layer = RenderLayer {
            name: "landuse".to_string(),
            features: vec![
                typed_feature("1", "A", square(0.0, 0.0, 10.0, 10.0)),
                typed_feature("2", "B", square(10.0, 0.0, 20.0, 10.0)),
            ],
            style: Style {
                name: None,
                rules: vec![
                    fill_rule(Some(eq_type("A")), "#FF0000", None),
                    fill_rule(Some(Filter::Else), "#00FF00", None),
                ],
            },
        };
        let png = render_map(&request, &[layer]);
        assert_eq!(rgba_at(&png, 50, 50), [255, 0, 0, 255]);
        assert_eq!(rgba_at(&png, 150, 50), [0, 255, 0, 255]);
    }

    #[test]
    fn render_map_skips_rule_below_min_scale() {
        let request = map_request("0,0,10,10", 200, 100);
        let denom = scale_denominator(&[0.0, 0.0, 10.0, 10.0], 200);
        assert!(denom < 10_000.0);
        let layer = RenderLayer {
            name: "landuse".to_string(),
            features: vec![typed_feature("1", "A", square(0.0, 0.0, 10.0, 10.0))],
            style: Style {
                name: None,
                rules: vec![fill_rule(Some(eq_type("A")), "#FF0000", Some(10_000.0))],
            },
        };
        let png = render_map(&request, &[layer]);
        assert_eq!(rgba_at(&png, 100, 50), [255, 255, 255, 255]);
    }

    #[test]
    fn render_map_unsupported_filter_does_not_draw() {
        let request = map_request("0,0,10,10", 200, 100);
        let layer = RenderLayer {
            name: "landuse".to_string(),
            features: vec![typed_feature("1", "A", square(0.0, 0.0, 10.0, 10.0))],
            style: Style {
                name: None,
                rules: vec![fill_rule(
                    Some(Filter::Unsupported("PropertyIsLike".into())),
                    "#FF0000",
                    None,
                )],
            },
        };
        let png = render_map(&request, &[layer]);
        assert_eq!(rgba_at(&png, 100, 50), [255, 255, 255, 255]);
    }

    #[test]
    fn render_map_parsed_categorized_sld() {
        let sld_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<StyledLayerDescriptor version="1.0.0" xmlns:ogc="http://www.opengis.net/ogc">
  <NamedLayer>
    <Name>landuse</Name>
    <UserStyle>
      <Name>by-type</Name>
      <Rule>
        <ogc:Filter>
          <ogc:PropertyIsEqualTo>
            <ogc:PropertyName>type</ogc:PropertyName>
            <ogc:Literal>forest</ogc:Literal>
          </ogc:PropertyIsEqualTo>
        </ogc:Filter>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#FF0000</CssParameter></Fill>
          <Stroke><CssParameter name="stroke-width">0</CssParameter></Stroke>
        </PolygonSymbolizer>
      </Rule>
      <Rule>
        <ogc:Filter>
          <ogc:PropertyIsEqualTo>
            <ogc:PropertyName>type</ogc:PropertyName>
            <ogc:Literal>water</ogc:Literal>
          </ogc:PropertyIsEqualTo>
        </ogc:Filter>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#0000FF</CssParameter></Fill>
          <Stroke><CssParameter name="stroke-width">0</CssParameter></Stroke>
        </PolygonSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;
        let sld = crate::sld::parse_sld(sld_xml).unwrap();
        let style = sld.named_layers[0].styles[0].clone();
        let request = map_request("0,0,20,10", 200, 100);
        let layer = RenderLayer {
            name: "landuse".to_string(),
            features: vec![
                typed_feature("1", "forest", square(0.0, 0.0, 10.0, 10.0)),
                typed_feature("2", "water", square(10.0, 0.0, 20.0, 10.0)),
            ],
            style,
        };
        let png = render_map(&request, &[layer]);
        assert_eq!(rgba_at(&png, 50, 50), [255, 0, 0, 255]);
        assert_eq!(rgba_at(&png, 150, 50), [0, 0, 255, 255]);
    }

    fn png_has_rgba(png: &[u8], want: [u8; 4]) -> bool {
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        buf.as_chunks::<4>()
            .0
            .iter()
            .take((info.width * info.height) as usize)
            .any(|pixel| *pixel == want)
    }

    fn line_rule(symbolizers: Vec<Symbolizer>) -> Rule {
        Rule {
            name: None,
            filter: None,
            min_scale: None,
            max_scale: None,
            symbolizers,
        }
    }

    fn line_stroke(color: &str, width: f64) -> Symbolizer {
        Symbolizer::Line(crate::sld::LineSymbolizer {
            stroke: Stroke {
                color: Some(color.into()),
                width: Some(width),
                opacity: None,
                dash_array: None,
            },
        })
    }

    #[test]
    fn later_same_type_symbolizer_paints_on_top() {
        let request = map_request("0,0,10,10", 100, 100);
        let layer = RenderLayer {
            name: "roads".to_string(),
            features: vec![Feature::new(
                Some("1".into()),
                Geometry::LineString {
                    coordinates: vec![[0.0, 5.0], [10.0, 5.0]],
                },
                serde_json::json!({}),
            )],
            style: Style {
                name: None,
                rules: vec![line_rule(vec![
                    line_stroke("#FF0000", 20.0),
                    line_stroke("#0000FF", 20.0),
                ])],
            },
        };
        let png = render_map(&request, &[layer]);
        assert_eq!(
            rgba_at(&png, 50, 50),
            [0, 0, 255, 255],
            "the later line symbolizer should cover the earlier one"
        );
    }

    #[test]
    fn text_symbolizer_draws_label_from_property() {
        let request = map_request("0,0,10,10", 200, 200);
        let layer = RenderLayer {
            name: "places".to_string(),
            features: vec![Feature::new(
                Some("1".into()),
                Geometry::Point {
                    coordinates: [5.0, 5.0],
                },
                serde_json::json!({"name": "X"}),
            )],
            style: Style {
                name: None,
                rules: vec![line_rule(vec![Symbolizer::Text(
                    crate::sld::TextSymbolizer {
                        label_property: "name".into(),
                        font_family: None,
                        font_size: Some(64.0),
                        fill: Some(Fill {
                            color: Some("#FF0000".into()),
                            opacity: None,
                        }),
                    },
                )])],
            },
        };
        let png = render_map(&request, &[layer]);
        assert!(
            png_has_rgba(&png, [255, 0, 0, 255]),
            "a TextSymbolizer should paint the property as red label pixels"
        );
        assert_eq!(rgba_at(&png, 2, 2), [255, 255, 255, 255]);
    }

    #[test]
    fn text_symbolizer_skips_missing_property() {
        let request = map_request("0,0,10,10", 80, 80);
        let layer = RenderLayer {
            name: "places".to_string(),
            features: vec![Feature::new(
                Some("1".into()),
                Geometry::Point {
                    coordinates: [5.0, 5.0],
                },
                serde_json::json!({"other": "X"}),
            )],
            style: Style {
                name: None,
                rules: vec![line_rule(vec![Symbolizer::Text(
                    crate::sld::TextSymbolizer {
                        label_property: "name".into(),
                        font_family: None,
                        font_size: Some(32.0),
                        fill: Some(Fill {
                            color: Some("#FF0000".into()),
                            opacity: None,
                        }),
                    },
                )])],
            },
        };
        let png = render_map(&request, &[layer]);
        assert!(
            !png_has_rgba(&png, [255, 0, 0, 255]),
            "no label when the property is absent"
        );
    }
}
