//! OGC API Processes — types for executing geoprocessing workflows.
//!
//! Implements the OGC API - Processes - Part 1: Core standard
//! for describing and executing geospatial processing operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Describes an available process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessDescription {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub version: String,
    pub inputs: HashMap<String, InputDescription>,
    pub outputs: HashMap<String, OutputDescription>,
    pub job_control_options: Vec<JobControlOption>,
}

/// Description of a process input parameter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputDescription {
    pub title: String,
    pub description: Option<String>,
    pub schema: SchemaRef,
    pub min_occurs: u32,
    pub max_occurs: MaxOccurs,
}

/// Description of a process output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputDescription {
    pub title: String,
    pub description: Option<String>,
    pub schema: SchemaRef,
}

/// Maximum number of occurrences.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MaxOccurs {
    Bounded(u32),
    Unbounded,
}

/// Schema reference for input/output types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaRef {
    pub r#type: String,
    pub format: Option<String>,
    pub content_media_type: Option<String>,
}

/// Job control options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobControlOption {
    #[serde(rename = "sync-execute")]
    SyncExecute,
    #[serde(rename = "async-execute")]
    AsyncExecute,
    #[serde(rename = "dismiss")]
    Dismiss,
}

/// Request to execute a process.
#[derive(Debug, Clone, Deserialize)]
pub struct ExecuteRequest {
    pub inputs: HashMap<String, serde_json::Value>,
    pub outputs: Option<HashMap<String, OutputRequest>>,
    pub response: Option<ResponseType>,
}

/// Requested output format.
#[derive(Debug, Clone, Deserialize)]
pub struct OutputRequest {
    pub format: Option<OutputFormat>,
    pub transmission_mode: Option<TransmissionMode>,
}

/// Output format specification.
#[derive(Debug, Clone, Deserialize)]
pub struct OutputFormat {
    pub media_type: Option<String>,
}

/// How results are transmitted.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum TransmissionMode {
    #[serde(rename = "value")]
    Value,
    #[serde(rename = "reference")]
    Reference,
}

/// Response type requested.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ResponseType {
    #[serde(rename = "raw")]
    Raw,
    #[serde(rename = "document")]
    Document,
}

/// Status of a processing job.
#[derive(Debug, Clone, Serialize)]
pub struct JobStatus {
    pub job_id: String,
    pub process_id: String,
    pub status: JobState,
    pub message: Option<String>,
    pub progress: Option<u8>,
    pub created: String,
    pub started: Option<String>,
    pub finished: Option<String>,
}

/// Job lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobState {
    #[serde(rename = "accepted")]
    Accepted,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "successful")]
    Successful,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "dismissed")]
    Dismissed,
}

/// Result of a completed process execution.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessResult {
    pub outputs: HashMap<String, serde_json::Value>,
}

/// Built-in process IDs.
pub mod builtin {
    pub const BUFFER: &str = "buffer";
    pub const CLIP: &str = "clip";
    pub const UNION: &str = "union";
    pub const INTERSECT: &str = "intersect";
    pub const SIMPLIFY: &str = "simplify";
    pub const CONVEX_HULL: &str = "convex-hull";
    pub const CENTROID: &str = "centroid";
    pub const DISSOLVE: &str = "dissolve";
    pub const REPROJECT: &str = "reproject";
    pub const AREA: &str = "area";
    pub const LENGTH: &str = "length";
    pub const DISTANCE: &str = "distance";
}

