//! OpenAPI 3.0 description of the routes the router registers.
//!
//! The path keys are the axum route patterns, so a route added without an
//! entry here shows up as a gap in `openapi_describes_every_registered_route`.

use fenestra_core::mvt::MVT_MEDIA_TYPE;
use serde_json::{Value, json};

/// Media type OGC API Features asks a `service-desc` document to be served as.
pub const OPENAPI_MEDIA_TYPE: &str = "application/vnd.oai.openapi+json;version=3.0";

const OPENAPI_VERSION: &str = "3.0.3";
const JSON: &str = "application/json";
const GEOJSON: &str = "application/geo+json";
const XML: &str = "application/xml";
const PNG: &str = "image/png";
const TIFF: &str = "image/tiff";
const TEXT: &str = "text/plain";

struct Parameter {
    name: &'static str,
    location: &'static str,
    schema_type: &'static str,
    description: &'static str,
}

struct Operation {
    path: &'static str,
    method: &'static str,
    summary: &'static str,
    parameters: &'static [Parameter],
    request_body: Option<&'static str>,
    response: &'static str,
    response_media_type: &'static str,
}

const COLLECTION_ID: Parameter = Parameter {
    name: "id",
    location: "path",
    schema_type: "string",
    description: "Collection identifier",
};
const FEATURE_ID: Parameter = Parameter {
    name: "featureId",
    location: "path",
    schema_type: "string",
    description: "Feature identifier within the collection",
};
const LIMIT: Parameter = Parameter {
    name: "limit",
    location: "query",
    schema_type: "integer",
    description: "Maximum features to return, 10 by default",
};
const OFFSET: Parameter = Parameter {
    name: "offset",
    location: "query",
    schema_type: "integer",
    description: "Features to skip before the page starts",
};
const BBOX: Parameter = Parameter {
    name: "bbox",
    location: "query",
    schema_type: "string",
    description: "minx,miny,maxx,maxy in CRS84, keeps features that intersect it",
};
const OGC_REQUEST: Parameter = Parameter {
    name: "request",
    location: "query",
    schema_type: "string",
    description: "OGC operation name, GetCapabilities by default",
};
const TILE_MATRIX: Parameter = Parameter {
    name: "tileMatrix",
    location: "path",
    schema_type: "integer",
    description: "Zoom level of the WebMercatorQuad tile matrix set",
};
const TILE_ROW: Parameter = Parameter {
    name: "tileRow",
    location: "path",
    schema_type: "integer",
    description: "Tile row, counted from the north edge",
};
const TILE_COLUMN: Parameter = Parameter {
    name: "tileCol",
    location: "path",
    schema_type: "integer",
    description: "Tile column, counted from the west edge",
};
const SLD_LAYER: Parameter = Parameter {
    name: "layer",
    location: "query",
    schema_type: "string",
    description: "NamedLayer to convert, the first one by default",
};
const SLD_STYLE: Parameter = Parameter {
    name: "style",
    location: "query",
    schema_type: "string",
    description: "UserStyle to convert, the first one by default",
};
const WMTS_REST_PARAMETERS: &[Parameter] = &[
    Parameter {
        name: "layer",
        location: "path",
        schema_type: "string",
        description: "Layer identifier",
    },
    Parameter {
        name: "tms",
        location: "path",
        schema_type: "string",
        description: "Tile matrix set identifier, WebMercatorQuad",
    },
    Parameter {
        name: "matrix",
        location: "path",
        schema_type: "string",
        description: "Tile matrix (zoom level)",
    },
    Parameter {
        name: "row",
        location: "path",
        schema_type: "integer",
        description: "Tile row",
    },
    Parameter {
        name: "col",
        location: "path",
        schema_type: "string",
        description: "Tile column, with an optional .png suffix",
    },
];

