//! Fenestra Cascade Plugin
//!
//! Proxies remote WMS/WMTS/WFS services through the local Fenestra instance.
//! Adds caching, reprojection, and unified capability documents.
//!
//! ## Features
//! - Proxy remote WMS layers as local layers
//! - Proxy remote WMTS tile services
//! - Response caching with TTL
//! - On-the-fly reprojection between source and target CRS
//! - Merged capabilities documents (local + remote layers)
//! - Health monitoring of upstream services
//! - Fallback/failover between multiple upstream sources

mod cache;
mod proxy;
mod upstream;

pub use proxy::CascadeProxy;
pub use upstream::{UpstreamHealth, UpstreamService, UpstreamType};

use std::any::Any;
use std::sync::Arc;
use tokio::sync::RwLock;

use fenestra_core::{
    BoxFuture, HookOutcome, HookPhase, Plugin, PluginManifest, PluginResult, RequestContext,
};
use serde_json::Value;

/// Cascading WMS/WMTS proxy plugin.
pub struct CascadePlugin {
    manifest: PluginManifest,
    upstreams: Arc<RwLock<Vec<UpstreamService>>>,
}

impl CascadePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest {
                id: "fenestra.cascade".to_string(),
                name: "Cascading WMS/WMTS".to_string(),
                version: "0.1.0".to_string(),
                description: "Proxy and cache remote OGC services".to_string(),
                hooks: vec![HookPhase::PreExecute],
                dependencies: vec![],
            },
            upstreams: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl Default for CascadePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for CascadePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn on_load(&self, config: Value) -> BoxFuture<'_, PluginResult<()>> {
        Box::pin(async move {
            if let Some(services) = config.get("upstreams").and_then(|v| v.as_array()) {
                let mut upstreams = self.upstreams.write().await;
                for svc_val in services {
                    if let Ok(svc) = serde_json::from_value::<UpstreamService>(svc_val.clone()) {
                        upstreams.push(svc);
                    }
                }
            }
            Ok(())
        })
    }

    fn on_unload(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    fn on_hook(&self, _phase: HookPhase, ctx: RequestContext) -> BoxFuture<'_, HookOutcome> {
        let upstreams = self.upstreams.clone();
        Box::pin(async move {
            // Check if the requested layer is a cascaded layer
            let layer = ctx
                .query_params
                .get("LAYERS")
                .or_else(|| ctx.query_params.get("layers"))
                .cloned()
                .unwrap_or_default();

            let upstreams = upstreams.read().await;
            let matching = upstreams.iter().find(|u| {
                u.layer_mappings
                    .iter()
                    .any(|(local, _remote)| local == &layer)
            });

            if let Some(_upstream) = matching {
                // TODO: Proxy the request to the upstream service
                // For now, pass through
            }

            HookOutcome::Continue(Box::new(ctx))
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
