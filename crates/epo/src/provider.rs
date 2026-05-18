// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + skeleton stub.
//!
//! Live HTTP/API implementation deferred. The stub here returns one
//! canned [`Patent`] per Lookup so callers can wire Formations
//! against the surface today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::EpoError;
use crate::types::{EpoNumber, Patent};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EpoRequest {
    Lookup { identifier: EpoNumber },
}

impl FactPayload for EpoRequest {
    const FAMILY: &'static str = "embassy.epo.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpoResponse {
    pub records: Vec<Observation<Patent>>,
}

#[async_trait]
pub trait EpoProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(&self, request: &EpoRequest, ctx: &CallContext)
    -> Result<EpoResponse, EpoError>;
}

#[derive(Debug, Clone, Default)]
pub struct StubEpoProvider;

#[async_trait]
impl EpoProvider for StubEpoProvider {
    fn name(&self) -> &'static str {
        "stub_epo"
    }

    async fn fetch(
        &self,
        request: &EpoRequest,
        _ctx: &CallContext,
    ) -> Result<EpoResponse, EpoError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| EpoError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let EpoRequest::Lookup { identifier } = request;
        let entity = Patent {
            publication_number: identifier.clone(),
            title: "Stub Patent".to_string(),
        };

        let obs = Observation {
            observation_id: format!("obs:epo:{request_hash}"),
            request_hash,
            vendor: "stub_epo".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: None,
        };

        Ok(EpoResponse { records: vec![obs] })
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
        let provider = StubEpoProvider;
        let req = EpoRequest::Lookup {
            identifier: EpoNumber::parse("EP1234567A1").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation() {
        let provider = StubEpoProvider;
        let req = EpoRequest::Lookup {
            identifier: EpoNumber::parse("EP1234567A1").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
