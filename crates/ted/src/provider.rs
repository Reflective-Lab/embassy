// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + deterministic stub.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::TedError;
use crate::types::{ProcurementNotice, ProcurementType, TedNoticeId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TedRequest {
    Lookup { notice_id: TedNoticeId },
}

impl FactPayload for TedRequest {
    const FAMILY: &'static str = "embassy.ted.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TedResponse {
    pub records: Vec<Observation<ProcurementNotice>>,
}

#[async_trait]
pub trait TedProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn lookup(
        &self,
        request: &TedRequest,
        ctx: &CallContext,
    ) -> Result<TedResponse, TedError>;
}

/// Deterministic stub. Returns one canned [`ProcurementNotice`] per
/// Lookup.
#[derive(Debug, Clone, Default)]
pub struct StubTedProvider;

#[async_trait]
impl TedProvider for StubTedProvider {
    fn name(&self) -> &'static str {
        "stub_ted"
    }

    async fn lookup(
        &self,
        request: &TedRequest,
        _ctx: &CallContext,
    ) -> Result<TedResponse, TedError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| TedError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let TedRequest::Lookup { notice_id } = request;
        let notice = ProcurementNotice {
            notice_id: notice_id.clone(),
            contracting_authority: "Stockholm Stad (stub)".to_string(),
            title: format!("Stub procurement {}", notice_id.as_str()),
            country: "SE".to_string(),
            procurement_type: ProcurementType::ContractNotice,
            deadline: Some("2026-12-31T23:59:59Z".to_string()),
        };

        let obs = Observation {
            observation_id: format!("obs:ted:{request_hash}"),
            request_hash,
            vendor: "stub_ted".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: notice,
            raw_response: None,
        };

        Ok(TedResponse { records: vec![obs] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        let provider = StubTedProvider;
        let req = TedRequest::Lookup {
            notice_id: TedNoticeId::parse("123456-2026").unwrap(),
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
        let provider = StubTedProvider;
        let req = TedRequest::Lookup {
            notice_id: TedNoticeId::parse("123456-2026").unwrap(),
        };
        let resp = provider
            .lookup(&req, &CallContext::default())
            .await
            .unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
