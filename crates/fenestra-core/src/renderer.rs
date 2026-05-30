//! Map renderer — server-side rasterization of geospatial features to PNG.
//!
//! Supports two rendering backends:
//! - **CPU (default)**: `tiny-skia` — pure-Rust software rasterizer, works headless
//! - **GPU (feature `vello`)**: `jung-vello` — Vello + wgpu GPU-accelerated path rendering
//!
//! The GPU backend is preferred when available (10-100x faster for complex scenes).
//! Falls back to CPU when no GPU is present or for headless CI/server environments.

use crate::ogcapi::{Feature, Geometry};
use crate::sld::{Fill, Stroke, Style, Symbolizer};
use crate::wms::WmsGetMapRequest;

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

        for layer in layers {
            render_layer(&mut pixmap, layer, &transform);
        }

        encode_png(pixmap.width(), pixmap.height(), pixmap.data())
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
            Symbolizer::Text(_) => {}
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

        let builder = SceneBuilder::new(request.width, request.height, bbox);
        let jung_style = convert_style(layers);
        let jung_features = convert_features(layers);
        let scene = builder.build(&jung_style, &jung_features);

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

    /// Convert fenestra OGC features to jung-core geometry features.
    fn convert_features(layers: &[RenderLayer]) -> Vec<jung_core::geometry::Feature> {
        use jung_core::geometry::{Geometry as JungGeom, Point, PolygonGeom};

        layers
            .iter()
            .flat_map(|layer| &layer.features)
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

    /// Convert fenestra SLD styles to jung-style layers.
    fn convert_style(layers: &[RenderLayer]) -> jung_style::Style {
        use jung_style::{Color, Layer as JungLayer};

        let jung_layers: Vec<JungLayer> = layers
            .iter()
            .flat_map(|layer| {
                if layer.style.rules.is_empty() {
                    vec![make_layer(
                        Some(Color::rgba(100, 149, 237, 128)),
                        Some(Color::rgba(0, 0, 0, 255)),
                        Some(1.0),
                        Some(4.0),
                    )]
                } else {
                    layer
                        .style
                        .rules
                        .iter()
                        .flat_map(|rule| {
                            rule.symbolizers.iter().map(|sym| match sym {
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
                                    let width =
                                        ps.stroke.as_ref().and_then(|s| s.width).unwrap_or(1.0)
                                            as f32;
                                    make_layer(Some(fill), Some(stroke), Some(width), None)
                                }
                                Symbolizer::Text(_) => make_layer(None, None, None, None),
                            })
                        })
                        .collect()
                }
            })
            .collect();

        jung_style::Style {
            name: String::new(),
            layers: jung_layers,
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
}
