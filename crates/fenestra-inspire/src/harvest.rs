//! Metadata harvesting — periodic fetch from remote CSW endpoints.

use serde::{Deserialize, Serialize};

/// Configuration for metadata harvesting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarvestConfig {
    /// Remote CSW endpoint URL.
    pub source_url: String,
    /// Harvest interval in seconds (0 = one-shot).
    pub interval_secs: u64,
    /// Maximum records to fetch per harvest cycle.
    pub max_records: u32,
    /// Filter expression (CQL or OGC Filter).
    pub filter: Option<String>,
}
