// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + skeleton stub.
//!
//! Live HTTP/API implementation deferred. The stub here returns one
//! canned [`Organization`] per Lookup so callers can wire Formations
//! against the surface today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::GithubError;
use crate::types::{OrgSlug, Organization};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GithubRequest {
    Lookup { identifier: OrgSlug },
}

impl FactPayload for GithubRequest {
    const FAMILY: &'static str = "embassy.github.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubResponse {
    pub records: Vec<Observation<Organization>>,
}

#[async_trait]
pub trait GithubProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &GithubRequest,
        ctx: &CallContext,
    ) -> Result<GithubResponse, GithubError>;
}

#[derive(Debug, Clone, Default)]
pub struct StubGithubProvider;

#[async_trait]
impl GithubProvider for StubGithubProvider {
    fn name(&self) -> &'static str {
        "stub_github"
    }

    async fn fetch(
        &self,
        request: &GithubRequest,
        _ctx: &CallContext,
    ) -> Result<GithubResponse, GithubError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| GithubError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let GithubRequest::Lookup { identifier } = request;
        let entity = Organization {
            login: identifier.clone(),
            html_url: format!("Stub Organization"),
        };

        let obs = Observation {
            observation_id: format!("obs:github:{request_hash}"),
            request_hash,
            vendor: "stub_github".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: None,
        };

        Ok(GithubResponse { records: vec![obs] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        // Intent: replay-from-audit contract — the recorded
        // request_hash must equal content_hash(canonical-JSON
        // request). Same load-bearing guarantee as every other port.
        let provider = StubGithubProvider;
        let req = GithubRequest::Lookup {
            identifier: OrgSlug::parse("STUB-001").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation() {
        let provider = StubGithubProvider;
        let req = GithubRequest::Lookup {
            identifier: OrgSlug::parse("STUB-001").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
