//! Structural assertions on every capabilities document, parsed rather than
//! substring-matched, so a namespace or an element in the wrong place fails.

use fenestra_core::capabilities::{wfs_capabilities_xml, wms_capabilities_xml};
use fenestra_core::{
    Feature, FeatureTypeSchema, Geometry, LayerConfig, ServiceConfig, describe_feature_type_xml,
    wcs_capabilities_xml, wfs_hits_xml, wmts_capabilities_xml,
};
use roxmltree::{Document, Node};

const WMS_NAMESPACE: &str = "http://www.opengis.net/wms";
const WFS_NAMESPACE: &str = "http://www.opengis.net/wfs/2.0";
const WMTS_NAMESPACE: &str = "http://www.opengis.net/wmts/1.0";
const WCS_NAMESPACE: &str = "http://www.opengis.net/wcs/2.0";
const OWS_1_1_NAMESPACE: &str = "http://www.opengis.net/ows/1.1";
const XSI_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema-instance";
const XSD_NAMESPACE: &str = "http://www.w3.org/2001/XMLSchema";

const BASE_URL: &str = "http://localhost:8099";
/// Monaco, so the mercator numbers are not symmetric about zero.
const PARCEL_BBOX: [f64; 4] = [7.42, 43.72, 7.45, 43.76];

fn config() -> ServiceConfig {
    ServiceConfig {
        title: "Fenestra OGC Server".to_string(),
        abstract_text: "OGC WMS/WFS/WMTS services".to_string(),
        host: "0.0.0.0".to_string(),
        port: 8080,
        layers: vec![LayerConfig {
            name: "demo_parcels".to_string(),
            title: "Demo Parcels".to_string(),
            srs: vec!["EPSG:4326".to_string(), "EPSG:3857".to_string()],
            bbox: PARCEL_BBOX,
            source: String::new(),
        }],
    }
}

fn schema_location(document: &Document, namespace: &str) -> String {
    let declared = document
        .root_element()
        .attribute((XSI_NAMESPACE, "schemaLocation"))
        .unwrap_or_else(|| {
            panic!(
                "xsi:schemaLocation on {}",
                document.root_element().tag_name().name()
            )
        });
    assert!(
        declared.starts_with(namespace),
        "schemaLocation should pair {namespace} with its xsd: {declared}"
    );
    declared.to_string()
}

fn descendant<'a>(node: Node<'a, 'a>, namespace: &str, name: &str) -> Node<'a, 'a> {
    node.descendants()
        .find(|n| n.tag_name().name() == name && n.tag_name().namespace() == Some(namespace))
        .unwrap_or_else(|| panic!("{name} in {namespace}"))
}

fn descendants<'a>(node: Node<'a, 'a>, namespace: &str, name: &str) -> Vec<Node<'a, 'a>> {
    node.descendants()
        .filter(|n| n.tag_name().name() == name && n.tag_name().namespace() == Some(namespace))
        .collect()
}

fn text(node: Node<'_, '_>) -> String {
    node.text().unwrap_or_default().trim().to_string()
}

#[test]
fn wms_root_declares_its_namespace_and_schema() {
    let xml = wms_capabilities_xml(&config(), BASE_URL);
    let document = Document::parse(&xml).expect("well formed WMS capabilities");
    let root = document.root_element();
    assert_eq!(root.tag_name().name(), "WMS_Capabilities");
    assert_eq!(root.tag_name().namespace(), Some(WMS_NAMESPACE));
    assert_eq!(root.attribute("version"), Some("1.3.0"));
    assert!(schema_location(&document, WMS_NAMESPACE).ends_with("capabilities_1_3_0.xsd"));
}

#[test]
fn wms_service_and_request_point_at_the_public_url() {
    let xml = wms_capabilities_xml(&config(), BASE_URL);
    let document = Document::parse(&xml).unwrap();
    let root = document.root_element();
    let service = descendant(root, WMS_NAMESPACE, "Service");
    assert_eq!(text(descendant(service, WMS_NAMESPACE, "Name")), "WMS");

    let expected = format!("{BASE_URL}/wms?");
    let resources = descendants(root, WMS_NAMESPACE, "OnlineResource");
    assert!(!resources.is_empty());
    for resource in resources {
        assert_eq!(
            resource.attribute(("http://www.w3.org/1999/xlink", "href")),
            Some(expected.as_str())
        );
    }

    let request = descendant(root, WMS_NAMESPACE, "Request");
    let get_map = descendant(request, WMS_NAMESPACE, "GetMap");
    assert_eq!(
        text(descendant(get_map, WMS_NAMESPACE, "Format")),
        "image/png"
    );
    // no GetFeatureInfo implementation, so it must not be advertised
    assert!(descendants(request, WMS_NAMESPACE, "GetFeatureInfo").is_empty());
}

