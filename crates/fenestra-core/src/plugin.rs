//! Plugin system for extending Fenestra with server-side capabilities.
//!
//! Plugins can hook into request processing, add new endpoints,
//! provide data transformations, or implement access control.

use std::any::Any;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;

/// Result type for plugin operations.
pub type PluginResult<T> = Result<T, PluginError>;

/// Errors that can occur during plugin execution.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("access denied: {0}")]
    AccessDenied(String),
    #[error("plugin configuration error: {0}")]
    Config(String),
    #[error("plugin internal error: {0}")]
    Internal(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}

/// Lifecycle phase at which a plugin hooks into request processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookPhase {
    /// Before request routing — can modify or reject requests.
    PreRoute,
    /// After request parsing, before service execution.
    PreExecute,
    /// After service execution, before response serialization.
    PostExecute,
    /// After response serialization — can modify final output.
    PostResponse,
}

/// Metadata describing a plugin's capabilities and requirements.
#[derive(Debug, Clone)]
pub struct PluginManifest {
    /// Unique identifier for this plugin.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Version string (semver).
    pub version: String,
    /// Brief description of what the plugin does.
    pub description: String,
    /// Lifecycle phases this plugin hooks into.
    pub hooks: Vec<HookPhase>,
    /// Other plugin IDs this plugin depends on.
    pub dependencies: Vec<String>,
}

/// An incoming request context passed to plugin hooks.
#[derive(Debug, Clone)]
pub struct RequestContext {
    /// HTTP method.
    pub method: String,
    /// Request path.
    pub path: String,
    /// Query parameters.
    pub query_params: HashMap<String, String>,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// Authenticated user (if any).
    pub user: Option<UserIdentity>,
    /// Arbitrary metadata carried through the request pipeline.
    pub metadata: HashMap<String, Value>,
}

/// Authenticated user identity available to plugins.
#[derive(Debug, Clone)]
pub struct UserIdentity {
    pub id: String,
    pub username: String,
    pub roles: Vec<String>,
    pub attributes: HashMap<String, Value>,
}

/// Response context for post-execution hooks.
#[derive(Debug, Clone)]
pub struct ResponseContext {
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: HashMap<String, String>,
    /// Response body (may be empty for streaming responses).
    pub body: Vec<u8>,
    /// Content type.
    pub content_type: String,
}

/// Outcome of a plugin hook invocation.
#[derive(Debug)]
pub enum HookOutcome {
    /// Continue processing with potentially modified context.
    Continue(Box<RequestContext>),
    /// Short-circuit the pipeline with a response.
    Respond(ResponseContext),
    /// Reject the request with an error.
    Reject(PluginError),
}

/// Boxed future type for async plugin hooks.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Core trait that all Fenestra plugins must implement.
pub trait Plugin: Send + Sync + 'static {
    /// Returns the plugin manifest describing this plugin.
    fn manifest(&self) -> &PluginManifest;

    /// Called once when the plugin is loaded. Use for initialization.
    fn on_load(&self, config: Value) -> BoxFuture<'_, PluginResult<()>>;

    /// Called when the plugin is being unloaded. Use for cleanup.
    fn on_unload(&self) -> BoxFuture<'_, ()>;

    /// Hook into a specific lifecycle phase. Only called for phases
    /// declared in the manifest's `hooks` field.
    fn on_hook(&self, phase: HookPhase, ctx: RequestContext) -> BoxFuture<'_, HookOutcome>;

    /// Return this plugin as `Any` for downcasting to concrete types.
    fn as_any(&self) -> &dyn Any;
}

/// Registry that manages loaded plugins and dispatches hooks.
pub struct PluginRegistry {
    plugins: Vec<Arc<dyn Plugin>>,
}

impl PluginRegistry {
    /// Creates a new empty plugin registry.
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Registers a plugin. Calls `on_load` with the provided config.
    pub async fn register(&mut self, plugin: Arc<dyn Plugin>, config: Value) -> PluginResult<()> {
        plugin.on_load(config).await?;
        self.plugins.push(plugin);
        Ok(())
    }

    /// Unloads all plugins in reverse registration order.
    pub async fn unload_all(&mut self) {
        for plugin in self.plugins.iter().rev() {
            plugin.on_unload().await;
        }
        self.plugins.clear();
    }

    /// Runs all plugin hooks for a given phase, returning the final context
    /// or a short-circuit response.
    pub async fn run_hooks(
        &self,
        phase: HookPhase,
        mut ctx: RequestContext,
    ) -> Result<RequestContext, ResponseContext> {
        for plugin in &self.plugins {
            if !plugin.manifest().hooks.contains(&phase) {
                continue;
            }
            match plugin.on_hook(phase, ctx.clone()).await {
                HookOutcome::Continue(new_ctx) => ctx = *new_ctx,
                HookOutcome::Respond(resp) => return Err(resp),
                HookOutcome::Reject(err) => {
                    return Err(ResponseContext {
                        status: 403,
                        headers: HashMap::new(),
                        body: err.to_string().into_bytes(),
                        content_type: "text/plain".to_string(),
                    });
                }
            }
        }
        Ok(ctx)
    }

    /// Returns all registered plugins.
    pub fn plugins(&self) -> &[Arc<dyn Plugin>] {
        &self.plugins
    }

    /// Find a plugin by ID.
    pub fn get(&self, id: &str) -> Option<&Arc<dyn Plugin>> {
        self.plugins.iter().find(|p| p.manifest().id == id)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
