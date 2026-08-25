use crate::config::{LayerConfig, ServiceConfig};
use crate::crs::{EPSG_3857, EPSG_3857_URN, EPSG_4326, EPSG_4326_URN, bbox_to_web_mercator};
use crate::wfs::{
    DESCRIBE_FEATURE_TYPE_FORMAT, GEOJSON_OUTPUT_FORMAT, WFS_NAMESPACE, WFS_SCHEMA_LOCATION,
};
use crate::xml::{OWS_1_1_NAMESPACE, XLINK_NAMESPACE, XSI_NAMESPACE, escape};

const WMS_NAMESPACE: &str = "http://www.opengis.net/wms";
const WMS_SCHEMA_LOCATION: &str = "http://schemas.opengis.net/wms/1.3.0/capabilities_1_3_0.xsd";

/// WFS operations, each with the output formats it advertises.
const WFS_OPERATIONS: [(&str, &[&str]); 3] = [
    ("GetCapabilities", &[]),
    ("DescribeFeatureType", &[DESCRIBE_FEATURE_TYPE_FORMAT]),
    ("GetFeature", &[GEOJSON_OUTPUT_FORMAT]),
];

/// WFS 2.0 conformance classes and whether this server implements them.
const WFS_CONFORMANCE: [(&str, bool); 15] = [
    ("ImplementsSimpleWFS", false),
    ("ImplementsBasicWFS", false),
    ("ImplementsTransactionalWFS", false),
    ("ImplementsLockingWFS", false),
    ("KVPEncoding", true),
    ("XMLEncoding", false),
    ("SOAPEncoding", false),
    ("ImplementsInheritance", false),
    ("ImplementsRemoteResolve", false),
    ("ImplementsResultPaging", true),
    ("ImplementsStandardJoins", false),
    ("ImplementsSpatialJoins", false),
    ("ImplementsTemporalJoins", false),
    ("ImplementsFeatureVersioning", false),
    ("ManagesStoredQueries", false),
];

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

/// Generate a WMS 1.3.0 GetCapabilities XML document.
pub fn wms_capabilities_xml(config: &ServiceConfig, base_url: &str) -> String {
    let href = escape(&format!("{base_url}/wms?"));
    let title = escape(&config.title);
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(&format!(
        concat!(
            r#"<WMS_Capabilities version="1.3.0" xmlns="{}""#,
            r#" xmlns:xlink="{}" xmlns:xsi="{}""#,
            r#" xsi:schemaLocation="{} {}">"#,
            "\n",
        ),
        WMS_NAMESPACE, XLINK_NAMESPACE, XSI_NAMESPACE, WMS_NAMESPACE, WMS_SCHEMA_LOCATION
    ));
    xml.push_str("  <Service>\n");
    xml.push_str("    <Name>WMS</Name>\n");
    xml.push_str(&format!("    <Title>{title}</Title>\n"));
    xml.push_str(&format!(
        "    <Abstract>{}</Abstract>\n",
        escape(&config.abstract_text)
    ));
    xml.push_str(&format!(
        "    <OnlineResource xlink:type=\"simple\" xlink:href=\"{href}\"/>\n"
    ));
    xml.push_str("  </Service>\n");
    xml.push_str("  <Capability>\n");
    // no GetFeatureInfo: the server renders tiles but cannot query them
    xml.push_str(&format!(
        r#"    <Request>
      <GetCapabilities>
        <Format>text/xml</Format>
        <DCPType>
          <HTTP>
            <Get>
              <OnlineResource xlink:type="simple" xlink:href="{href}"/>
            </Get>
          </HTTP>
        </DCPType>
      </GetCapabilities>
      <GetMap>
        <Format>image/png</Format>
        <DCPType>
          <HTTP>
            <Get>
              <OnlineResource xlink:type="simple" xlink:href="{href}"/>
            </Get>
          </HTTP>
        </DCPType>
      </GetMap>
    </Request>
    <Exception>
      <Format>XML</Format>
    </Exception>
"#
    ));
    xml.push_str("    <Layer>\n");
    xml.push_str(&format!("      <Title>{title}</Title>\n"));
    xml.push_str(&format!("      <CRS>{EPSG_4326}</CRS>\n"));
    xml.push_str(&format!("      <CRS>{EPSG_3857}</CRS>\n"));
    for layer in &config.layers {
        xml.push_str(&wms_layer_xml(layer));
    }
    xml.push_str("    </Layer>\n");
    xml.push_str("  </Capability>\n");
    xml.push_str("</WMS_Capabilities>\n");
    xml
}

