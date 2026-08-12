//! Convert a parsed SLD style into the data-driven symbology the viewer applies
//! to a vector layer: graduated, categorized or rule-based colouring.
//!
//! The symbology carries colour and nothing else, so most of what an SLD says
//! about drawing has no home here. Rather than approximate it, everything left
//! behind is listed in [`SymbologyConversion::unsupported`] so the caller can
//! tell the user what did not come across.

use crate::Error;
use crate::sld::{
    ComparisonOp, Filter, NamedLayer, Rule, Style, StyledLayerDescriptor, Symbolizer,
};
use serde::Serialize;

/// A graduated symbology needs a break method and a colour ramp. An SLD states
/// neither, and its explicit per-class colours are what actually render.
const ASSUMED_BREAK_METHOD: &str = "equal";
const ASSUMED_RAMP: &str = "viridis";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Symbology {
    Graduated {
        field: String,
        method: &'static str,
        ramp: &'static str,
        breaks: Vec<f64>,
        colors: Vec<String>,
    },
    Categorized {
        field: String,
        categories: Vec<Category>,
    },
    Rules {
        rules: Vec<SymbologyRule>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Category {
    pub value: serde_json::Value,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SymbologyRule {
    pub field: String,
    pub op: &'static str,
    pub value: String,
    pub color: String,
}

/// Something in the SLD that the symbology shape could not carry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Unsupported {
    /// The SLD element it came from, by local name.
    pub construct: String,
    pub rule_index: Option<usize>,
    pub rule_name: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SymbologyConversion {
    pub layer: String,
    pub style: Option<String>,
    /// None when nothing in the style classifies features by a property.
    pub symbology: Option<Symbology>,
    pub unsupported: Vec<Unsupported>,
}

struct Report {
    entries: Vec<Unsupported>,
}

impl Report {
    fn style_level(&mut self, construct: &str, detail: impl Into<String>) {
        self.entries.push(Unsupported {
            construct: construct.to_string(),
            rule_index: None,
            rule_name: None,
            detail: detail.into(),
        });
    }

    fn rule_level(
        &mut self,
        construct: &str,
        index: usize,
        rule: &Rule,
        detail: impl Into<String>,
    ) {
        self.entries.push(Unsupported {
            construct: construct.to_string(),
            rule_index: Some(index),
            rule_name: rule.name.clone(),
            detail: detail.into(),
        });
    }
}

/// A rule reduced to the one thing the symbology can use: a colour.
struct ColoredRule<'a> {
    index: usize,
    rule: &'a Rule,
    color: String,
}

fn comparison_op(op: ComparisonOp) -> &'static str {
    match op {
        ComparisonOp::EqualTo => "==",
        ComparisonOp::NotEqualTo => "!=",
        ComparisonOp::LessThan => "<",
        ComparisonOp::LessThanOrEqualTo => "<=",
        ComparisonOp::GreaterThan => ">",
        ComparisonOp::GreaterThanOrEqualTo => ">=",
    }
}

/// The first colour a rule draws with: a fill where there is one, otherwise a
/// stroke. Labels never contribute a colour.
fn rule_color(rule: &Rule) -> Option<String> {
    rule.symbolizers
        .iter()
        .find_map(|symbolizer| match symbolizer {
            Symbolizer::Polygon(polygon) => polygon
                .fill
                .as_ref()
                .and_then(|fill| fill.color.clone())
                .or_else(|| polygon.stroke.as_ref().and_then(|s| s.color.clone())),
            Symbolizer::Line(line) => line.stroke.color.clone(),
            Symbolizer::Point(point) => point.graphic.mark.as_ref().and_then(|mark| {
                mark.fill
                    .as_ref()
                    .and_then(|fill| fill.color.clone())
                    .or_else(|| mark.stroke.as_ref().and_then(|s| s.color.clone()))
            }),
            Symbolizer::Text(_) => None,
        })
}

/// A category value is compared strictly against the feature property, so a
/// literal that reads as a number has to arrive as one.
fn literal_value(literal: &str) -> serde_json::Value {
    if let Ok(int) = literal.parse::<i64>() {
        return int.into();
    }
    match literal.parse::<f64>() {
        Ok(float) if float.is_finite() => float.into(),
        _ => literal.into(),
    }
}

fn select_layer<'a>(
    sld: &'a StyledLayerDescriptor,
    layer: Option<&str>,
) -> Result<&'a NamedLayer, Error> {
    match layer {
        Some(name) => sld
            .named_layers
            .iter()
            .find(|l| l.name == name)
            .ok_or_else(|| Error::LayerNotFound(name.to_string())),
        None => sld
            .named_layers
            .first()
            .ok_or_else(|| Error::InvalidRequest("no NamedLayer in SLD document".into())),
    }
}

