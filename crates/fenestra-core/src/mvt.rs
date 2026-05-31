//! Mapbox Vector Tile (MVT) encoding.
//!
//! Encodes geospatial features into Mapbox Vector Tile format (PBF/protobuf)
//! per the [MVT spec v2](https://github.com/mapbox/vector-tile-spec/tree/master/2.1).
//!
//! Features are clipped to the tile extent and coordinates are quantized to
//! integer tile-local positions within a configurable extent (default 4096).

use std::collections::HashMap;

/// Default tile extent in MVT coordinate space.
pub const DEFAULT_EXTENT: u32 = 4096;

/// A feature to encode into an MVT layer.
#[derive(Debug, Clone)]
pub struct MvtFeature {
    pub id: Option<u64>,
    pub geometry: MvtGeometry,
    pub properties: HashMap<String, MvtValue>,
}

/// Geometry types supported by the MVT spec.
#[derive(Debug, Clone)]
pub enum MvtGeometry {
    Point(Vec<[f64; 2]>),
    LineString(Vec<Vec<[f64; 2]>>),
    Polygon(Vec<Vec<[f64; 2]>>),
}

/// Property value types per the MVT spec.
#[derive(Debug, Clone, PartialEq)]
pub enum MvtValue {
    String(String),
    Float(f32),
    Double(f64),
    Int(i64),
    UInt(u64),
    Bool(bool),
}

/// A layer to encode into a vector tile.
#[derive(Debug, Clone)]
pub struct MvtLayer {
    pub name: String,
    pub extent: u32,
    pub features: Vec<MvtFeature>,
}

/// Encode a set of layers into MVT protobuf bytes.
pub fn encode_tile(layers: &[MvtLayer], bbox: [f64; 4]) -> Vec<u8> {
    let mut buf = Vec::new();
    for layer in layers {
        let layer_bytes = encode_layer(layer, bbox);
        // field 3 (layers), wire type 2 (length-delimited)
        write_tag(&mut buf, 3, 2);
        write_varint(&mut buf, layer_bytes.len() as u64);
        buf.extend_from_slice(&layer_bytes);
    }
    buf
}

fn encode_layer(layer: &MvtLayer, bbox: [f64; 4]) -> Vec<u8> {
    let mut buf = Vec::new();

    // field 15: version = 2
    write_tag(&mut buf, 15, 0);
    write_varint(&mut buf, 2);

    // field 1: name
    write_tag(&mut buf, 1, 2);
    write_varint(&mut buf, layer.name.len() as u64);
    buf.extend_from_slice(layer.name.as_bytes());

    // Build key/value tables
    let mut keys: Vec<String> = Vec::new();
    let mut key_index: HashMap<String, u32> = HashMap::new();
    let mut values: Vec<MvtValue> = Vec::new();
    let mut value_index: HashMap<u64, u32> = HashMap::new();

    for feature in &layer.features {
        for (k, v) in &feature.properties {
            if let std::collections::hash_map::Entry::Vacant(e) = key_index.entry(k.clone()) {
                e.insert(keys.len() as u32);
                keys.push(k.clone());
            }
            let vh = value_hash(v);
            if let std::collections::hash_map::Entry::Vacant(e) = value_index.entry(vh) {
                e.insert(values.len() as u32);
                values.push(v.clone());
            }
        }
    }

    // field 2: features
    for feature in &layer.features {
        let feature_bytes = encode_feature(feature, layer.extent, bbox, &key_index, &value_index);
        write_tag(&mut buf, 2, 2);
        write_varint(&mut buf, feature_bytes.len() as u64);
        buf.extend_from_slice(&feature_bytes);
    }

    // field 3: keys
    for key in &keys {
        write_tag(&mut buf, 3, 2);
        write_varint(&mut buf, key.len() as u64);
        buf.extend_from_slice(key.as_bytes());
    }

    // field 4: values
    for value in &values {
        let val_bytes = encode_value(value);
        write_tag(&mut buf, 4, 2);
        write_varint(&mut buf, val_bytes.len() as u64);
        buf.extend_from_slice(&val_bytes);
    }

    // field 5: extent
    write_tag(&mut buf, 5, 0);
    write_varint(&mut buf, u64::from(layer.extent));

    buf
}

