//! CSW request/response handling.

use serde::{Deserialize, Serialize};

/// CSW request types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CswRequest {
    GetCapabilities,
    GetRecords {
        start_position: u32,
        max_records: u32,
        query: Option<String>,
        output_format: RecordOutputFormat,
    },
    GetRecordById {
        id: String,
        output_format: RecordOutputFormat,
    },
    DescribeRecord,
    Transaction(CswTransaction),
}

/// CSW transaction types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CswTransaction {
    Insert(String),
    Update { id: String, record: String },
    Delete { id: String },
}

/// Output format for CSW responses.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum RecordOutputFormat {
    #[default]
    DublinCore,
    IsoAp,
    Full,
}

/// CSW response types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CswResponse {
    Capabilities(String),
    Records {
        total: u32,
        next_record: u32,
        records: Vec<String>,
    },
    Record(String),
    Acknowledgement,
}
