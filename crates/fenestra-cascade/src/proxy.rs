//! Proxy logic for forwarding requests to upstream services.

use crate::upstream::UpstreamService;

/// Handles proxying OGC requests to upstream services.
pub struct CascadeProxy;

impl CascadeProxy {
    /// Rewrites a local layer request into an upstream request URL.
    pub fn rewrite_url(
        upstream: &UpstreamService,
        local_layer: &str,
        params: &[(String, String)],
    ) -> Option<String> {
        let remote_layer = upstream
            .layer_mappings
            .iter()
            .find(|(local, _)| local == local_layer)
            .map(|(_, remote)| remote.as_str())?;

        let mut url = format!("{}?", upstream.url);
        for (key, value) in params {
            if key.eq_ignore_ascii_case("LAYERS") || key.eq_ignore_ascii_case("TYPENAME") {
                url.push_str(&format!("{key}={remote_layer}&"));
            } else {
                url.push_str(&format!("{key}={value}&"));
            }
        }
        url.pop(); // trailing &
        Some(url)
    }
}
