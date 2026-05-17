// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + deterministic stub.
//!
//! Real provider: EU FSF endpoint (XML feed). Stub here returns
//! synthetic hits driven by the queried name.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::EuSanctionsError;
use crate::types::{MatchType, SanctionsHit, SanctionsSubject, SubjectType};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EuSanctionsRequest {
    Screen { subject: SanctionsSubject },
}

impl FactPayload for EuSanctionsRequest {
    const FAMILY: &'static str = "embassy.eu_sanctions.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EuSanctionsResponse {
    pub records: Vec<Observation<SanctionsHit>>,
}

#[async_trait]
pub trait EuSanctionsProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn screen(
        &self,
        request: &EuSanctionsRequest,
        ctx: &CallContext,
    ) -> Result<EuSanctionsResponse, EuSanctionsError>;
}

/// Deterministic stub.
///
/// Stub rule: a subject whose name contains "BLOCKED" returns one
/// synthetic hit; otherwise empty (clean screen).
#[derive(Debug, Clone, Default)]
pub struct StubEuSanctionsProvider;

#[async_trait]
impl EuSanctionsProvider for StubEuSanctionsProvider {
    fn name(&self) -> &'static str {
        "stub_eu_sanctions"
    }

    async fn screen(
        &self,
        request: &EuSanctionsRequest,
        _ctx: &CallContext,
    ) -> Result<EuSanctionsResponse, EuSanctionsError> {
        let hash_input = serde_json::to_string(request).map_err(|e| {
            EuSanctionsError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&hash_input);

        let EuSanctionsRequest::Screen { subject } = request;
        let mut records = Vec::new();
        if subject.name.to_ascii_uppercase().contains("BLOCKED") {
            let hit = SanctionsHit {
                subject_name: subject.name.clone(),
                match_score: 0.98,
                match_type: MatchType::Exact,
                subject_type: SubjectType::Entity,
                list_name: "EU Consolidated".to_string(),
                list_program: Some("(EU) STUB/2026".to_string()),
                listed_at: Some("2026-01-01".to_string()),
                aliases: Vec::new(),
                jurisdictions: vec!["EU".to_string()],
            };
            records.push(Observation {
                observation_id: format!("obs:eu_sanctions:{request_hash}"),
                request_hash: request_hash.clone(),
                vendor: "stub_eu_sanctions".to_string(),
                model: "stub".to_string(),
                latency_ms: 5,
                cost_estimate: None,
                tokens: None,
                content: hit,
                raw_response: None,
            });
        }

        Ok(EuSanctionsResponse { records })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_returns_hit_for_blocked_subject() {
        // Intent: same trigger ("BLOCKED" in name) across the sanctions
        // trio so Formation-level tests can rely on uniform stub
        // behaviour.
        let provider = StubEuSanctionsProvider;
        let req = EuSanctionsRequest::Screen {
            subject: SanctionsSubject::parse("BLOCKED Holdings AB").unwrap(),
        };
        let resp = provider
            .screen(&req, &CallContext::default())
            .await
            .unwrap();
        assert_eq!(resp.records.len(), 1);
    }

    #[tokio::test]
    async fn stub_returns_empty_for_clean_subject() {
        // Intent: clean screen produces zero records — the no-hit
        // contract.
        let provider = StubEuSanctionsProvider;
        let req = EuSanctionsRequest::Screen {
            subject: SanctionsSubject::parse("Volvo AB").unwrap(),
        };
        let resp = provider
            .screen(&req, &CallContext::default())
            .await
            .unwrap();
        assert!(resp.records.is_empty());
    }

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        // Intent: replay-from-audit contract.
        let provider = StubEuSanctionsProvider;
        let req = EuSanctionsRequest::Screen {
            subject: SanctionsSubject::parse("BLOCKED Holdings AB").unwrap(),
        };
        let resp = provider
            .screen(&req, &CallContext::default())
            .await
            .unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }
}