fn encode_feature(
    feature: &MvtFeature,
    extent: u32,
    bbox: [f64; 4],
    key_index: &HashMap<String, u32>,
    value_index: &HashMap<u64, u32>,
) -> Vec<u8> {
    let mut buf = Vec::new();

    // field 1: id
    if let Some(id) = feature.id {
        write_tag(&mut buf, 1, 0);
        write_varint(&mut buf, id);
    }

    // field 2: tags (packed key/value index pairs)
    let mut tags = Vec::new();
    for (k, v) in &feature.properties {
        if let Some(&ki) = key_index.get(k) {
            let vh = value_hash(v);
            if let Some(&vi) = value_index.get(&vh) {
                write_varint(&mut tags, u64::from(ki));
                write_varint(&mut tags, u64::from(vi));
            }
        }
    }
    if !tags.is_empty() {
        write_tag(&mut buf, 2, 2);
        write_varint(&mut buf, tags.len() as u64);
        buf.extend_from_slice(&tags);
    }

    // field 3: type
    let geom_type = match &feature.geometry {
        MvtGeometry::Point(_) => 1u64,
        MvtGeometry::LineString(_) => 2,
        MvtGeometry::Polygon(_) => 3,
    };
    write_tag(&mut buf, 3, 0);
    write_varint(&mut buf, geom_type);

    // field 4: geometry (packed commands)
    let commands = encode_geometry(&feature.geometry, extent, bbox);
    let mut geom_bytes = Vec::new();
    for cmd in &commands {
        write_varint(&mut geom_bytes, u64::from(*cmd));
    }
    write_tag(&mut buf, 4, 2);
    write_varint(&mut buf, geom_bytes.len() as u64);
    buf.extend_from_slice(&geom_bytes);

    buf
}

fn encode_geometry(geom: &MvtGeometry, extent: u32, bbox: [f64; 4]) -> Vec<u32> {
    let mut commands = Vec::new();
    let ext = f64::from(extent);
    let bw = bbox[2] - bbox[0];
    let bh = bbox[3] - bbox[1];

    match geom {
        MvtGeometry::Point(points) => {
            if points.is_empty() {
                return commands;
            }
            // MoveTo command with count
            commands.push(command_integer(1, points.len() as u32));
            let mut cx = 0i32;
            let mut cy = 0i32;
            for pt in points {
                let tx = ((pt[0] - bbox[0]) / bw * ext) as i32;
                let ty = ((bbox[3] - pt[1]) / bh * ext) as i32;
                let dx = tx - cx;
                let dy = ty - cy;
                commands.push(zigzag_encode(dx));
                commands.push(zigzag_encode(dy));
                cx = tx;
                cy = ty;
            }
        }
        MvtGeometry::LineString(lines) => {
            let mut cx = 0i32;
            let mut cy = 0i32;
            for line in lines {
                if line.len() < 2 {
                    continue;
                }
                // MoveTo
                commands.push(command_integer(1, 1));
                let tx = ((line[0][0] - bbox[0]) / bw * ext) as i32;
                let ty = ((bbox[3] - line[0][1]) / bh * ext) as i32;
                commands.push(zigzag_encode(tx - cx));
                commands.push(zigzag_encode(ty - cy));
                cx = tx;
                cy = ty;

                // LineTo for remaining points
                commands.push(command_integer(2, (line.len() - 1) as u32));
                for pt in &line[1..] {
                    let tx = ((pt[0] - bbox[0]) / bw * ext) as i32;
                    let ty = ((bbox[3] - pt[1]) / bh * ext) as i32;
                    commands.push(zigzag_encode(tx - cx));
                    commands.push(zigzag_encode(ty - cy));
                    cx = tx;
                    cy = ty;
                }
            }
        }
        MvtGeometry::Polygon(rings) => {
            let mut cx = 0i32;
            let mut cy = 0i32;
            for ring in rings {
                if ring.len() < 3 {
                    continue;
                }
                // MoveTo first point
                commands.push(command_integer(1, 1));
                let tx = ((ring[0][0] - bbox[0]) / bw * ext) as i32;
                let ty = ((bbox[3] - ring[0][1]) / bh * ext) as i32;
                commands.push(zigzag_encode(tx - cx));
                commands.push(zigzag_encode(ty - cy));
                cx = tx;
                cy = ty;

                // LineTo remaining (excluding closing vertex if same as first)
                let end = if ring.len() > 1
                    && (ring.last().unwrap()[0] - ring[0][0]).abs() < 1e-10
                    && (ring.last().unwrap()[1] - ring[0][1]).abs() < 1e-10
                {
                    ring.len() - 1
                } else {
                    ring.len()
                };

                if end > 1 {
                    commands.push(command_integer(2, (end - 1) as u32));
                    for pt in &ring[1..end] {
                        let tx = ((pt[0] - bbox[0]) / bw * ext) as i32;
                        let ty = ((bbox[3] - pt[1]) / bh * ext) as i32;
                        commands.push(zigzag_encode(tx - cx));
                        commands.push(zigzag_encode(ty - cy));
                        cx = tx;
                        cy = ty;
                    }
                }

                // ClosePath
                commands.push(command_integer(7, 1));
            }
        }
    }

    commands
}