fn select_style<'a>(layer: &'a NamedLayer, style: Option<&str>) -> Result<&'a Style, Error> {
    match style {
        Some(name) => layer
            .styles
            .iter()
            .find(|s| s.name.as_deref() == Some(name))
            .ok_or_else(|| Error::InvalidRequest(format!("no UserStyle named {name}"))),
        None => layer.styles.first().ok_or_else(|| {
            Error::InvalidRequest(format!("no UserStyle in NamedLayer {}", layer.name))
        }),
    }
}

fn scale_range(rule: &Rule) -> Option<String> {
    match (rule.min_scale, rule.max_scale) {
        (Some(min), Some(max)) => Some(format!("{min} to {max}")),
        (Some(min), None) => Some(format!("{min} and above")),
        (None, Some(max)) => Some(format!("up to {max}")),
        (None, None) => None,
    }
}

/// Report the drawing instructions the symbology drops, once for the style
/// rather than once per rule that carries them.
fn report_decoration(style: &Style, report: &mut Report) {
    let mut labels = false;
    let mut stroke_detail = false;
    let mut fill_opacity = false;
    let mut graphic_detail = false;

    for rule in &style.rules {
        for symbolizer in &rule.symbolizers {
            let (fill, stroke) = match symbolizer {
                Symbolizer::Text(_) => {
                    labels = true;
                    continue;
                }
                Symbolizer::Polygon(polygon) => (polygon.fill.as_ref(), polygon.stroke.as_ref()),
                Symbolizer::Line(line) => (None, Some(&line.stroke)),
                Symbolizer::Point(point) => {
                    let graphic = &point.graphic;
                    if graphic.size.is_some()
                        || graphic.rotation.is_some()
                        || graphic.mark.is_some()
                    {
                        graphic_detail = true;
                    }
                    match graphic.mark.as_ref() {
                        Some(mark) => (mark.fill.as_ref(), mark.stroke.as_ref()),
                        None => (None, None),
                    }
                }
            };
            if fill.is_some_and(|f| f.opacity.is_some()) {
                fill_opacity = true;
            }
            if stroke
                .is_some_and(|s| s.width.is_some() || s.opacity.is_some() || s.dash_array.is_some())
            {
                stroke_detail = true;
            }
        }
        if let Some(range) = scale_range(rule) {
            report.style_level(
                "ScaleDenominator",
                format!(
                    "rule {} draws only at scale denominators {range}; symbology has no scale range",
                    rule.name.as_deref().unwrap_or("(unnamed)")
                ),
            );
        }
    }

    if labels {
        report.style_level(
            "TextSymbolizer",
            "labels are dropped: symbology sets colour only",
        );
    }
    if stroke_detail {
        report.style_level(
            "Stroke",
            "stroke width, opacity and dash pattern are dropped: symbology sets colour only",
        );
    }
    if fill_opacity {
        report.style_level(
            "Fill",
            "fill opacity is dropped: symbology sets colour only",
        );
    }
    if graphic_detail {
        report.style_level(
            "Graphic",
            "mark shape, size and rotation are dropped: symbology sets colour only",
        );
    }
}

fn colored_rules<'a>(style: &'a Style, report: &mut Report) -> Vec<ColoredRule<'a>> {
    let mut colored = Vec::new();
    for (index, rule) in style.rules.iter().enumerate() {
        match rule_color(rule) {
            Some(color) => colored.push(ColoredRule { index, rule, color }),
            None => report.rule_level(
                "Rule",
                index,
                rule,
                "rule dropped: no fill or stroke colour to apply",
            ),
        }
    }
    colored
}

