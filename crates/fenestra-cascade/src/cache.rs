//! Response caching for cascaded services.

#![allow(dead_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Cache entry for a proxied response.
struct CacheEntry {
    data: Vec<u8>,
    content_type: String,
    created: Instant,
    ttl: Duration,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        self.created.elapsed() > self.ttl
    }
}

/// Simple in-memory cache for upstream responses.
pub struct ResponseCache {
    entries: HashMap<String, CacheEntry>,
    max_size_bytes: usize,
    current_size: usize,
}

impl ResponseCache {
    pub fn new(max_size_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_size_bytes,
            current_size: 0,
        }
    }

    /// Get a cached response if it exists and hasn't expired.
    pub fn get(&self, key: &str) -> Option<(&[u8], &str)> {
        self.entries.get(key).and_then(|entry| {
            if entry.is_expired() {
                None
            } else {
                Some((entry.data.as_slice(), entry.content_type.as_str()))
            }
        })
    }

    /// Store a response in the cache.
    pub fn put(&mut self, key: String, data: Vec<u8>, content_type: String, ttl: Duration) {
        let size = data.len();

        // Evict expired entries first
        self.evict_expired();

        // If still too large, don't cache (avoid eviction storms)
        if self.current_size + size > self.max_size_bytes {
            return;
        }

        self.current_size += size;
        self.entries.insert(
            key,
            CacheEntry {
                data,
                content_type,
                created: Instant::now(),
                ttl,
            },
        );
    }

    fn evict_expired(&mut self) {
        let expired_keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, v)| v.is_expired())
            .map(|(k, _)| k.clone())
            .collect();

        for key in expired_keys {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_size -= entry.data.len();
            }
        }
    }
}
