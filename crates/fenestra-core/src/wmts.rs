use crate::Error;

/// WMTS GetTile request parameters.
#[derive(Debug, Clone)]
pub struct WmtsGetTileRequest {
    pub layer: String,
    pub style: String,
    pub tile_matrix_set: String,
    pub tile_matrix: String,
    pub tile_row: u32,
    pub tile_col: u32,
    pub format: String,
}

impl WmtsGetTileRequest {
    pub fn parse(
        layer: &str,
        style: &str,
        tile_matrix_set: &str,
        tile_matrix: &str,
        tile_row: u32,
        tile_col: u32,
        format: &str,
    ) -> Result<Self, Error> {
        if layer.is_empty() {
            return Err(Error::InvalidRequest("layer must not be empty".into()));
        }
        Ok(Self {
            layer: layer.to_string(),
            style: style.to_string(),
            tile_matrix_set: tile_matrix_set.to_string(),
            tile_matrix: tile_matrix.to_string(),
            tile_row,
            tile_col,
            format: format.to_string(),
        })
    }

    /// Convert tile coordinates to geographic bounding box (Web Mercator, EPSG:3857).
    pub fn tile_bounds(&self) -> (f64, f64, f64, f64) {
        let z: u32 = self.tile_matrix.parse().unwrap_or(0);
        let n = 2.0_f64.powi(z as i32);
        let x = self.tile_col as f64;
        let y = self.tile_row as f64;

        let origin = 20037508.342789244;
        let tile_size = 2.0 * origin / n;

        let min_x = -origin + x * tile_size;
        let max_x = min_x + tile_size;
        let max_y = origin - y * tile_size;
        let min_y = max_y - tile_size;

        (min_x, min_y, max_x, max_y)
    }
}

/// WMTS tile response.
#[derive(Debug, Clone)]
pub struct WmtsResponse {
    pub content_type: String,
    pub data: Vec<u8>,
}

impl WmtsResponse {
    /// Create a placeholder 256x256 PNG tile (single color).
    pub fn placeholder_tile(r: u8, g: u8, b: u8) -> Self {
        // Minimal valid 1x1 PNG — for a real implementation you'd render map data
        let data = create_minimal_png(r, g, b);
        Self {
            content_type: "image/png".to_string(),
            data,
        }
    }

    /// Create a "not found" empty tile.
    pub fn empty_tile() -> Self {
        Self::placeholder_tile(0, 0, 0)
    }
}

/// Create a minimal 1x1 PNG image (for placeholder responses).
fn create_minimal_png(r: u8, g: u8, b: u8) -> Vec<u8> {
    // PNG signature
    let mut png = vec![137, 80, 78, 71, 13, 10, 26, 10];

    // IHDR chunk: 1x1, 8-bit RGB
    let ihdr_data: Vec<u8> = vec![
        0, 0, 0, 1, // width = 1
        0, 0, 0, 1, // height = 1
        8, // bit depth
        2, // color type (RGB)
        0, // compression
        0, // filter
        0, // interlace
    ];
    write_chunk(&mut png, b"IHDR", &ihdr_data);

    // IDAT chunk: single row with filter byte + RGB
    let raw_data = vec![0, r, g, b]; // filter=none + pixel
    let compressed = deflate_raw(&raw_data);
    write_chunk(&mut png, b"IDAT", &compressed);

    // IEND chunk
    write_chunk(&mut png, b"IEND", &[]);

    png
}

