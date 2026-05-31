//! Metadata record storage and retrieval.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A stored metadata record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataRecord {
    /// Unique record identifier.
    pub id: String,
    /// Record title.
    pub title: String,
    /// Abstract/description.
    pub abstract_text: String,
    /// Keywords.
    pub keywords: Vec<String>,
    /// Bounding box [west, south, east, north].
    pub bbox: Option<[f64; 4]>,
    /// Date the record was created.
    pub date_created: DateTime<Utc>,
    /// Date the record was last modified.
    pub date_modified: DateTime<Utc>,
    /// Original format of the record.
    pub format: RecordFormat,
    /// Raw XML content (ISO 19139 or Dublin Core).
    pub raw_xml: String,
    /// Source URL if harvested.
    pub source_url: Option<String>,
}

impl MetadataRecord {
    /// Creates a new empty record with a generated UUID.
    pub fn new(title: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            abstract_text: String::new(),
            keywords: Vec::new(),
            bbox: None,
            date_created: now,
            date_modified: now,
            format: RecordFormat::Iso19139,
            raw_xml: String::new(),
            source_url: None,
        }
    }
}

/// Record format standard.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecordFormat {
    /// ISO 19139 Geographic Metadata XML.
    Iso19139,
    /// Dublin Core.
    DublinCore,
    /// DCAT-AP (Data Catalog Vocabulary).
    DcatAp,
}