/// Create a standard buffer process description.
pub fn buffer_process() -> ProcessDescription {
    ProcessDescription {
        id: builtin::BUFFER.to_string(),
        title: "Buffer".to_string(),
        description: Some("Compute a buffer zone around geometries".to_string()),
        version: "1.0.0".to_string(),
        inputs: HashMap::from([
            (
                "geometry".to_string(),
                InputDescription {
                    title: "Input Geometry".to_string(),
                    description: Some("GeoJSON geometry or feature collection".to_string()),
                    schema: SchemaRef {
                        r#type: "object".to_string(),
                        format: None,
                        content_media_type: Some("application/geo+json".to_string()),
                    },
                    min_occurs: 1,
                    max_occurs: MaxOccurs::Bounded(1),
                },
            ),
            (
                "distance".to_string(),
                InputDescription {
                    title: "Buffer Distance".to_string(),
                    description: Some("Buffer distance in CRS units".to_string()),
                    schema: SchemaRef {
                        r#type: "number".to_string(),
                        format: None,
                        content_media_type: None,
                    },
                    min_occurs: 1,
                    max_occurs: MaxOccurs::Bounded(1),
                },
            ),
        ]),
        outputs: HashMap::from([(
            "result".to_string(),
            OutputDescription {
                title: "Buffered Geometry".to_string(),
                description: Some("GeoJSON result".to_string()),
                schema: SchemaRef {
                    r#type: "object".to_string(),
                    format: None,
                    content_media_type: Some("application/geo+json".to_string()),
                },
            },
        )]),
        job_control_options: vec![
            JobControlOption::SyncExecute,
            JobControlOption::AsyncExecute,
        ],
    }
}

/// Create a standard clip process description.
pub fn clip_process() -> ProcessDescription {
    ProcessDescription {
        id: builtin::CLIP.to_string(),
        title: "Clip".to_string(),
        description: Some("Clip features to a boundary geometry".to_string()),
        version: "1.0.0".to_string(),
        inputs: HashMap::from([
            (
                "input".to_string(),
                InputDescription {
                    title: "Input Features".to_string(),
                    description: Some("Features to clip".to_string()),
                    schema: SchemaRef {
                        r#type: "object".to_string(),
                        format: None,
                        content_media_type: Some("application/geo+json".to_string()),
                    },
                    min_occurs: 1,
                    max_occurs: MaxOccurs::Bounded(1),
                },
            ),
            (
                "clip_geometry".to_string(),
                InputDescription {
                    title: "Clip Geometry".to_string(),
                    description: Some("Boundary to clip to".to_string()),
                    schema: SchemaRef {
                        r#type: "object".to_string(),
                        format: None,
                        content_media_type: Some("application/geo+json".to_string()),
                    },
                    min_occurs: 1,
                    max_occurs: MaxOccurs::Bounded(1),
                },
            ),
        ]),
        outputs: HashMap::from([(
            "result".to_string(),
            OutputDescription {
                title: "Clipped Features".to_string(),
                description: Some("Clipped GeoJSON output".to_string()),
                schema: SchemaRef {
                    r#type: "object".to_string(),
                    format: None,
                    content_media_type: Some("application/geo+json".to_string()),
                },
            },
        )]),
        job_control_options: vec![
            JobControlOption::SyncExecute,
            JobControlOption::AsyncExecute,
        ],
    }
}

/// List all built-in process descriptions.
pub fn builtin_processes() -> Vec<ProcessDescription> {
    vec![buffer_process(), clip_process()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_process_description() {
        let p = buffer_process();
        assert_eq!(p.id, "buffer");
        assert!(p.inputs.contains_key("geometry"));
        assert!(p.inputs.contains_key("distance"));
        assert!(p.outputs.contains_key("result"));
    }

    #[test]
    fn test_job_state_serialization() {
        let s = serde_json::to_string(&JobState::Running).unwrap();
        assert_eq!(s, "\"running\"");
    }

    #[test]
    fn test_execute_request_deserialization() {
        let json = r#"{
            "inputs": {
                "geometry": {"type": "Point", "coordinates": [0, 0]},
                "distance": 100.0
            }
        }"#;
        let req: ExecuteRequest = serde_json::from_str(json).unwrap();
        assert!(req.inputs.contains_key("geometry"));
        assert!(req.inputs.contains_key("distance"));
    }
}
