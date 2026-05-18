// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + skeleton stub.
//!
//! Live HTTP/API implementation deferred. The stub here returns one
//! canned [`TableMetadata`] per Lookup so callers can wire Formations
//! against the surface today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::ScbError;
use crate::types::{TableId, TableMetadata};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScbRequest {
    Lookup { identifier: TableId },
}

impl FactPayload for ScbRequest {
    const FAMILY: &'static str = "embassy.scb.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScbResponse {
    pub records: Vec<Observation<TableMetadata>>,
}

#[async_trait]
pub trait ScbProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(&self, request: &ScbRequest, ctx: &CallContext)
    -> Result<ScbResponse, ScbError>;
}

#[derive(Debug, Clone, Default)]
pub struct StubScbProvider;

#[async_trait]
impl ScbProvider for StubScbProvider {
    fn name(&self) -> &'static str {
        "stub_scb"
    }

    async fn fetch(
        &self,
        request: &ScbRequest,
        _ctx: &CallContext,
    ) -> Result<ScbResponse, ScbError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| ScbError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let ScbRequest::Lookup { identifier } = request;
        let entity = TableMetadata {
            table_id: identifier.clone(),
            title: "Stub TableMetadata".to_string(),
        };

        let obs = Observation {
            observation_id: format!("obs:scb:{request_hash}"),
            request_hash,
            vendor: "stub_scb".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: None,
        };

        Ok(ScbResponse { records: vec![obs] })
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
        let provider = StubScbProvider;
        let req = ScbRequest::Lookup {
            identifier: TableId::parse("BE0101A1").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation() {
        let provider = StubScbProvider;
        let req = ScbRequest::Lookup {
            identifier: TableId::parse("BE0101A1").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
