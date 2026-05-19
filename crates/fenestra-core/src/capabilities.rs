use crate::config::ServiceConfig;

/// Metadata exposed in GetCapabilities responses.
#[derive(Debug, Clone)]
pub struct ServiceMetadata {
    pub title: String,
    pub abstract_text: String,
    pub wms_version: String,
    pub wfs_version: String,
}

impl From<&ServiceConfig> for ServiceMetadata {
    fn from(config: &ServiceConfig) -> Self {
        Self {
            title: config.title.clone(),
            abstract_text: config.abstract_text.clone(),
            wms_version: "1.3.0".to_string(),
            wfs_version: "2.0.0".to_string(),
        }
    }
}

/// Generate a WMS GetCapabilities XML document.
pub fn wms_capabilities_xml(config: &ServiceConfig) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(r#"<WMS_Capabilities version="1.3.0">"#);
    xml.push('\n');
    xml.push_str("  <Service>\n");
    xml.push_str(&format!("    <Title>{}</Title>\n", config.title));
    xml.push_str(&format!(
        "    <Abstract>{}</Abstract>\n",
        config.abstract_text
    ));
    xml.push_str("  </Service>\n");
    xml.push_str("  <Capability>\n");
    xml.push_str("    <Layer>\n");
    for layer in &config.layers {
        xml.push_str("      <Layer queryable=\"1\">\n");
        xml.push_str(&format!("        <Name>{}</Name>\n", layer.name));
        xml.push_str(&format!("        <Title>{}</Title>\n", layer.title));
        for srs in &layer.srs {
            xml.push_str(&format!("        <CRS>{srs}</CRS>\n"));
        }
        xml.push_str("      </Layer>\n");
    }
    xml.push_str("    </Layer>\n");
    xml.push_str("  </Capability>\n");
    xml.push_str("</WMS_Capabilities>\n");
    xml
}

/// Generate a WFS GetCapabilities XML document.
pub fn wfs_capabilities_xml(config: &ServiceConfig) -> String {
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(r#"<wfs:WFS_Capabilities version="2.0.0">"#);
    xml.push('\n');
    xml.push_str("  <ows:ServiceIdentification>\n");
    xml.push_str(&format!("    <ows:Title>{}</ows:Title>\n", config.title));
    xml.push_str("  </ows:ServiceIdentification>\n");
    xml.push_str("  <FeatureTypeList>\n");
    for layer in &config.layers {
        xml.push_str("    <FeatureType>\n");
        xml.push_str(&format!("      <Name>{}</Name>\n", layer.name));
        xml.push_str(&format!("      <Title>{}</Title>\n", layer.title));
        xml.push_str("    </FeatureType>\n");
    }
    xml.push_str("  </FeatureTypeList>\n");
    xml.push_str("</wfs:WFS_Capabilities>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LayerConfig;

    #[test]
    fn test_wms_capabilities() {
        let config = ServiceConfig {
            title: "Test".to_string(),
            abstract_text: "Test server".to_string(),
            host: "localhost".to_string(),
            port: 8080,
            layers: vec![LayerConfig {
                name: "roads".to_string(),
                title: "Roads".to_string(),
                srs: vec!["EPSG:4326".to_string()],
                bbox: [-180.0, -90.0, 180.0, 90.0],
                source: "/data/roads.gpkg".to_string(),
            }],
        };
        let xml = wms_capabilities_xml(&config);
        assert!(xml.contains("<Name>roads</Name>"));
        assert!(xml.contains("<CRS>EPSG:4326</CRS>"));
    }
}
