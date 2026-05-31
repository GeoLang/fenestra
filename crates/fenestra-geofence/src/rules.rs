//! Access control rules and evaluation engine.

use serde::{Deserialize, Serialize};

use crate::spatial::SpatialConstraint;
use fenestra_core::RequestContext;

/// Effect of an access rule when it matches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuleEffect {
    /// Allow the request through.
    Allow,
    /// Deny the request entirely.
    Deny,
    /// Allow but clip results to the spatial constraint.
    Clip,
}

/// A single access control rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessRule {
    /// Rule priority (lower = higher priority).
    pub priority: u32,
    /// Which roles this rule applies to (empty = all).
    pub roles: Vec<String>,
    /// Which layers this rule applies to (empty = all).
    pub layers: Vec<String>,
    /// Which services this rule applies to (e.g., "WMS", "WFS").
    pub services: Vec<String>,
    /// Which operations (e.g., "GetMap", "GetFeature").
    pub operations: Vec<String>,
    /// Optional spatial constraint for the rule.
    pub spatial: Option<SpatialConstraint>,
    /// Effect when the rule matches.
    pub effect: RuleEffect,
}

/// Outcome of evaluating rules against a request.
#[derive(Debug, Clone)]
pub enum AccessDecision {
    Allow,
    Deny(String),
    Clip(SpatialConstraint),
}

/// In-memory store of access rules.
pub struct RuleStore {
    rules: Vec<AccessRule>,
}

impl RuleStore {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add(&mut self, rule: AccessRule) {
        self.rules.push(rule);
        self.rules.sort_by_key(|r| r.priority);
    }

    /// Evaluate all rules against a request context.
    /// First matching rule wins (lowest priority number).
    pub fn evaluate(&self, ctx: &RequestContext) -> AccessDecision {
        let user_roles: Vec<&str> = ctx
            .user
            .as_ref()
            .map(|u| u.roles.iter().map(|s| s.as_str()).collect())
            .unwrap_or_default();

        let layer = ctx
            .query_params
            .get("LAYERS")
            .or_else(|| ctx.query_params.get("layers"))
            .or_else(|| ctx.query_params.get("TYPENAME"))
            .or_else(|| ctx.query_params.get("typename"))
            .cloned()
            .unwrap_or_default();

        let service = ctx
            .query_params
            .get("SERVICE")
            .or_else(|| ctx.query_params.get("service"))
            .cloned()
            .unwrap_or_default();

        let request = ctx
            .query_params
            .get("REQUEST")
            .or_else(|| ctx.query_params.get("request"))
            .cloned()
            .unwrap_or_default();

        for rule in &self.rules {
            if !rule.roles.is_empty()
                && !rule.roles.iter().any(|r| user_roles.contains(&r.as_str()))
            {
                continue;
            }
            if !rule.layers.is_empty() && !rule.layers.iter().any(|l| layer.contains(l.as_str())) {
                continue;
            }
            if !rule.services.is_empty()
                && !rule
                    .services
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(&service))
            {
                continue;
            }
            if !rule.operations.is_empty()
                && !rule
                    .operations
                    .iter()
                    .any(|o| o.eq_ignore_ascii_case(&request))
            {
                continue;
            }

            // Rule matches
            return match &rule.effect {
                RuleEffect::Allow => AccessDecision::Allow,
                RuleEffect::Deny => AccessDecision::Deny(format!(
                    "Access denied by rule (priority {})",
                    rule.priority
                )),
                RuleEffect::Clip => {
                    if let Some(ref spatial) = rule.spatial {
                        AccessDecision::Clip(spatial.clone())
                    } else {
                        AccessDecision::Allow
                    }
                }
            };
        }

        // Default: allow if no rules match
        AccessDecision::Allow
    }
}

impl Default for RuleStore {
    fn default() -> Self {
        Self::new()
    }
}
