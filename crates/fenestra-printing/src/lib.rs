//! Fenestra Printing Plugin
//!
//! High-quality cartographic map printing to PDF, PNG, and SVG.
//! Equivalent to MapFish Print / GeoServer printing module.
//!
//! ## Features
//! - Template-based layouts (title, legend, scale bar, north arrow, map)
//! - Multi-page atlas generation
//! - DPI-aware rendering (72, 150, 300 dpi)
//! - PDF output with vector layers preserved
//! - Legend auto-generation from SLD styles
//! - Overview map inset

mod layout;
mod pdf;
mod render;

pub use layout::{LayoutTemplate, PageSize, PrintElement};
pub use render::{PrintJob, PrintOutput, PrintRequest};

use std::any::Any;

use fenestra_core::{
    BoxFuture, HookOutcome, HookPhase, Plugin, PluginManifest, PluginResult, RequestContext,
};
use serde_json::Value;

/// Map printing plugin for Fenestra.
pub struct PrintingPlugin {
    manifest: PluginManifest,
}

impl PrintingPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest {
                id: "fenestra.printing".to_string(),
                name: "Map Printing".to_string(),
                version: "0.1.0".to_string(),
                description: "High-quality PDF/PNG map printing with templates".to_string(),
                hooks: vec![HookPhase::PreRoute],
                dependencies: vec![],
            },
        }
    }
}

impl Default for PrintingPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for PrintingPlugin {
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
            // Intercept print requests at /print path
            if ctx.path.starts_with("/print") {
                // TODO: Parse print request and generate PDF/PNG
            }
            HookOutcome::Continue(Box::new(ctx))
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
