//! Styled Layer Descriptor (SLD) / Symbology Encoding (SE) parser.
//!
//! Parses basic SLD/SE XML for point, line, and polygon symbolizers
//! with fill, stroke, and mark styling.

use crate::Error;

/// A parsed SLD style document.
#[derive(Debug, Clone)]
pub struct StyledLayerDescriptor {
    pub named_layers: Vec<NamedLayer>,
}

/// A named layer within an SLD.
#[derive(Debug, Clone)]
pub struct NamedLayer {
    pub name: String,
    pub styles: Vec<Style>,
}

/// A style containing one or more rules.
#[derive(Debug, Clone)]
pub struct Style {
    pub name: Option<String>,
    pub rules: Vec<Rule>,
}

/// A rendering rule with optional filter and symbolizers.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: Option<String>,
    pub min_scale: Option<f64>,
    pub max_scale: Option<f64>,
    pub symbolizers: Vec<Symbolizer>,
}

/// A symbolizer defining how to render a feature.
#[derive(Debug, Clone)]
pub enum Symbolizer {
    Point(PointSymbolizer),
    Line(LineSymbolizer),
    Polygon(PolygonSymbolizer),
    Text(TextSymbolizer),
}

/// Point symbolizer with optional mark or external graphic.
#[derive(Debug, Clone)]
pub struct PointSymbolizer {
    pub graphic: Graphic,
}

/// Line symbolizer with stroke properties.
#[derive(Debug, Clone)]
pub struct LineSymbolizer {
    pub stroke: Stroke,
}