/// Every rule tests the same property for equality, so each is one category.
fn as_categorized(colored: &[ColoredRule]) -> Option<Symbology> {
    let mut field: Option<&str> = None;
    let mut categories = Vec::new();

    for entry in colored {
        let Some(Filter::Comparison {
            property,
            op: ComparisonOp::EqualTo,
            value,
        }) = &entry.rule.filter
        else {
            return None;
        };
        let property = property.as_str();
        if *field.get_or_insert(property) != property {
            return None;
        }
        categories.push(Category {
            value: literal_value(value),
            color: entry.color.clone(),
        });
    }

    Some(Symbology::Categorized {
        field: field?.to_string(),
        categories,
    })
}

/// Every rule tests the same property for a numeric range, so the ranges are
/// the classes of a graduated renderer.
fn as_graduated(colored: &[ColoredRule], report: &mut Report) -> Option<Symbology> {
    let mut field: Option<&str> = None;
    let mut classes = Vec::new();

    for entry in colored {
        let Some(Filter::Between {
            property,
            lower,
            upper,
        }) = &entry.rule.filter
        else {
            return None;
        };
        let property = property.as_str();
        if *field.get_or_insert(property) != property {
            return None;
        }
        let (Ok(lower), Ok(upper)) = (lower.parse::<f64>(), upper.parse::<f64>()) else {
            return None;
        };
        classes.push((lower, upper, entry.color.clone()));
    }

    let field = field?;
    classes.sort_by(|a, b| a.0.total_cmp(&b.0));

    if classes.windows(2).any(|pair| pair[0].1 != pair[1].0) {
        report.style_level(
            "PropertyIsBetween",
            "the ranges do not meet end to end; a value in a gap takes the colour of the class below it",
        );
    }
    report.style_level(
        "PropertyIsBetween",
        format!(
            "the outer bounds are open: a value below {} or above {} takes the nearest class colour",
            classes[0].0,
            classes[classes.len() - 1].1
        ),
    );
    report.style_level(
        "UserStyle",
        format!(
            "an SLD states no break method or colour ramp, so method {ASSUMED_BREAK_METHOD} and ramp {ASSUMED_RAMP} are placeholders; the listed colours are what render"
        ),
    );

    Some(Symbology::Graduated {
        field: field.to_string(),
        method: ASSUMED_BREAK_METHOD,
        ramp: ASSUMED_RAMP,
        breaks: classes.iter().map(|(lower, _, _)| *lower).collect(),
        colors: classes.into_iter().map(|(_, _, color)| color).collect(),
    })
}

/// Whatever is left: one comparison per rule, first match wins.
fn as_rules(colored: &[ColoredRule], report: &mut Report) -> Option<Symbology> {
    let mut rules = Vec::new();

    for entry in colored {
        match &entry.rule.filter {
            Some(Filter::Comparison { property, op, value }) => rules.push(SymbologyRule {
                field: property.clone(),
                op: comparison_op(*op),
                value: value.clone(),
                color: entry.color.clone(),
            }),
            Some(Filter::Between { property, .. }) => report.rule_level(
                "PropertyIsBetween",
                entry.index,
                entry.rule,
                format!(
                    "rule dropped: a range on {property} needs two comparisons and a symbology rule holds one"
                ),
            ),
            Some(Filter::Else) => report.rule_level(
                "ElseFilter",
                entry.index,
                entry.rule,
                "rule dropped: a feature no rule matches keeps the layer colour",
            ),
            Some(Filter::Unsupported(name)) => report.rule_level(
                name,
                entry.index,
                entry.rule,
                "rule dropped: a symbology rule tests one property against one literal",
            ),
            None => report.rule_level(
                "Rule",
                entry.index,
                entry.rule,
                format!(
                    "rule dropped: it has no filter, so it paints every feature {}; use it as the layer colour",
                    entry.color
                ),
            ),
        }
    }

    (!rules.is_empty()).then_some(Symbology::Rules { rules })
}