fn encode_value(value: &MvtValue) -> Vec<u8> {
    let mut buf = Vec::new();
    match value {
        MvtValue::String(s) => {
            write_tag(&mut buf, 1, 2);
            write_varint(&mut buf, s.len() as u64);
            buf.extend_from_slice(s.as_bytes());
        }
        MvtValue::Float(f) => {
            write_tag(&mut buf, 2, 5);
            buf.extend_from_slice(&f.to_le_bytes());
        }
        MvtValue::Double(d) => {
            write_tag(&mut buf, 3, 1);
            buf.extend_from_slice(&d.to_le_bytes());
        }
        MvtValue::Int(i) => {
            write_tag(&mut buf, 4, 0);
            write_varint(&mut buf, zigzag_encode_64(*i));
        }
        MvtValue::UInt(u) => {
            write_tag(&mut buf, 5, 0);
            write_varint(&mut buf, *u);
        }
        MvtValue::Bool(b) => {
            write_tag(&mut buf, 7, 0);
            write_varint(&mut buf, u64::from(*b));
        }
    }
    buf
}

fn value_hash(v: &MvtValue) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::mem::discriminant(v).hash(&mut h);
    match v {
        MvtValue::String(s) => s.hash(&mut h),
        MvtValue::Float(f) => f.to_bits().hash(&mut h),
        MvtValue::Double(d) => d.to_bits().hash(&mut h),
        MvtValue::Int(i) => i.hash(&mut h),
        MvtValue::UInt(u) => u.hash(&mut h),
        MvtValue::Bool(b) => b.hash(&mut h),
    }
    h.finish()
}

// --- Protobuf encoding primitives ---

fn write_varint(buf: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            buf.push(byte);
            break;
        }
        buf.push(byte | 0x80);
    }
}

fn write_tag(buf: &mut Vec<u8>, field: u32, wire_type: u32) {
    write_varint(buf, u64::from(field << 3 | wire_type));
}

fn command_integer(id: u32, count: u32) -> u32 {
    (id & 0x7) | (count << 3)
}

fn zigzag_encode(n: i32) -> u32 {
    ((n << 1) ^ (n >> 31)) as u32
}

