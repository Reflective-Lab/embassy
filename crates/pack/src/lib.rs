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
