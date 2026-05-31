//! Fenestra INSPIRE/CSW Plugin
//!
//! Implements OGC Catalogue Service for the Web (CSW 2.0.2/3.0)
//! and INSPIRE metadata compliance for EU SDI requirements.
//!
//! ## Features
//! - CSW GetCapabilities, GetRecords, GetRecordById
//! - INSPIRE metadata validation (ISO 19115/19139)
//! - Metadata harvesting (push/pull)
//! - Dublin Core and ISO AP record formats
//! - INSPIRE Discovery Service conformance

mod csw;
mod harvest;
mod inspire;
mod records;

pub use csw::{CswRequest, CswResponse};
pub use harvest::HarvestConfig;
pub use inspire::InspireValidator;
pub use records::{MetadataRecord, RecordFormat};

use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;

use fenestra_core::{
    BoxFuture, HookOutcome, HookPhase, Plugin, PluginManifest, PluginResult, RequestContext,
};
use serde_json::Value;

/// INSPIRE/CSW plugin for Fenestra.
pub struct InspirePlugin {
    manifest: PluginManifest,
    #[allow(dead_code)]
    records: Arc<RwLock<Vec<MetadataRecord>>>,
}

impl InspirePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest {
                id: "fenestra.inspire".to_string(),
                name: "INSPIRE/CSW Catalogue".to_string(),
                version: "0.1.0".to_string(),
                description: "OGC CSW 2.0.2/3.0 and INSPIRE Discovery Service".to_string(),
                hooks: vec![HookPhase::PreRoute],
                dependencies: vec![],
            },
            records: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for InspirePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for InspirePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn on_load(&self, _config: Value) -> BoxFuture<'_, PluginResult<()>> {
        Box::pin(async { Ok(()) })
    }

    fn on_unload(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    fn on_hook(&self, _phase: HookPhase, ctx: RequestContext) -> BoxFuture<'_, HookOutcome> {
        Box::pin(async move {
            // Intercept CSW requests at /csw path
            if ctx.path.starts_with("/csw") {
                // TODO: Parse CSW request and dispatch to handler
            }
            HookOutcome::Continue(Box::new(ctx))
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