fn write_chunk(png: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let len = data.len() as u32;
    png.extend_from_slice(&len.to_be_bytes());
    png.extend_from_slice(chunk_type);
    png.extend_from_slice(data);
    // CRC32 of type + data
    let crc = crc32(chunk_type, data);
    png.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(chunk_type: &[u8], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in chunk_type.iter().chain(data.iter()) {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 == 1 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}

/// Minimal DEFLATE compression (store only — no actual compression).
fn deflate_raw(data: &[u8]) -> Vec<u8> {
    // zlib header
    let mut out = vec![0x78, 0x01];
    // Single stored block (BFINAL=1, BTYPE=00)
    let len = data.len() as u16;
    let nlen = !len;
    out.push(0x01); // BFINAL=1, BTYPE=00
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&nlen.to_le_bytes());
    out.extend_from_slice(data);
    // Adler32 checksum
    let adler = adler32(data);
    out.extend_from_slice(&adler.to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// Generate a WMTS capabilities XML document.
pub fn wmts_capabilities_xml(layers: &[&str], base_url: &str) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Capabilities xmlns="http://www.opengis.net/wmts/1.0"
  xmlns:ows="http://www.opengis.net/ows/1.1"
  version="1.0.0">
  <ows:ServiceIdentification>
    <ows:Title>Fenestra WMTS</ows:Title>
    <ows:ServiceType>OGC WMTS</ows:ServiceType>
    <ows:ServiceTypeVersion>1.0.0</ows:ServiceTypeVersion>
  </ows:ServiceIdentification>
  <Contents>
"#,
    );

    for layer in layers {
        xml.push_str(&format!(
            r#"    <Layer>
      <ows:Title>{layer}</ows:Title>
      <ows:Identifier>{layer}</ows:Identifier>
      <ResourceURL format="image/png"
        resourceType="tile"
        template="{base_url}/wmts/{layer}/{{TileMatrixSet}}/{{TileMatrix}}/{{TileRow}}/{{TileCol}}.png"/>
      <TileMatrixSetLink>
        <TileMatrixSet>WebMercatorQuad</TileMatrixSet>
      </TileMatrixSetLink>
    </Layer>
"#
        ));
    }

    xml.push_str(
        r#"    <TileMatrixSet>
      <ows:Identifier>WebMercatorQuad</ows:Identifier>
      <ows:SupportedCRS>urn:ogc:def:crs:EPSG::3857</ows:SupportedCRS>
    </TileMatrixSet>
  </Contents>
</Capabilities>"#,
    );

    xml
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wmts_get_tile_parse() {
        let req = WmtsGetTileRequest::parse(
            "roads",
            "default",
            "WebMercatorQuad",
            "5",
            10,
            15,
            "image/png",
        )
        .unwrap();
        assert_eq!(req.layer, "roads");
        assert_eq!(req.tile_row, 10);
        assert_eq!(req.tile_col, 15);
    }

    #[test]
    fn test_wmts_tile_bounds() {
        let req =
            WmtsGetTileRequest::parse("test", "default", "WebMercatorQuad", "0", 0, 0, "image/png")
                .unwrap();
        let (min_x, min_y, max_x, max_y) = req.tile_bounds();
        // At zoom 0, single tile covers entire world
        assert!((min_x - (-20037508.342789244)).abs() < 1.0);
        assert!((max_x - 20037508.342789244).abs() < 1.0);
        assert!((min_y - (-20037508.342789244)).abs() < 1.0);
        assert!((max_y - 20037508.342789244).abs() < 1.0);
    }

    #[test]
    fn test_wmts_placeholder_tile_is_valid_png() {
        let resp = WmtsResponse::placeholder_tile(128, 64, 32);
        assert_eq!(resp.content_type, "image/png");
        // PNG magic bytes
        assert_eq!(&resp.data[0..4], &[137, 80, 78, 71]);
    }

    #[test]
    fn test_wmts_capabilities_xml() {
        let xml = wmts_capabilities_xml(&["roads", "buildings"], "http://localhost:8080");
        assert!(xml.contains("Fenestra WMTS"));
        assert!(xml.contains("<ows:Identifier>roads</ows:Identifier>"));
        assert!(xml.contains("<ows:Identifier>buildings</ows:Identifier>"));
        assert!(xml.contains("WebMercatorQuad"));
    }
}