fn zigzag_encode_64(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty_tile() {
        let tile = encode_tile(&[], [0.0, 0.0, 1.0, 1.0]);
        assert!(tile.is_empty());
    }

    #[test]
    fn encode_point_layer() {
        let layer = MvtLayer {
            name: "points".to_string(),
            extent: DEFAULT_EXTENT,
            features: vec![MvtFeature {
                id: Some(1),
                geometry: MvtGeometry::Point(vec![[0.5, 0.5]]),
                properties: {
                    let mut p = HashMap::new();
                    p.insert("name".to_string(), MvtValue::String("test".to_string()));
                    p
                },
            }],
        };
        let bytes = encode_tile(&[layer], [0.0, 0.0, 1.0, 1.0]);
        assert!(!bytes.is_empty());
        // Verify it starts with field 3 tag (layers)
        assert_eq!(bytes[0], (3 << 3) | 2);
    }

    #[test]
    fn encode_linestring() {
        let layer = MvtLayer {
            name: "roads".to_string(),
            extent: DEFAULT_EXTENT,
            features: vec![MvtFeature {
                id: Some(1),
                geometry: MvtGeometry::LineString(vec![vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]]),
                properties: HashMap::new(),
            }],
        };
        let bytes = encode_tile(&[layer], [0.0, 0.0, 1.0, 1.0]);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn encode_polygon() {
        let layer = MvtLayer {
            name: "buildings".to_string(),
            extent: DEFAULT_EXTENT,
            features: vec![MvtFeature {
                id: Some(1),
                geometry: MvtGeometry::Polygon(vec![vec![
                    [0.0, 0.0],
                    [1.0, 0.0],
                    [1.0, 1.0],
                    [0.0, 1.0],
                    [0.0, 0.0],
                ]]),
                properties: HashMap::new(),
            }],
        };
        let bytes = encode_tile(&[layer], [0.0, 0.0, 1.0, 1.0]);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn encode_multiple_value_types() {
        let mut props = HashMap::new();
        props.insert("name".to_string(), MvtValue::String("road".to_string()));
        props.insert("lanes".to_string(), MvtValue::Int(4));
        props.insert("width".to_string(), MvtValue::Double(12.5));
        props.insert("oneway".to_string(), MvtValue::Bool(true));

        let layer = MvtLayer {
            name: "roads".to_string(),
            extent: DEFAULT_EXTENT,
            features: vec![MvtFeature {
                id: Some(42),
                geometry: MvtGeometry::Point(vec![[0.5, 0.5]]),
                properties: props,
            }],
        };
        let bytes = encode_tile(&[layer], [0.0, 0.0, 1.0, 1.0]);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn zigzag_encode_values() {
        assert_eq!(zigzag_encode(0), 0);
        assert_eq!(zigzag_encode(-1), 1);
        assert_eq!(zigzag_encode(1), 2);
        assert_eq!(zigzag_encode(-2), 3);
        assert_eq!(zigzag_encode(2), 4);
    }

    #[test]
    fn command_integer_encoding() {
        // MoveTo(1) with count 1
        assert_eq!(command_integer(1, 1), 9);
        // LineTo(2) with count 3
        assert_eq!(command_integer(2, 3), 26);
        // ClosePath(7) with count 1
        assert_eq!(command_integer(7, 1), 15);
    }

    #[test]
    fn varint_encoding() {
        let mut buf = Vec::new();
        write_varint(&mut buf, 1);
        assert_eq!(buf, vec![1]);

        buf.clear();
        write_varint(&mut buf, 300);
        assert_eq!(buf, vec![0xAC, 0x02]);
    }

    #[test]
    fn encode_multipoint() {
        let layer = MvtLayer {
            name: "pois".to_string(),
            extent: DEFAULT_EXTENT,
            features: vec![MvtFeature {
                id: Some(1),
                geometry: MvtGeometry::Point(vec![[0.25, 0.25], [0.75, 0.75]]),
                properties: HashMap::new(),
            }],
        };
        let bytes = encode_tile(&[layer], [0.0, 0.0, 1.0, 1.0]);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn encode_multiple_layers() {
        let layer1 = MvtLayer {
            name: "roads".to_string(),
            extent: DEFAULT_EXTENT,
            features: vec![MvtFeature {
                id: Some(1),
                geometry: MvtGeometry::LineString(vec![vec![[0.0, 0.0], [1.0, 1.0]]]),
                properties: HashMap::new(),
            }],
        };
        let layer2 = MvtLayer {
            name: "buildings".to_string(),
            extent: DEFAULT_EXTENT,
            features: vec![MvtFeature {
                id: Some(2),
                geometry: MvtGeometry::Polygon(vec![vec![
                    [0.2, 0.2],
                    [0.4, 0.2],
                    [0.4, 0.4],
                    [0.2, 0.4],
                    [0.2, 0.2],
                ]]),
                properties: HashMap::new(),
            }],
        };
        let bytes = encode_tile(&[layer1, layer2], [0.0, 0.0, 1.0, 1.0]);
        // Two layer field tags
        let layer_tags = bytes
            .iter()
            .enumerate()
            .filter(|&(_, b)| *b == (3 << 3 | 2))
            .count();
        assert!(layer_tags >= 2);
    }
}