#[test]
fn wms_layer_carries_its_extent_in_every_advertised_crs() {
    let xml = wms_capabilities_xml(&config(), BASE_URL);
    let document = Document::parse(&xml).unwrap();
    let named = descendants(document.root_element(), WMS_NAMESPACE, "Layer")
        .into_iter()
        .find(|layer| {
            descendants(*layer, WMS_NAMESPACE, "Name")
                .first()
                .is_some_and(|name| text(*name) == "demo_parcels")
        })
        .expect("named layer");

    let geographic = descendant(named, WMS_NAMESPACE, "EX_GeographicBoundingBox");
    let bound = |name: &str| {
        text(descendant(geographic, WMS_NAMESPACE, name))
            .parse::<f64>()
            .unwrap()
    };
    assert_eq!(bound("westBoundLongitude"), PARCEL_BBOX[0]);
    assert_eq!(bound("southBoundLatitude"), PARCEL_BBOX[1]);
    assert_eq!(bound("eastBoundLongitude"), PARCEL_BBOX[2]);
    assert_eq!(bound("northBoundLatitude"), PARCEL_BBOX[3]);

    let boxes = descendants(named, WMS_NAMESPACE, "BoundingBox");
    let by_crs: std::collections::HashMap<&str, Node<'_, '_>> = boxes
        .iter()
        .map(|node| (node.attribute("CRS").unwrap(), *node))
        .collect();

    // WMS 1.3.0 orders EPSG:4326 latitude first
    let geographic_box = by_crs["EPSG:4326"];
    assert_eq!(geographic_box.attribute("minx"), Some("43.72"));
    assert_eq!(geographic_box.attribute("miny"), Some("7.42"));
    assert_eq!(geographic_box.attribute("maxx"), Some("43.76"));
    assert_eq!(geographic_box.attribute("maxy"), Some("7.45"));

    let mercator_box = by_crs["EPSG:3857"];
    let mercator = |name: &str| {
        mercator_box
            .attribute(name)
            .unwrap()
            .parse::<f64>()
            .unwrap()
    };
    assert!(
        (mercator("minx") - 825_990.6).abs() < 1.0,
        "{}",
        mercator("minx")
    );
    assert!(
        (mercator("miny") - 5_422_213.3).abs() < 1.0,
        "{}",
        mercator("miny")
    );
    assert!(
        (mercator("maxx") - 829_330.2).abs() < 1.0,
        "{}",
        mercator("maxx")
    );
    assert!(
        (mercator("maxy") - 5_428_376.4).abs() < 1.0,
        "{}",
        mercator("maxy")
    );
}

#[test]
fn wfs_root_declares_the_default_namespace_and_schema() {
    let xml = wfs_capabilities_xml(&config(), BASE_URL);
    let document = Document::parse(&xml).expect("well formed WFS capabilities");
    let root = document.root_element();
    assert_eq!(root.tag_name().name(), "WFS_Capabilities");
    assert_eq!(root.tag_name().namespace(), Some(WFS_NAMESPACE));
    assert_eq!(root.attribute("version"), Some("2.0.0"));
    assert!(schema_location(&document, WFS_NAMESPACE).ends_with("wfs.xsd"));

    let identification = descendant(root, OWS_1_1_NAMESPACE, "ServiceIdentification");
    assert_eq!(
        text(descendant(identification, OWS_1_1_NAMESPACE, "ServiceType")),
        "WFS"
    );
    assert_eq!(
        text(descendant(
            identification,
            OWS_1_1_NAMESPACE,
            "ServiceTypeVersion"
        )),
        "2.0.0"
    );
    descendant(root, OWS_1_1_NAMESPACE, "ServiceProvider");
}