fn wms_layer_xml(layer: &LayerConfig) -> String {
    let [min_x, min_y, max_x, max_y] = layer.bbox;
    let mut xml = String::new();
    xml.push_str("      <Layer queryable=\"0\">\n");
    xml.push_str(&format!("        <Name>{}</Name>\n", escape(&layer.name)));
    xml.push_str(&format!(
        "        <Title>{}</Title>\n",
        escape(&layer.title)
    ));
    for srs in &layer.srs {
        xml.push_str(&format!("        <CRS>{}</CRS>\n", escape(srs)));
    }
    xml.push_str(&format!(
        r#"        <EX_GeographicBoundingBox>
          <westBoundLongitude>{min_x}</westBoundLongitude>
          <eastBoundLongitude>{max_x}</eastBoundLongitude>
          <southBoundLatitude>{min_y}</southBoundLatitude>
          <northBoundLatitude>{max_y}</northBoundLatitude>
        </EX_GeographicBoundingBox>
"#
    ));
    for srs in &layer.srs {
        if let Some(bounding_box) = wms_bounding_box_xml(srs, layer.bbox) {
            xml.push_str(&bounding_box);
        }
    }
    xml.push_str("      </Layer>\n");
    xml
}

/// A `BoundingBox` in the axis order the CRS declares. `None` for a CRS this
/// server cannot express the extent in.
fn wms_bounding_box_xml(crs: &str, bbox: [f64; 4]) -> Option<String> {
    let [min_x, min_y, max_x, max_y] = bbox;
    // wms 1.3.0 uses each crs's own axis order, and EPSG:4326 is latitude first
    let [minx, miny, maxx, maxy] = match crs {
        EPSG_4326 => [min_y, min_x, max_y, max_x],
        EPSG_3857 => bbox_to_web_mercator(bbox),
        _ => return None,
    };
    Some(format!(
        "        <BoundingBox CRS=\"{crs}\" minx=\"{minx}\" miny=\"{miny}\" maxx=\"{maxx}\" maxy=\"{maxy}\"/>\n"
    ))
}