const OPERATIONS: &[Operation] = &[
    Operation {
        path: "/health",
        method: "get",
        summary: "Health check",
        parameters: &[],
        request_body: None,
        response: "The server is up",
        response_media_type: TEXT,
    },
    Operation {
        path: "/healthz",
        method: "get",
        summary: "Liveness probe",
        parameters: &[],
        request_body: None,
        response: "The process is alive",
        response_media_type: TEXT,
    },
    Operation {
        path: "/readyz",
        method: "get",
        summary: "Readiness probe",
        parameters: &[],
        request_body: None,
        response: "The server is ready to serve",
        response_media_type: TEXT,
    },
    Operation {
        path: "/metrics",
        method: "get",
        summary: "Prometheus metrics",
        parameters: &[],
        request_body: None,
        response: "Metrics in the Prometheus text exposition format",
        response_media_type: TEXT,
    },
    Operation {
        path: "/wms",
        method: "get",
        summary: "WMS 1.3.0 GetCapabilities and GetMap",
        parameters: &[OGC_REQUEST],
        request_body: None,
        response: "Capabilities XML, or a rendered map for GetMap",
        response_media_type: PNG,
    },
    Operation {
        path: "/wfs",
        method: "get",
        summary: "WFS 2.0.0 GetCapabilities, DescribeFeatureType and GetFeature",
        parameters: &[OGC_REQUEST],
        request_body: None,
        response: "Capabilities or schema XML, or GeoJSON features for GetFeature",
        response_media_type: XML,
    },
    Operation {
        path: "/wmts",
        method: "get",
        summary: "WMTS 1.0.0 GetCapabilities and GetTile",
        parameters: &[OGC_REQUEST],
        request_body: None,
        response: "Capabilities XML, or a rendered tile for GetTile",
        response_media_type: PNG,
    },
    Operation {
        path: "/wmts/{layer}/{tms}/{matrix}/{row}/{col}",
        method: "get",
        summary: "WMTS RESTful tile",
        parameters: WMTS_REST_PARAMETERS,
        request_body: None,
        response: "A rendered map tile",
        response_media_type: PNG,
    },
    Operation {
        path: "/wcs",
        method: "get",
        summary: "WCS 2.0.1 GetCapabilities, DescribeCoverage and GetCoverage",
        parameters: &[OGC_REQUEST],
        request_body: None,
        response: "Capabilities or coverage description XML, or a GeoTIFF for GetCoverage",
        response_media_type: TIFF,
    },
    Operation {
        path: "/ogc/",
        method: "get",
        summary: "OGC API Features landing page",
        parameters: &[],
        request_body: None,
        response: "Links to the conformance, collections and API documents",
        response_media_type: JSON,
    },
    Operation {
        path: "/ogc/api",
        method: "get",
        summary: "This OpenAPI document",
        parameters: &[],
        request_body: None,
        response: "The API definition",
        response_media_type: OPENAPI_MEDIA_TYPE,
    },
    Operation {
        path: "/ogc/conformance",
        method: "get",
        summary: "Conformance declaration",
        parameters: &[],
        request_body: None,
        response: "The conformance classes this server implements",
        response_media_type: JSON,
    },
    Operation {
        path: "/ogc/collections",
        method: "get",
        summary: "Feature collections",
        parameters: &[],
        request_body: None,
        response: "Every collection the source offers",
        response_media_type: JSON,
    },
    Operation {
        path: "/ogc/collections/{id}",
        method: "get",
        summary: "Single collection description",
        parameters: &[COLLECTION_ID],
        request_body: None,
        response: "Metadata and links for one collection",
        response_media_type: JSON,
    },
    Operation {
        path: "/ogc/collections/{id}/items",
        method: "get",
        summary: "Features of a collection",
        parameters: &[COLLECTION_ID, LIMIT, OFFSET, BBOX],
        request_body: None,
        response: "A page of features with paging links",
        response_media_type: GEOJSON,
    },
    Operation {
        path: "/ogc/collections/{id}/items/{featureId}",
        method: "get",
        summary: "Single feature",
        parameters: &[COLLECTION_ID, FEATURE_ID],
        request_body: None,
        response: "One feature, 404 when the collection has no such id",
        response_media_type: GEOJSON,
    },
    Operation {
        path: "/ogc/collections/{id}/tiles/WebMercatorQuad/{tileMatrix}/{tileRow}/{tileCol}",
        method: "get",
        summary: "Vector tile of a collection",
        parameters: &[COLLECTION_ID, TILE_MATRIX, TILE_ROW, TILE_COLUMN],
        request_body: None,
        response: "The features of the tile, encoded as a Mapbox vector tile",
        response_media_type: MVT_MEDIA_TYPE,
    },
    Operation {
        path: "/sld/symbology",
        method: "post",
        summary: "Convert an SLD document into viewer symbology",
        parameters: &[SLD_LAYER, SLD_STYLE],
        request_body: Some(XML),
        response: "The converted symbology and every construct it cannot carry",
        response_media_type: JSON,
    },
];

fn parameter_json(parameter: &Parameter) -> Value {
    json!({
        "name": parameter.name,
        "in": parameter.location,
        "description": parameter.description,
        "required": parameter.location == "path",
        "schema": {"type": parameter.schema_type},
    })
}

/// The OpenAPI description of this server, with `base_url` as its only server.
pub fn openapi_document(base_url: &str) -> Value {
    let mut paths = serde_json::Map::new();
    for operation in OPERATIONS {
        let parameters: Vec<Value> = operation.parameters.iter().map(parameter_json).collect();
        let mut body = json!({
            "summary": operation.summary,
            "parameters": parameters,
            "responses": {
                "200": {
                    "description": operation.response,
                    "content": {operation.response_media_type: {}},
                },
            },
        });
        if let Some(media_type) = operation.request_body {
            body["requestBody"] = json!({
                "required": true,
                "content": {media_type: {}},
            });
        }
        let entry = paths
            .entry(operation.path.to_string())
            .or_insert_with(|| json!({}));
        entry[operation.method] = body;
    }
    json!({
        "openapi": OPENAPI_VERSION,
        "info": {
            "title": "Fenestra OGC services",
            "description": "WMS, WFS, WMTS, WCS and OGC API Features over a Ptolemy source",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "servers": [{"url": base_url}],
        "paths": Value::Object(paths),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_documents_its_path_parameters() {
        for operation in OPERATIONS {
            for segment in operation.path.split('/') {
                let Some(name) = segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')) else {
                    continue;
                };
                assert!(
                    operation
                        .parameters
                        .iter()
                        .any(|p| p.name == name && p.location == "path"),
                    "{} has no parameter for {name}",
                    operation.path
                );
            }
        }
    }
}
