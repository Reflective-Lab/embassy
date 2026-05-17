// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + skeleton stub.
//!
//! Live HTTP/API implementation deferred. The stub here returns one
//! canned [`Entity`] per Lookup so callers can wire Formations
//! against the surface today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::WikidataError;
use crate::types::{Entity, QId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WikidataRequest {
    Lookup { identifier: QId },
}

impl FactPayload for WikidataRequest {
    const FAMILY: &'static str = "embassy.wikidata.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikidataResponse {
    pub records: Vec<Observation<Entity>>,
}

#[async_trait]
pub trait WikidataProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &WikidataRequest,
        ctx: &CallContext,
    ) -> Result<WikidataResponse, WikidataError>;
}

#[derive(Debug, Clone, Default)]
pub struct StubWikidataProvider;

#[async_trait]
impl WikidataProvider for StubWikidataProvider {
    fn name(&self) -> &'static str {
        "stub_wikidata"
    }

    async fn fetch(
        &self,
        request: &WikidataRequest,
        _ctx: &CallContext,
    ) -> Result<WikidataResponse, WikidataError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| WikidataError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let WikidataRequest::Lookup { identifier } = request;
        let entity = Entity {
            qid: identifier.clone(),
            label: format!("Stub Entity"),
        };

        let obs = Observation {
            observation_id: format!("obs:wikidata:{request_hash}"),
            request_hash,
            vendor: "stub_wikidata".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: None,
        };

        Ok(WikidataResponse { records: vec![obs] })
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
        let provider = StubWikidataProvider;
        let req = WikidataRequest::Lookup {
            identifier: QId::parse("STUB-001").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation() {
        let provider = StubWikidataProvider;
        let req = WikidataRequest::Lookup {
            identifier: QId::parse("STUB-001").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