/// Generate a WFS 2.0.0 GetCapabilities XML document.
pub fn wfs_capabilities_xml(config: &ServiceConfig, base_url: &str) -> String {
    let href = escape(&format!("{base_url}/wfs?"));
    let mut xml = String::new();
    xml.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push('\n');
    xml.push_str(&format!(
        concat!(
            r#"<WFS_Capabilities version="2.0.0" xmlns="{}""#,
            r#" xmlns:ows="{}" xmlns:xlink="{}" xmlns:xsi="{}""#,
            r#" xsi:schemaLocation="{} {}">"#,
            "\n",
        ),
        WFS_NAMESPACE,
        OWS_1_1_NAMESPACE,
        XLINK_NAMESPACE,
        XSI_NAMESPACE,
        WFS_NAMESPACE,
        WFS_SCHEMA_LOCATION
    ));
    xml.push_str("  <ows:ServiceIdentification>\n");
    xml.push_str(&format!(
        "    <ows:Title>{}</ows:Title>\n",
        escape(&config.title)
    ));
    xml.push_str(&format!(
        "    <ows:Abstract>{}</ows:Abstract>\n",
        escape(&config.abstract_text)
    ));
    xml.push_str("    <ows:ServiceType>WFS</ows:ServiceType>\n");
    xml.push_str("    <ows:ServiceTypeVersion>2.0.0</ows:ServiceTypeVersion>\n");
    xml.push_str("  </ows:ServiceIdentification>\n");
    xml.push_str("  <ows:ServiceProvider>\n");
    xml.push_str("    <ows:ProviderName>Fenestra</ows:ProviderName>\n");
    xml.push_str(&format!(
        "    <ows:ProviderSite xlink:type=\"simple\" xlink:href=\"{href}\"/>\n"
    ));
    xml.push_str("  </ows:ServiceProvider>\n");
    xml.push_str("  <ows:OperationsMetadata>\n");
    for (operation, output_formats) in WFS_OPERATIONS {
        xml.push_str(&format!("    <ows:Operation name=\"{operation}\">\n"));
        xml.push_str("      <ows:DCP>\n        <ows:HTTP>\n");
        xml.push_str(&format!(
            "          <ows:Get xlink:type=\"simple\" xlink:href=\"{href}\"/>\n"
        ));
        xml.push_str("        </ows:HTTP>\n      </ows:DCP>\n");
        if !output_formats.is_empty() {
            xml.push_str("      <ows:Parameter name=\"outputFormat\">\n");
            xml.push_str("        <ows:AllowedValues>\n");
            for format in output_formats {
                xml.push_str(&format!(
                    "          <ows:Value>{}</ows:Value>\n",
                    escape(format)
                ));
            }
            xml.push_str("        </ows:AllowedValues>\n");
            xml.push_str("      </ows:Parameter>\n");
        }
        xml.push_str("    </ows:Operation>\n");
    }
    xml.push_str("    <ows:Parameter name=\"version\">\n");
    xml.push_str("      <ows:AllowedValues>\n");
    xml.push_str("        <ows:Value>2.0.0</ows:Value>\n");
    xml.push_str("      </ows:AllowedValues>\n");
    xml.push_str("    </ows:Parameter>\n");
    for (name, implemented) in WFS_CONFORMANCE {
        let value = if implemented { "TRUE" } else { "FALSE" };
        xml.push_str(&format!("    <ows:Constraint name=\"{name}\">\n"));
        xml.push_str("      <ows:NoValues/>\n");
        xml.push_str(&format!(
            "      <ows:DefaultValue>{value}</ows:DefaultValue>\n"
        ));
        xml.push_str("    </ows:Constraint>\n");
    }
    xml.push_str("  </ows:OperationsMetadata>\n");
    xml.push_str("  <FeatureTypeList>\n");
    for layer in &config.layers {
        let [min_x, min_y, max_x, max_y] = layer.bbox;
        xml.push_str("    <FeatureType>\n");
        xml.push_str(&format!("      <Name>{}</Name>\n", escape(&layer.name)));
        xml.push_str(&format!("      <Title>{}</Title>\n", escape(&layer.title)));
        xml.push_str(&format!("      <DefaultCRS>{EPSG_4326_URN}</DefaultCRS>\n"));
        xml.push_str(&format!("      <OtherCRS>{EPSG_3857_URN}</OtherCRS>\n"));
        xml.push_str("      <ows:WGS84BoundingBox>\n");
        xml.push_str(&format!(
            "        <ows:LowerCorner>{min_x} {min_y}</ows:LowerCorner>\n"
        ));
        xml.push_str(&format!(
            "        <ows:UpperCorner>{max_x} {max_y}</ows:UpperCorner>\n"
        ));
        xml.push_str("      </ows:WGS84BoundingBox>\n");
        xml.push_str("    </FeatureType>\n");
    }
    xml.push_str("  </FeatureTypeList>\n");
    xml.push_str("</WFS_Capabilities>\n");
    xml
}
