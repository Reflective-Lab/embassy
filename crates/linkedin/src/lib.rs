// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! LinkedIn port — professional network research.
//!
//! Extracted from `organism/crates/intelligence/src/linkedin.rs` on
//! 2026-05-05 as the first port to land in embassy. LinkedIn is a
//! sovereign integration: its identity is part of the contract (ToS, rate
//! limits, vendor laws), so it cannot be abstracted behind a generic
//! interface.

use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedInGetRequest {
    pub endpoint: String,
    pub query: HashMap<String, String>,
}

impl LinkedInGetRequest {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            query: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedInProfile {
    pub profile_id: String,
    pub name: String,
    pub title: Option<String>,
    pub company: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedInGetResponse {
    pub records: Vec<Observation<LinkedInProfile>>,
}

pub trait LinkedInProvider: Send + Sync {
    fn name(&self) -> &str;
    fn get(
        &self,
        request: &LinkedInGetRequest,
        ctx: &CallContext,
    ) -> Result<LinkedInGetResponse, String>;
}

#[derive(Debug, Clone, Default)]
pub struct StubLinkedInProvider;

impl LinkedInProvider for StubLinkedInProvider {
    fn name(&self) -> &'static str {
        "stub_linkedin"
    }

    fn get(
        &self,
        request: &LinkedInGetRequest,
        _ctx: &CallContext,
    ) -> Result<LinkedInGetResponse, String> {
        if request.endpoint.trim().is_empty() {
            return Err("Empty endpoint".to_string());
        }
        let hash_input = format!("{}:{:?}", request.endpoint, request.query);
        let obs = Observation {
            observation_id: format!("obs:linkedin:{}", content_hash(&hash_input)),
            request_hash: content_hash(&hash_input),
            vendor: "stub_linkedin".to_string(),
            model: "stub".to_string(),
            latency_ms: 10,
            cost_estimate: None,
            tokens: None,
            content: LinkedInProfile {
                profile_id: "LI-STUB-001".to_string(),
                name: "Jane Doe".to_string(),
                title: Some("VP Engineering".to_string()),
                company: Some("Acme Corp".to_string()),
                payload: serde_json::json!({"name": "Jane Doe"}),
            },
            raw_response: None,
        };
        Ok(LinkedInGetResponse { records: vec![obs] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_new_starts_with_empty_query() {
        let req = LinkedInGetRequest::new("/v2/people");
        assert_eq!(req.endpoint, "/v2/people");
        assert!(req.query.is_empty());
    }

    #[test]
    fn stub_provider_name_is_stable() {
        let provider = StubLinkedInProvider;
        assert_eq!(provider.name(), "stub_linkedin");
    }

    #[test]
    fn stub_provider_returns_one_observation() {
        let provider = StubLinkedInProvider;
        let mut req = LinkedInGetRequest::new("/v2/people");
        req.query.insert("q".into(), "engineer".into());
        let ctx = CallContext::default();

        let response = provider.get(&req, &ctx).expect("ok");
        assert_eq!(response.records.len(), 1);

        let obs = &response.records[0];
        assert_eq!(obs.vendor, "stub_linkedin");
        assert_eq!(obs.model, "stub");
        assert_eq!(obs.content.profile_id, "LI-STUB-001");
        assert_eq!(obs.content.name, "Jane Doe");
        assert_eq!(obs.content.title.as_deref(), Some("VP Engineering"));
        assert_eq!(obs.content.company.as_deref(), Some("Acme Corp"));
        assert!(obs.observation_id.starts_with("obs:linkedin:"));
        assert_eq!(obs.observation_id.len(), "obs:linkedin:".len() + 16);
    }

    #[test]
    fn stub_provider_rejects_empty_endpoint() {
        let provider = StubLinkedInProvider;
        let req = LinkedInGetRequest::new("   ");
        let ctx = CallContext::default();
        let err = provider.get(&req, &ctx).expect_err("must reject");
        assert_eq!(err, "Empty endpoint");
    }

    #[test]
    fn stub_provider_is_deterministic_per_request() {
        let provider = StubLinkedInProvider;
        let req = LinkedInGetRequest::new("/v2/people");
        let ctx = CallContext::default();
        let a = provider.get(&req, &ctx).expect("ok");
        let b = provider.get(&req, &ctx).expect("ok");
        assert_eq!(a.records[0].request_hash, b.records[0].request_hash);
        assert_eq!(a.records[0].observation_id, b.records[0].observation_id);
    }

    #[test]
    fn profile_payload_round_trips() {
        let req = LinkedInGetRequest::new("/v2/people");
        let provider = StubLinkedInProvider;
        let ctx = CallContext::default();
        let response = provider.get(&req, &ctx).expect("ok");
        let json = serde_json::to_string(&response).expect("serialize");
        let back: LinkedInGetResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.records.len(), 1);
        assert_eq!(back.records[0].content.name, "Jane Doe");
    }
}