/// Polygon symbolizer with fill and stroke.
#[derive(Debug, Clone)]
pub struct PolygonSymbolizer {
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

/// Text symbolizer for labels.
#[derive(Debug, Clone)]
pub struct TextSymbolizer {
    pub label_property: String,
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub fill: Option<Fill>,
}

/// A graphic element (mark or external).
#[derive(Debug, Clone)]
pub struct Graphic {
    pub mark: Option<Mark>,
    pub size: Option<f64>,
    pub rotation: Option<f64>,
}

/// A well-known mark shape.
#[derive(Debug, Clone)]
pub struct Mark {
    pub well_known_name: String,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

/// Fill styling.
#[derive(Debug, Clone)]
pub struct Fill {
    pub color: Option<String>,
    pub opacity: Option<f64>,
}

/// Stroke styling.
#[derive(Debug, Clone)]
pub struct Stroke {
    pub color: Option<String>,
    pub width: Option<f64>,
    pub opacity: Option<f64>,
    pub dash_array: Option<Vec<f64>>,
}

/// Parse an SLD XML document.
pub fn parse_sld(xml: &str) -> Result<StyledLayerDescriptor, Error> {
    let mut sld = StyledLayerDescriptor {
        named_layers: Vec::new(),
    };

    // Simple XML parsing without external dependencies
    let mut pos = 0;
    let bytes = xml.as_bytes();

    while let Some(layer_start) = find_tag(xml, pos, "NamedLayer") {
        let layer_end = find_closing_tag(xml, layer_start, "NamedLayer")
            .ok_or_else(|| Error::InvalidRequest("unclosed NamedLayer tag".into()))?;

        let layer_content = &xml[layer_start..layer_end];
        let name = extract_tag_content(layer_content, "Name").unwrap_or_default();

        let mut styles = Vec::new();
        let mut style_pos = 0;

        while let Some(style_start) = find_tag(layer_content, style_pos, "UserStyle") {
            let style_end = find_closing_tag(layer_content, style_start, "UserStyle")
                .unwrap_or(layer_content.len());
            let style_content = &layer_content[style_start..style_end];
            let style_name = extract_tag_content(style_content, "Name");

            let mut rules = Vec::new();
            let mut rule_pos = 0;

            while let Some(rule_start) = find_tag(style_content, rule_pos, "Rule") {
                let rule_end = find_closing_tag(style_content, rule_start, "Rule")
                    .unwrap_or(style_content.len());
                let rule_content = &style_content[rule_start..rule_end];

                let rule = parse_rule(rule_content);
                rules.push(rule);
                rule_pos = rule_end;
            }

            styles.push(Style {
                name: style_name,
                rules,
            });
            style_pos = style_end;
        }

        sld.named_layers.push(NamedLayer { name, styles });
        pos = layer_end;
        let _ = bytes; // suppress unused warning
    }

    Ok(sld)
}

fn parse_rule(content: &str) -> Rule {
    let name = extract_tag_content(content, "Name");
    let min_scale =
        extract_tag_content(content, "MinScaleDenominator").and_then(|s| s.parse::<f64>().ok());
    let max_scale =
        extract_tag_content(content, "MaxScaleDenominator").and_then(|s| s.parse::<f64>().ok());

    let mut symbolizers = Vec::new();

    // Point symbolizer
    if let Some(ps_start) = find_tag(content, 0, "PointSymbolizer") {
        let ps_end =
            find_closing_tag(content, ps_start, "PointSymbolizer").unwrap_or(content.len());
        let ps_content = &content[ps_start..ps_end];
        symbolizers.push(Symbolizer::Point(parse_point_symbolizer(ps_content)));
    }

    // Line symbolizer
    if let Some(ls_start) = find_tag(content, 0, "LineSymbolizer") {
        let ls_end = find_closing_tag(content, ls_start, "LineSymbolizer").unwrap_or(content.len());
        let ls_content = &content[ls_start..ls_end];
        symbolizers.push(Symbolizer::Line(parse_line_symbolizer(ls_content)));
    }

    // Polygon symbolizer
    if let Some(poly_start) = find_tag(content, 0, "PolygonSymbolizer") {
        let poly_end =
            find_closing_tag(content, poly_start, "PolygonSymbolizer").unwrap_or(content.len());
        let poly_content = &content[poly_start..poly_end];
        symbolizers.push(Symbolizer::Polygon(parse_polygon_symbolizer(poly_content)));
    }

    // Text symbolizer
    if let Some(ts_start) = find_tag(content, 0, "TextSymbolizer") {
        let ts_end = find_closing_tag(content, ts_start, "TextSymbolizer").unwrap_or(content.len());
        let ts_content = &content[ts_start..ts_end];
        symbolizers.push(Symbolizer::Text(parse_text_symbolizer(ts_content)));
    }

    Rule {
        name,
        min_scale,
        max_scale,
        symbolizers,
    }
}

fn parse_point_symbolizer(content: &str) -> PointSymbolizer {
    let mut graphic = Graphic {
        mark: None,
        size: None,
        rotation: None,
    };

    if let Some(g_start) = find_tag(content, 0, "Graphic") {
        let g_end = find_closing_tag(content, g_start, "Graphic").unwrap_or(content.len());
        let g_content = &content[g_start..g_end];

        graphic.size = extract_tag_content(g_content, "Size").and_then(|s| s.parse::<f64>().ok());
        graphic.rotation =
            extract_tag_content(g_content, "Rotation").and_then(|s| s.parse::<f64>().ok());

        if let Some(m_start) = find_tag(g_content, 0, "Mark") {
            let m_end = find_closing_tag(g_content, m_start, "Mark").unwrap_or(g_content.len());
            let m_content = &g_content[m_start..m_end];

            let well_known_name =
                extract_tag_content(m_content, "WellKnownName").unwrap_or_else(|| "square".into());
            let fill = parse_fill(m_content);
            let stroke = parse_stroke(m_content);

            graphic.mark = Some(Mark {
                well_known_name,
                fill,
                stroke,
            });
        }
    }

    PointSymbolizer { graphic }
}

fn parse_line_symbolizer(content: &str) -> LineSymbolizer {
    let stroke = parse_stroke(content).unwrap_or(Stroke {
        color: Some("#000000".into()),
        width: Some(1.0),
        opacity: None,
        dash_array: None,
    });
    LineSymbolizer { stroke }
}

fn parse_polygon_symbolizer(content: &str) -> PolygonSymbolizer {
    PolygonSymbolizer {
        fill: parse_fill(content),
        stroke: parse_stroke(content),
    }
}

fn parse_text_symbolizer(content: &str) -> TextSymbolizer {
    let label_property =
        extract_tag_content(content, "PropertyName").unwrap_or_else(|| "name".into());
    let font_family = extract_css_param(content, "font-family");
    let font_size = extract_css_param(content, "font-size").and_then(|s| s.parse::<f64>().ok());
    let fill = parse_fill(content);

    TextSymbolizer {
        label_property,
        font_family,
        font_size,
        fill,
    }
}

fn parse_fill(content: &str) -> Option<Fill> {
    let fill_start = find_tag(content, 0, "Fill")?;
    let fill_end = find_closing_tag(content, fill_start, "Fill").unwrap_or(content.len());
    let fill_content = &content[fill_start..fill_end];

    let color = extract_css_param(fill_content, "fill");
    let opacity =
        extract_css_param(fill_content, "fill-opacity").and_then(|s| s.parse::<f64>().ok());

    Some(Fill { color, opacity })
}

fn parse_stroke(content: &str) -> Option<Stroke> {
    let stroke_start = find_tag(content, 0, "Stroke")?;
    let stroke_end = find_closing_tag(content, stroke_start, "Stroke").unwrap_or(content.len());
    let stroke_content = &content[stroke_start..stroke_end];

    let color = extract_css_param(stroke_content, "stroke");
    let width =
        extract_css_param(stroke_content, "stroke-width").and_then(|s| s.parse::<f64>().ok());
    let opacity =
        extract_css_param(stroke_content, "stroke-opacity").and_then(|s| s.parse::<f64>().ok());
    let dash_array = extract_css_param(stroke_content, "stroke-dasharray").map(|s| {
        s.split_whitespace()
            .filter_map(|v| v.parse::<f64>().ok())
            .collect()
    });

    Some(Stroke {
        color,
        width,
        opacity,
        dash_array,
    })
}

/// Extract content of a CSS parameter from SLD CssParameter element.
fn extract_css_param(content: &str, name: &str) -> Option<String> {
    // Look for <CssParameter name="X">value</CssParameter>
    // or <se:CssParameter name="X">value</se:CssParameter>
    let patterns = [format!("name=\"{}\"", name), format!("name='{}'", name)];

    for pattern in &patterns {
        if let Some(idx) = content.find(pattern.as_str()) {
            // Find the > that closes this opening tag
            let after_attr = idx + pattern.len();
            if let Some(gt) = content[after_attr..].find('>') {
                let value_start = after_attr + gt + 1;
                // Find the closing </CssParameter> or </se:CssParameter>
                if let Some(end) = content[value_start..].find("</") {
                    let value = content[value_start..value_start + end].trim();
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

// --- Simple XML helpers (no external dependency) ---

fn find_tag(content: &str, start: usize, tag: &str) -> Option<usize> {
    let search = &content[start..];
    // Match <Tag or <ns:Tag, ensuring it's an exact tag (not prefix of longer name)
    let patterns = [
        format!("<{}", tag),
        format!("<se:{}", tag),
        format!("<sld:{}", tag),
    ];

    let mut best: Option<usize> = None;
    for pattern in &patterns {
        let mut offset = 0;
        while offset < search.len() {
            if let Some(idx) = search[offset..].find(pattern.as_str()) {
                let pos = offset + idx;
                let after = pos + pattern.len();
                if after < search.len() {
                    let ch = search.as_bytes()[after];
                    if ch == b'>'
                        || ch == b' '
                        || ch == b'/'
                        || ch == b'\t'
                        || ch == b'\n'
                        || ch == b'\r'
                    {
                        let abs_idx = start + pos;
                        best = Some(best.map_or(abs_idx, |b: usize| b.min(abs_idx)));
                        break;
                    }
                }
                offset = pos + 1;
            } else {
                break;
            }
        }
    }
    best
}

fn find_closing_tag(content: &str, start: usize, tag: &str) -> Option<usize> {
    let patterns = [
        format!("</{}>", tag),
        format!("</se:{}>", tag),
        format!("</sld:{}>", tag),
    ];

    let search = &content[start..];
    let mut best: Option<usize> = None;
    for pattern in &patterns {
        if let Some(idx) = search.find(pattern.as_str()) {
            let abs_idx = start + idx + pattern.len();
            best = Some(best.map_or(abs_idx, |b: usize| b.min(abs_idx)));
        }
    }
    best
}

fn extract_tag_content(content: &str, tag: &str) -> Option<String> {
    let start = find_tag(content, 0, tag)?;
    let search = &content[start..];
    let gt = search.find('>')?;
    let value_start = start + gt + 1;

    let patterns = [
        format!("</{}>", tag),
        format!("</se:{}>", tag),
        format!("</sld:{}>", tag),
    ];

    for pattern in &patterns {
        if let Some(end_idx) = content[value_start..].find(pattern.as_str()) {
            let value = content[value_start..value_start + end_idx].trim();
            return Some(value.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_polygon_sld() {
        let sld_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<StyledLayerDescriptor version="1.0.0">
  <NamedLayer>
    <Name>buildings</Name>
    <UserStyle>
      <Name>building-style</Name>
      <Rule>
        <Name>default</Name>
        <PolygonSymbolizer>
          <Fill>
            <CssParameter name="fill">#FF0000</CssParameter>
            <CssParameter name="fill-opacity">0.8</CssParameter>
          </Fill>
          <Stroke>
            <CssParameter name="stroke">#000000</CssParameter>
            <CssParameter name="stroke-width">1.5</CssParameter>
          </Stroke>
        </PolygonSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

        let sld = parse_sld(sld_xml).unwrap();
        assert_eq!(sld.named_layers.len(), 1);
        assert_eq!(sld.named_layers[0].name, "buildings");
        assert_eq!(sld.named_layers[0].styles.len(), 1);
        assert_eq!(
            sld.named_layers[0].styles[0].name.as_deref(),
            Some("building-style")
        );

        let rule = &sld.named_layers[0].styles[0].rules[0];
        assert_eq!(rule.name.as_deref(), Some("default"));
        assert_eq!(rule.symbolizers.len(), 1);

        if let Symbolizer::Polygon(ref ps) = rule.symbolizers[0] {
            let fill = ps.fill.as_ref().unwrap();
            assert_eq!(fill.color.as_deref(), Some("#FF0000"));
            assert_eq!(fill.opacity, Some(0.8));
            let stroke = ps.stroke.as_ref().unwrap();
            assert_eq!(stroke.color.as_deref(), Some("#000000"));
            assert_eq!(stroke.width, Some(1.5));
        } else {
            panic!("expected polygon symbolizer");
        }
    }

    #[test]
    fn test_parse_point_sld() {
        let sld_xml = r#"<StyledLayerDescriptor version="1.0.0">
  <NamedLayer>
    <Name>poi</Name>
    <UserStyle>
      <Name>circles</Name>
      <Rule>
        <PointSymbolizer>
          <Graphic>
            <Mark>
              <WellKnownName>circle</WellKnownName>
              <Fill>
                <CssParameter name="fill">#0000FF</CssParameter>
              </Fill>
              <Stroke>
                <CssParameter name="stroke">#FFFFFF</CssParameter>
                <CssParameter name="stroke-width">0.5</CssParameter>
              </Stroke>
            </Mark>
            <Size>12</Size>
          </Graphic>
        </PointSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

        let sld = parse_sld(sld_xml).unwrap();
        let rule = &sld.named_layers[0].styles[0].rules[0];

        if let Symbolizer::Point(ref ps) = rule.symbolizers[0] {
            assert_eq!(ps.graphic.size, Some(12.0));
            let mark = ps.graphic.mark.as_ref().unwrap();
            assert_eq!(mark.well_known_name, "circle");
            assert_eq!(
                mark.fill.as_ref().unwrap().color.as_deref(),
                Some("#0000FF")
            );
            assert_eq!(
                mark.stroke.as_ref().unwrap().color.as_deref(),
                Some("#FFFFFF")
            );
        } else {
            panic!("expected point symbolizer");
        }
    }

    #[test]
    fn test_parse_line_sld() {
        let sld_xml = r#"<StyledLayerDescriptor version="1.0.0">
  <NamedLayer>
    <Name>roads</Name>
    <UserStyle>
      <Name>road-style</Name>
      <Rule>
        <LineSymbolizer>
          <Stroke>
            <CssParameter name="stroke">#333333</CssParameter>
            <CssParameter name="stroke-width">3.0</CssParameter>
            <CssParameter name="stroke-dasharray">5 3</CssParameter>
          </Stroke>
        </LineSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

        let sld = parse_sld(sld_xml).unwrap();
        let rule = &sld.named_layers[0].styles[0].rules[0];

        if let Symbolizer::Line(ref ls) = rule.symbolizers[0] {
            assert_eq!(ls.stroke.color.as_deref(), Some("#333333"));
            assert_eq!(ls.stroke.width, Some(3.0));
            assert_eq!(ls.stroke.dash_array.as_deref(), Some(&[5.0, 3.0][..]));
        } else {
            panic!("expected line symbolizer");
        }
    }

    #[test]
    fn test_parse_scale_denominators() {
        let sld_xml = r#"<StyledLayerDescriptor version="1.0.0">
  <NamedLayer>
    <Name>test</Name>
    <UserStyle>
      <Name>scaled</Name>
      <Rule>
        <MinScaleDenominator>10000</MinScaleDenominator>
        <MaxScaleDenominator>500000</MaxScaleDenominator>
        <LineSymbolizer>
          <Stroke>
            <CssParameter name="stroke">#000000</CssParameter>
          </Stroke>
        </LineSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

        let sld = parse_sld(sld_xml).unwrap();
        let rule = &sld.named_layers[0].styles[0].rules[0];
        assert_eq!(rule.min_scale, Some(10000.0));
        assert_eq!(rule.max_scale, Some(500000.0));
    }
}
