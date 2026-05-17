// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + deterministic stub.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::UsaspendingError;
use crate::types::{AwardId, FederalAward};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UsaspendingRequest {
    Lookup { award_id: AwardId },
}

impl FactPayload for UsaspendingRequest {
    const FAMILY: &'static str = "embassy.usaspending.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsaspendingResponse {
    pub records: Vec<Observation<FederalAward>>,
}

#[async_trait]
pub trait UsaspendingProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn lookup(
        &self,
        request: &UsaspendingRequest,
        ctx: &CallContext,
    ) -> Result<UsaspendingResponse, UsaspendingError>;
}

/// Deterministic stub. Returns one canned [`FederalAward`] per Lookup.
#[derive(Debug, Clone, Default)]
pub struct StubUsaspendingProvider;

#[async_trait]
impl UsaspendingProvider for StubUsaspendingProvider {
    fn name(&self) -> &'static str {
        "stub_usaspending"
    }

    async fn lookup(
        &self,
        request: &UsaspendingRequest,
        _ctx: &CallContext,
    ) -> Result<UsaspendingResponse, UsaspendingError> {
        let hash_input = serde_json::to_string(request).map_err(|e| {
            UsaspendingError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&hash_input);

        let UsaspendingRequest::Lookup { award_id } = request;
        let award = FederalAward {
            award_id: award_id.clone(),
            recipient_name: "Stub Federal Contractor LLC".to_string(),
            total_obligated_micros: 1_000_000_000_000,
            awarding_agency: "Department of Stub".to_string(),
            period_of_performance_start: Some("2026-01-01".to_string()),
            period_of_performance_end: Some("2027-01-01".to_string()),
        };

        let obs = Observation {
            observation_id: format!("obs:usaspending:{request_hash}"),
            request_hash,
            vendor: "stub_usaspending".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: award,
            raw_response: None,
        };

        Ok(UsaspendingResponse { records: vec![obs] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        let provider = StubUsaspendingProvider;
        let req = UsaspendingRequest::Lookup {
            award_id: AwardId::parse("CONT_AWD_STUB_001").unwrap(),
        };
        let resp = provider
            .lookup(&req, &CallContext::default())
            .await
            .unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation_per_lookup() {
        let provider = StubUsaspendingProvider;
        let req = UsaspendingRequest::Lookup {
            award_id: AwardId::parse("CONT_AWD_STUB_001").unwrap(),
        };
        let resp = provider
            .lookup(&req, &CallContext::default())
            .await
            .unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