#[test]
fn wfs_operations_metadata_lists_every_implemented_operation() {
    let xml = wfs_capabilities_xml(&config(), BASE_URL);
    let document = Document::parse(&xml).unwrap();
    let metadata = descendant(
        document.root_element(),
        OWS_1_1_NAMESPACE,
        "OperationsMetadata",
    );

    let operations = descendants(metadata, OWS_1_1_NAMESPACE, "Operation");
    let names: Vec<&str> = operations
        .iter()
        .map(|node| node.attribute("name").unwrap())
        .collect();
    assert_eq!(
        names,
        ["GetCapabilities", "DescribeFeatureType", "GetFeature"]
    );

    let expected = format!("{BASE_URL}/wfs?");
    for operation in &operations {
        let get = descendant(*operation, OWS_1_1_NAMESPACE, "Get");
        assert_eq!(
            get.attribute(("http://www.w3.org/1999/xlink", "href")),
            Some(expected.as_str())
        );
    }

    let get_feature = operations
        .iter()
        .find(|node| node.attribute("name") == Some("GetFeature"))
        .unwrap();
    let values: Vec<String> = descendants(*get_feature, OWS_1_1_NAMESPACE, "Value")
        .into_iter()
        .map(text)
        .collect();
    assert_eq!(values, ["application/json"]);

    let constraints: std::collections::HashMap<&str, String> =
        descendants(metadata, OWS_1_1_NAMESPACE, "Constraint")
            .into_iter()
            .map(|node| {
                (
                    node.attribute("name").unwrap(),
                    text(descendant(node, OWS_1_1_NAMESPACE, "DefaultValue")),
                )
            })
            .collect();
    assert_eq!(constraints["KVPEncoding"], "TRUE");
    assert_eq!(constraints["ImplementsResultPaging"], "TRUE");
    assert_eq!(constraints["ImplementsTransactionalWFS"], "FALSE");
    assert_eq!(constraints["ImplementsBasicWFS"], "FALSE");
}

#[test]
fn wfs_feature_type_carries_its_crs_and_extent() {
    let xml = wfs_capabilities_xml(&config(), BASE_URL);
    let document = Document::parse(&xml).unwrap();
    let feature_type = descendant(document.root_element(), WFS_NAMESPACE, "FeatureType");
    assert_eq!(
        text(descendant(feature_type, WFS_NAMESPACE, "Name")),
        "demo_parcels"
    );
    assert_eq!(
        text(descendant(feature_type, WFS_NAMESPACE, "DefaultCRS")),
        "urn:ogc:def:crs:EPSG::4326"
    );
    assert_eq!(
        text(descendant(feature_type, WFS_NAMESPACE, "OtherCRS")),
        "urn:ogc:def:crs:EPSG::3857"
    );
    let bounding_box = descendant(feature_type, OWS_1_1_NAMESPACE, "WGS84BoundingBox");
    assert_eq!(
        text(descendant(bounding_box, OWS_1_1_NAMESPACE, "LowerCorner")),
        "7.42 43.72"
    );
    assert_eq!(
        text(descendant(bounding_box, OWS_1_1_NAMESPACE, "UpperCorner")),
        "7.45 43.76"
    );
}

#[test]
fn describe_feature_type_extends_the_gml_feature_type() {
    let feature = Feature::new(
        Some("1".to_string()),
        Geometry::Polygon {
            coordinates: vec![vec![[7.42, 43.72]]],
        },
        serde_json::json!({"owner": "Meridian", "sqft": 52272, "acres": 1.2}),
    );
    let schema = FeatureTypeSchema::derive("demo_parcels", "polygon", Some(&feature));
    let xml = describe_feature_type_xml(&[schema]);
    let document = Document::parse(&xml).expect("well formed schema");
    let root = document.root_element();
    assert_eq!(root.tag_name().name(), "schema");
    assert_eq!(root.tag_name().namespace(), Some(XSD_NAMESPACE));

    let element = descendants(root, XSD_NAMESPACE, "element")
        .into_iter()
        .find(|node| node.attribute("name") == Some("demo_parcels"))
        .expect("top level element for the feature type");
    assert_eq!(element.attribute("type"), Some("demo_parcelsType"));
    assert_eq!(
        element.attribute("substitutionGroup"),
        Some("gml:AbstractFeature")
    );

    let extension = descendant(root, XSD_NAMESPACE, "extension");
    assert_eq!(extension.attribute("base"), Some("gml:AbstractFeatureType"));

    let properties: std::collections::HashMap<&str, &str> = descendants(
        descendant(root, XSD_NAMESPACE, "sequence"),
        XSD_NAMESPACE,
        "element",
    )
    .into_iter()
    .map(|node| {
        (
            node.attribute("name").unwrap(),
            node.attribute("type").unwrap(),
        )
    })
    .collect();
    assert_eq!(properties["geometry"], "gml:SurfacePropertyType");
    assert_eq!(properties["owner"], "xsd:string");
    assert_eq!(properties["sqft"], "xsd:integer");
    assert_eq!(properties["acres"], "xsd:double");
}

