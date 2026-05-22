// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + skeleton stub.
//!
//! Live HTTP/API implementation deferred. The stub here returns one
//! canned [`Article`] per Lookup so callers can wire Formations
//! against the surface today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::PubmedError;
use crate::types::{Article, Pmid};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PubmedRequest {
    Lookup { identifier: Pmid },
}

impl FactPayload for PubmedRequest {
    const FAMILY: &'static str = "embassy.pubmed.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubmedResponse {
    pub records: Vec<Observation<Article>>,
}

#[async_trait]
pub trait PubmedProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &PubmedRequest,
        ctx: &CallContext,
    ) -> Result<PubmedResponse, PubmedError>;
}

#[derive(Debug, Clone, Default)]
pub struct StubPubmedProvider;

#[async_trait]
impl PubmedProvider for StubPubmedProvider {
    fn name(&self) -> &'static str {
        "stub_pubmed"
    }

    async fn fetch(
        &self,
        request: &PubmedRequest,
        _ctx: &CallContext,
    ) -> Result<PubmedResponse, PubmedError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| PubmedError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let PubmedRequest::Lookup { identifier } = request;
        let entity = Article {
            pmid: identifier.clone(),
            title: "Stub Article".to_string(),
        };

        let obs = Observation {
            observation_id: format!("obs:pubmed:{request_hash}"),
            request_hash,
            vendor: "stub_pubmed".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: None,
        };

        Ok(PubmedResponse { records: vec![obs] })
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
        let provider = StubPubmedProvider;
        let req = PubmedRequest::Lookup {
            identifier: Pmid::parse("38765432").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation() {
        let provider = StubPubmedProvider;
        let req = PubmedRequest::Lookup {
            identifier: Pmid::parse("38765432").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
