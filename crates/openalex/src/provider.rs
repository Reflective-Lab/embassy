// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + skeleton stub.
//!
//! Live HTTP/API implementation deferred. The stub here returns one
//! canned [`Work`] per Lookup so callers can wire Formations
//! against the surface today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::OpenAlexError;
use crate::types::{OpenAlexId, Work};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpenAlexRequest {
    Lookup { identifier: OpenAlexId },
}

impl FactPayload for OpenAlexRequest {
    const FAMILY: &'static str = "embassy.openalex.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAlexResponse {
    pub records: Vec<Observation<Work>>,
}

#[async_trait]
pub trait OpenAlexProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &OpenAlexRequest,
        ctx: &CallContext,
    ) -> Result<OpenAlexResponse, OpenAlexError>;
}

#[derive(Debug, Clone, Default)]
pub struct StubOpenAlexProvider;

#[async_trait]
impl OpenAlexProvider for StubOpenAlexProvider {
    fn name(&self) -> &'static str {
        "stub_openalex"
    }

    async fn fetch(
        &self,
        request: &OpenAlexRequest,
        _ctx: &CallContext,
    ) -> Result<OpenAlexResponse, OpenAlexError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| OpenAlexError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let OpenAlexRequest::Lookup { identifier } = request;
        let entity = Work {
            id: identifier.clone(),
            title: "Stub Work".to_string(),
        };

        let obs = Observation {
            observation_id: format!("obs:openalex:{request_hash}"),
            request_hash,
            vendor: "stub_openalex".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: None,
        };

        Ok(OpenAlexResponse { records: vec![obs] })
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
        let provider = StubOpenAlexProvider;
        let req = OpenAlexRequest::Lookup {
            identifier: OpenAlexId::parse("W2741809807").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation() {
        let provider = StubOpenAlexProvider;
        let req = OpenAlexRequest::Lookup {
            identifier: OpenAlexId::parse("W2741809807").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
