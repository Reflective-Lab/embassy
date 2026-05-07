// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Shared contract surface for embassy ports.
//!
//! Every port in `embassy/*` emits provenanced observations and accepts a
//! call context. These types are the cross-port contract — independent of
//! any specific external service.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct CallContext {
    pub correlation_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation<T> {
    pub observation_id: String,
    pub request_hash: String,
    pub vendor: String,
    pub model: String,
    pub latency_ms: u64,
    pub cost_estimate: Option<f64>,
    pub tokens: Option<u64>,
    pub content: T,
    pub raw_response: Option<String>,
}

pub fn content_hash(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_is_deterministic() {
        assert_eq!(content_hash("hello"), content_hash("hello"));
    }

    #[test]
    fn content_hash_differs_for_distinct_inputs() {
        assert_ne!(content_hash("hello"), content_hash("world"));
    }

    #[test]
    fn content_hash_is_sixteen_hex_chars() {
        let h = content_hash("anything");
        assert_eq!(h.len(), 16);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn call_context_default_is_empty() {
        let ctx = CallContext::default();
        assert!(ctx.correlation_id.is_none());
        assert!(ctx.metadata.is_empty());
    }

    #[test]
    fn call_context_carries_metadata() {
        let mut ctx = CallContext {
            correlation_id: Some("trace-1".to_string()),
            metadata: HashMap::new(),
        };
        ctx.metadata.insert("k".into(), "v".into());
        assert_eq!(ctx.correlation_id.as_deref(), Some("trace-1"));
        assert_eq!(ctx.metadata.get("k").map(String::as_str), Some("v"));
    }

    #[test]
    fn observation_round_trips_through_json() {
        let obs = Observation {
            observation_id: "obs:1".to_string(),
            request_hash: content_hash("req"),
            vendor: "vendor".to_string(),
            model: "model".to_string(),
            latency_ms: 42,
            cost_estimate: Some(0.5),
            tokens: Some(7),
            content: 99u32,
            raw_response: Some("{}".to_string()),
        };
        let json = serde_json::to_string(&obs).expect("serialize");
        let back: Observation<u32> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.observation_id, obs.observation_id);
        assert_eq!(back.request_hash, obs.request_hash);
        assert_eq!(back.vendor, obs.vendor);
        assert_eq!(back.model, obs.model);
        assert_eq!(back.latency_ms, obs.latency_ms);
        assert_eq!(back.cost_estimate, obs.cost_estimate);
        assert_eq!(back.tokens, obs.tokens);
        assert_eq!(back.content, obs.content);
        assert_eq!(back.raw_response, obs.raw_response);
    }
}