#[test]
fn hits_answer_reports_the_match_count_and_no_features() {
    let xml = wfs_hits_xml(50, "2026-08-25T12:00:00Z");
    let document = Document::parse(&xml).expect("well formed hits answer");
    let root = document.root_element();
    assert_eq!(root.tag_name().name(), "FeatureCollection");
    assert_eq!(root.tag_name().namespace(), Some(WFS_NAMESPACE));
    assert_eq!(root.attribute("numberMatched"), Some("50"));
    assert_eq!(root.attribute("numberReturned"), Some("0"));
    assert_eq!(root.attribute("timeStamp"), Some("2026-08-25T12:00:00Z"));
    assert!(root.children().all(|child| !child.is_element()));
}

#[test]
fn wmts_layer_has_the_style_format_and_extent_clients_require() {
    let xml = wmts_capabilities_xml(&config().layers, BASE_URL);
    let document = Document::parse(&xml).expect("well formed WMTS capabilities");
    let root = document.root_element();
    assert_eq!(root.tag_name().namespace(), Some(WMTS_NAMESPACE));
    assert!(
        schema_location(&document, WMTS_NAMESPACE).ends_with("wmtsGetCapabilities_response.xsd")
    );

    let layer = descendant(root, WMTS_NAMESPACE, "Layer");
    let style = descendant(layer, WMTS_NAMESPACE, "Style");
    assert_eq!(style.attribute("isDefault"), Some("true"));
    assert_eq!(
        text(descendant(style, OWS_1_1_NAMESPACE, "Identifier")),
        "default"
    );
    assert_eq!(
        text(descendant(layer, WMTS_NAMESPACE, "Format")),
        "image/png"
    );
    assert_eq!(
        text(descendant(
            descendant(layer, OWS_1_1_NAMESPACE, "WGS84BoundingBox"),
            OWS_1_1_NAMESPACE,
            "LowerCorner"
        )),
        "7.42 43.72"
    );
    let template = descendant(layer, WMTS_NAMESPACE, "ResourceURL")
        .attribute("template")
        .unwrap();
    assert!(template.starts_with(BASE_URL), "{template}");

    // TileMatrixSetLink holds a TileMatrixSet of the same name that is only a
    // reference, so pick the one that actually defines the grid
    let matrix_set = descendants(root, WMTS_NAMESPACE, "TileMatrixSet")
        .into_iter()
        .find(|node| !descendants(*node, WMTS_NAMESPACE, "TileMatrix").is_empty())
        .expect("TileMatrixSet definition");
    assert_eq!(
        text(descendant(matrix_set, OWS_1_1_NAMESPACE, "SupportedCRS")),
        "urn:ogc:def:crs:EPSG::3857"
    );
    assert_eq!(
        descendants(matrix_set, WMTS_NAMESPACE, "TileMatrix").len(),
        19
    );
}

#[test]
fn wcs_root_declares_its_namespace_and_schema() {
    let xml = wcs_capabilities_xml("Fenestra OGC Server", &["dem"]);
    let document = Document::parse(&xml).expect("well formed WCS capabilities");
    let root = document.root_element();
    assert_eq!(root.tag_name().name(), "Capabilities");
    assert_eq!(root.tag_name().namespace(), Some(WCS_NAMESPACE));
    assert!(schema_location(&document, WCS_NAMESPACE).ends_with("wcsAll.xsd"));
    assert_eq!(text(descendant(root, WCS_NAMESPACE, "CoverageId")), "dem");
}
