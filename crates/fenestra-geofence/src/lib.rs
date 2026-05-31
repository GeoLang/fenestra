//! Fenestra GeoFence Plugin
//!
//! Provides fine-grained spatial access control for geospatial services.
//! Rules can restrict access based on:
//! - Layer name
//! - Spatial extent (bounding box / polygon)
//! - User role / attribute
//! - Request type (WMS GetMap, WFS GetFeature, etc.)
//! - Time of day / IP range
//!
//! ## How it works
//! The GeoFence plugin intercepts requests at the `PreExecute` phase.
//! It evaluates the request against the configured access rules and either
//! allows, denies, or spatially clips the response.

mod rules;
mod spatial;

pub use rules::{AccessDecision, AccessRule, RuleEffect, RuleStore};
pub use spatial::SpatialConstraint;

use std::any::Any;

use fenestra_core::{
    BoxFuture, HookOutcome, HookPhase, Plugin, PluginError, PluginManifest, PluginResult,
    RequestContext,
};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

/// GeoFence spatial access control plugin.
pub struct GeofencePlugin {
    manifest: PluginManifest,
    rules: Arc<RwLock<RuleStore>>,
}

impl GeofencePlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest {
                id: "fenestra.geofence".to_string(),
                name: "GeoFence Spatial Access Control".to_string(),
                version: "0.1.0".to_string(),
                description: "Fine-grained spatial authorization per layer/user/role".to_string(),
                hooks: vec![HookPhase::PreExecute],
                dependencies: vec![],
            },
            rules: Arc::new(RwLock::new(RuleStore::new())),
        }
    }
}

impl Default for GeofencePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for GeofencePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn on_load(&self, config: Value) -> BoxFuture<'_, PluginResult<()>> {
        Box::pin(async move {
            // Load rules from config
            if let Some(rules_array) = config.get("rules").and_then(|v| v.as_array()) {
                let mut store = self.rules.write().await;
                for rule_val in rules_array {
                    if let Ok(rule) = serde_json::from_value::<AccessRule>(rule_val.clone()) {
                        store.add(rule);
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
        let rules = self.rules.clone();
        Box::pin(async move {
            let store = rules.read().await;
            match store.evaluate(&ctx) {
                AccessDecision::Allow => HookOutcome::Continue(Box::new(ctx)),
                AccessDecision::Deny(reason) => {
                    HookOutcome::Reject(PluginError::AccessDenied(reason))
                }
                AccessDecision::Clip(_constraint) => {
                    // TODO: Modify request bbox to intersection with allowed area
                    HookOutcome::Continue(Box::new(ctx))
                }
            }
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
