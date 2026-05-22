// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + skeleton stub.
//!
//! Live HTTP/API implementation deferred. The stub here returns one
//! canned [`Paper`] per Lookup so callers can wire Formations
//! against the surface today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::ArxivError;
use crate::types::{ArxivId, Paper};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArxivRequest {
    Lookup { identifier: ArxivId },
}

impl FactPayload for ArxivRequest {
    const FAMILY: &'static str = "embassy.arxiv.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArxivResponse {
    pub records: Vec<Observation<Paper>>,
}

#[async_trait]
pub trait ArxivProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &ArxivRequest,
        ctx: &CallContext,
    ) -> Result<ArxivResponse, ArxivError>;
}

#[derive(Debug, Clone, Default)]
pub struct StubArxivProvider;

#[async_trait]
impl ArxivProvider for StubArxivProvider {
    fn name(&self) -> &'static str {
        "stub_arxiv"
    }

    async fn fetch(
        &self,
        request: &ArxivRequest,
        _ctx: &CallContext,
    ) -> Result<ArxivResponse, ArxivError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| ArxivError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let ArxivRequest::Lookup { identifier } = request;
        let entity = Paper {
            arxiv_id: identifier.clone(),
            title: "Stub Paper".to_string(),
        };

        let obs = Observation {
            observation_id: format!("obs:arxiv:{request_hash}"),
            request_hash,
            vendor: "stub_arxiv".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: None,
        };

        Ok(ArxivResponse { records: vec![obs] })
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
        let provider = StubArxivProvider;
        let req = ArxivRequest::Lookup {
            identifier: ArxivId::parse("2301.00001").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation() {
        let provider = StubArxivProvider;
        let req = ArxivRequest::Lookup {
            identifier: ArxivId::parse("2301.00001").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