/// Convert one style of an SLD document into viewer symbology.
pub fn sld_to_symbology(
    sld: &StyledLayerDescriptor,
    layer: Option<&str>,
    style: Option<&str>,
) -> Result<SymbologyConversion, Error> {
    let named_layer = select_layer(sld, layer)?;
    let selected = select_style(named_layer, style)?;
    let mut report = Report {
        entries: Vec::new(),
    };

    if sld.named_layers.len() > 1 {
        report.style_level(
            "NamedLayer",
            format!(
                "the document holds {} named layers and only {} converted",
                sld.named_layers.len(),
                named_layer.name
            ),
        );
    }
    if named_layer.styles.len() > 1 {
        report.style_level(
            "UserStyle",
            format!(
                "the layer holds {} styles and only one converted",
                named_layer.styles.len()
            ),
        );
    }
    report_decoration(selected, &mut report);

    let colored = colored_rules(selected, &mut report);
    let symbology = as_categorized(&colored)
        .or_else(|| as_graduated(&colored, &mut report))
        .or_else(|| as_rules(&colored, &mut report));

    Ok(SymbologyConversion {
        layer: named_layer.name.clone(),
        style: selected.name.clone(),
        symbology,
        unsupported: report.entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sld::parse_sld;

    fn convert(xml: &str) -> SymbologyConversion {
        sld_to_symbology(&parse_sld(xml).unwrap(), None, None).unwrap()
    }

    fn details(conversion: &SymbologyConversion, construct: &str) -> Vec<String> {
        conversion
            .unsupported
            .iter()
            .filter(|entry| entry.construct == construct)
            .map(|entry| entry.detail.clone())
            .collect()
    }

    const CATEGORIZED: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<StyledLayerDescriptor version="1.0.0" xmlns:ogc="http://www.opengis.net/ogc">
  <NamedLayer>
    <Name>landuse</Name>
    <UserStyle>
      <Name>by-type</Name>
      <Rule>
        <Name>forest</Name>
        <ogc:Filter>
          <ogc:PropertyIsEqualTo>
            <ogc:PropertyName>type</ogc:PropertyName>
            <ogc:Literal>forest</ogc:Literal>
          </ogc:PropertyIsEqualTo>
        </ogc:Filter>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#1B7837</CssParameter></Fill>
        </PolygonSymbolizer>
      </Rule>
      <Rule>
        <Name>water</Name>
        <ogc:Filter>
          <ogc:PropertyIsEqualTo>
            <ogc:PropertyName>type</ogc:PropertyName>
            <ogc:Literal>water</ogc:Literal>
          </ogc:PropertyIsEqualTo>
        </ogc:Filter>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#2166AC</CssParameter></Fill>
        </PolygonSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

    const GRADUATED: &str = r#"<StyledLayerDescriptor version="1.0.0" xmlns:ogc="http://www.opengis.net/ogc">
  <NamedLayer>
    <Name>counties</Name>
    <UserStyle>
      <Name>population</Name>
      <Rule>
        <ogc:Filter>
          <ogc:PropertyIsBetween>
            <ogc:PropertyName>pop</ogc:PropertyName>
            <ogc:LowerBoundary><ogc:Literal>1000</ogc:Literal></ogc:LowerBoundary>
            <ogc:UpperBoundary><ogc:Literal>5000</ogc:Literal></ogc:UpperBoundary>
          </ogc:PropertyIsBetween>
        </ogc:Filter>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#EDF8B1</CssParameter></Fill>
        </PolygonSymbolizer>
      </Rule>
      <Rule>
        <ogc:Filter>
          <ogc:PropertyIsBetween>
            <ogc:PropertyName>pop</ogc:PropertyName>
            <ogc:LowerBoundary><ogc:Literal>5000</ogc:Literal></ogc:LowerBoundary>
            <ogc:UpperBoundary><ogc:Literal>20000</ogc:Literal></ogc:UpperBoundary>
          </ogc:PropertyIsBetween>
        </ogc:Filter>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#2C7FB8</CssParameter></Fill>
        </PolygonSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

    #[test]
    fn equality_rules_on_one_field_become_categories() {
        let conversion = convert(CATEGORIZED);
        assert_eq!(conversion.layer, "landuse");
        assert_eq!(conversion.style.as_deref(), Some("by-type"));
        assert_eq!(
            conversion.symbology,
            Some(Symbology::Categorized {
                field: "type".into(),
                categories: vec![
                    Category {
                        value: "forest".into(),
                        color: "#1B7837".into(),
                    },
                    Category {
                        value: "water".into(),
                        color: "#2166AC".into(),
                    },
                ],
            })
        );
        assert!(conversion.unsupported.is_empty());
    }

    #[test]
    fn numeric_literals_stay_numbers() {
        let xml = CATEGORIZED.replace("forest", "3").replace("water", "4.5");
        let conversion = convert(&xml);
        let Some(Symbology::Categorized { categories, .. }) = conversion.symbology else {
            panic!("expected categorized");
        };
        assert_eq!(categories[0].value, serde_json::json!(3));
        assert_eq!(categories[1].value, serde_json::json!(4.5));
    }

    #[test]
    fn range_rules_become_graduated_classes() {
        let conversion = convert(GRADUATED);
        assert_eq!(
            conversion.symbology,
            Some(Symbology::Graduated {
                field: "pop".into(),
                method: "equal",
                ramp: "viridis",
                breaks: vec![1000.0, 5000.0],
                colors: vec!["#EDF8B1".into(), "#2C7FB8".into()],
            })
        );
        let notes = details(&conversion, "PropertyIsBetween");
        assert_eq!(notes.len(), 1, "contiguous ranges need no gap note");
        assert!(
            notes[0].contains("below 1000") && notes[0].contains("above 20000"),
            "open outer bounds reported: {notes:?}"
        );
        assert!(
            details(&conversion, "UserStyle")[0].contains("no break method or colour ramp"),
            "invented method and ramp reported"
        );
    }

    #[test]
    fn graduated_json_carries_the_keys_the_viewer_reads() {
        let conversion = convert(GRADUATED);
        assert_eq!(
            serde_json::to_value(conversion.symbology).unwrap(),
            serde_json::json!({
                "kind": "graduated",
                "field": "pop",
                "method": "equal",
                "ramp": "viridis",
                "breaks": [1000.0, 5000.0],
                "colors": ["#EDF8B1", "#2C7FB8"],
            })
        );
    }

    #[test]
    fn ranges_with_a_gap_are_reported() {
        let conversion = convert(&GRADUATED.replace(
            "<ogc:LowerBoundary><ogc:Literal>5000</ogc:Literal></ogc:LowerBoundary>",
            "<ogc:LowerBoundary><ogc:Literal>9000</ogc:Literal></ogc:LowerBoundary>",
        ));
        assert!(
            details(&conversion, "PropertyIsBetween")
                .iter()
                .any(|d| d.contains("do not meet end to end")),
            "gap between classes reported"
        );
    }

    #[test]
    fn mixed_comparisons_become_first_match_rules() {
        let xml = r#"<StyledLayerDescriptor version="1.0.0" xmlns:ogc="http://www.opengis.net/ogc">
  <NamedLayer>
    <Name>roads</Name>
    <UserStyle>
      <Name>by-speed</Name>
      <Rule>
        <ogc:Filter>
          <ogc:PropertyIsGreaterThan>
            <ogc:PropertyName>speed</ogc:PropertyName>
            <ogc:Literal>80</ogc:Literal>
          </ogc:PropertyIsGreaterThan>
        </ogc:Filter>
        <LineSymbolizer>
          <Stroke><CssParameter name="stroke">#D73027</CssParameter></Stroke>
        </LineSymbolizer>
      </Rule>
      <Rule>
        <ogc:Filter>
          <ogc:PropertyIsEqualTo>
            <ogc:PropertyName>surface</ogc:PropertyName>
            <ogc:Literal>gravel</ogc:Literal>
          </ogc:PropertyIsEqualTo>
        </ogc:Filter>
        <LineSymbolizer>
          <Stroke><CssParameter name="stroke">#8C510A</CssParameter></Stroke>
        </LineSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

        let conversion = convert(xml);
        assert_eq!(
            serde_json::to_value(&conversion.symbology).unwrap(),
            serde_json::json!({
                "kind": "rules",
                "rules": [
                    {"field": "speed", "op": ">", "value": "80", "color": "#D73027"},
                    {"field": "surface", "op": "==", "value": "gravel", "color": "#8C510A"},
                ],
            })
        );
        assert_eq!(
            conversion.symbology,
            Some(Symbology::Rules {
                rules: vec![
                    SymbologyRule {
                        field: "speed".into(),
                        op: ">",
                        value: "80".into(),
                        color: "#D73027".into(),
                    },
                    SymbologyRule {
                        field: "surface".into(),
                        op: "==",
                        value: "gravel".into(),
                        color: "#8C510A".into(),
                    },
                ],
            })
        );
    }

    #[test]
    fn a_single_unfiltered_rule_has_no_symbology() {
        let xml = r#"<StyledLayerDescriptor version="1.0.0">
  <NamedLayer>
    <Name>buildings</Name>
    <UserStyle>
      <Name>plain</Name>
      <Rule>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#FF0000</CssParameter></Fill>
        </PolygonSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

        let conversion = convert(xml);
        assert_eq!(conversion.symbology, None);
        assert!(
            details(&conversion, "Rule")[0].contains("#FF0000"),
            "the single colour is reported so the caller can offer it as the layer colour"
        );
    }

    #[test]
    fn nothing_the_shape_carries_is_reported() {
        let xml = r#"<StyledLayerDescriptor version="1.0.0" xmlns:ogc="http://www.opengis.net/ogc">
  <NamedLayer>
    <Name>places</Name>
    <UserStyle>
      <Name>mixed</Name>
      <Rule>
        <Name>big-towns</Name>
        <MinScaleDenominator>1000</MinScaleDenominator>
        <MaxScaleDenominator>500000</MaxScaleDenominator>
        <ogc:Filter>
          <ogc:And>
            <ogc:PropertyIsEqualTo>
              <ogc:PropertyName>kind</ogc:PropertyName>
              <ogc:Literal>town</ogc:Literal>
            </ogc:PropertyIsEqualTo>
            <ogc:PropertyIsGreaterThan>
              <ogc:PropertyName>pop</ogc:PropertyName>
              <ogc:Literal>1000</ogc:Literal>
            </ogc:PropertyIsGreaterThan>
          </ogc:And>
        </ogc:Filter>
        <PointSymbolizer>
          <Graphic>
            <Mark>
              <WellKnownName>star</WellKnownName>
              <Fill><CssParameter name="fill">#333333</CssParameter></Fill>
              <Stroke>
                <CssParameter name="stroke">#FFFFFF</CssParameter>
                <CssParameter name="stroke-width">2</CssParameter>
              </Stroke>
            </Mark>
            <Size>14</Size>
          </Graphic>
        </PointSymbolizer>
        <TextSymbolizer>
          <Label><ogc:PropertyName>name</ogc:PropertyName></Label>
        </TextSymbolizer>
      </Rule>
      <Rule>
        <Name>rest</Name>
        <ElseFilter/>
        <PointSymbolizer>
          <Graphic>
            <Mark>
              <Fill><CssParameter name="fill">#BBBBBB</CssParameter></Fill>
            </Mark>
          </Graphic>
        </PointSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>
</StyledLayerDescriptor>"#;

        let conversion = convert(xml);
        assert_eq!(
            conversion.symbology, None,
            "neither rule survives, so there is nothing to apply"
        );

        let constructs: Vec<&str> = conversion
            .unsupported
            .iter()
            .map(|entry| entry.construct.as_str())
            .collect();
        for expected in [
            "And",
            "ElseFilter",
            "ScaleDenominator",
            "TextSymbolizer",
            "Stroke",
            "Graphic",
        ] {
            assert!(
                constructs.contains(&expected),
                "{expected} reported as unsupported, got {constructs:?}"
            );
        }

        let and = conversion
            .unsupported
            .iter()
            .find(|entry| entry.construct == "And")
            .unwrap();
        assert_eq!(and.rule_index, Some(0));
        assert_eq!(and.rule_name.as_deref(), Some("big-towns"));
    }

    #[test]
    fn a_named_layer_and_style_can_be_picked() {
        let xml = CATEGORIZED.replace(
            "</NamedLayer>",
            r#"</NamedLayer>
  <NamedLayer>
    <Name>other</Name>
    <UserStyle>
      <Name>other-style</Name>
      <Rule>
        <PolygonSymbolizer>
          <Fill><CssParameter name="fill">#000000</CssParameter></Fill>
        </PolygonSymbolizer>
      </Rule>
    </UserStyle>
  </NamedLayer>"#,
        );
        let sld = parse_sld(&xml).unwrap();

        let conversion = sld_to_symbology(&sld, Some("other"), None).unwrap();
        assert_eq!(conversion.layer, "other");
        assert!(
            details(&conversion, "NamedLayer")[0].contains("2 named layers"),
            "the layers left behind are reported"
        );

        assert!(matches!(
            sld_to_symbology(&sld, Some("missing"), None),
            Err(Error::LayerNotFound(_))
        ));
        assert!(matches!(
            sld_to_symbology(&sld, None, Some("missing")),
            Err(Error::InvalidRequest(_))
        ));
    }
}
