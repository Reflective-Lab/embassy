// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + deterministic stub.
//!
//! The real provider hits the OFAC Sanctions List Search API
//! (`https://sanctionssearch.ofac.treas.gov/`). The stub here returns
//! synthetic hits driven by the queried name — no network, no auth,
//! no rate-limit considerations — so callers can wire Formations
//! today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::OfacSlsError;
use crate::types::{MatchType, SanctionsHit, SanctionsSubject, SubjectType};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OfacSlsRequest {
    Screen { subject: SanctionsSubject },
}

impl FactPayload for OfacSlsRequest {
    const FAMILY: &'static str = "embassy.ofac_sls.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfacSlsResponse {
    pub records: Vec<Observation<SanctionsHit>>,
}

#[async_trait]
pub trait OfacSlsProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn screen(
        &self,
        request: &OfacSlsRequest,
        ctx: &CallContext,
    ) -> Result<OfacSlsResponse, OfacSlsError>;
}

/// Deterministic stub. No network, no auth.
///
/// Stub rule: a subject whose name contains "BLOCKED" returns one
/// synthetic hit; otherwise the response is empty (clean screen).
/// This lets tests assert *both* the hit and the no-hit branches
/// without mocking transport.
#[derive(Debug, Clone, Default)]
pub struct StubOfacSlsProvider;

#[async_trait]
impl OfacSlsProvider for StubOfacSlsProvider {
    fn name(&self) -> &'static str {
        "stub_ofac_sls"
    }

    async fn screen(
        &self,
        request: &OfacSlsRequest,
        _ctx: &CallContext,
    ) -> Result<OfacSlsResponse, OfacSlsError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| OfacSlsError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let OfacSlsRequest::Screen { subject } = request;
        let mut records = Vec::new();
        if subject.name.to_ascii_uppercase().contains("BLOCKED") {
            let hit = SanctionsHit {
                subject_name: subject.name.clone(),
                match_score: 0.99,
                match_type: MatchType::Exact,
                subject_type: SubjectType::Entity,
                list_name: "OFAC SDN".to_string(),
                list_program: Some("STUB-PROGRAM".to_string()),
                listed_at: Some("2026-01-01".to_string()),
                aliases: Vec::new(),
                jurisdictions: vec!["US".to_string()],
            };
            records.push(Observation {
                observation_id: format!("obs:ofac_sls:{request_hash}"),
                request_hash: request_hash.clone(),
                vendor: "stub_ofac_sls".to_string(),
                model: "stub".to_string(),
                latency_ms: 5,
                cost_estimate: None,
                tokens: None,
                content: hit,
                raw_response: None,
            });
        }

        Ok(OfacSlsResponse { records })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_returns_hit_for_blocked_subject() {
        // Intent: the stub must produce hits on the same trigger every
        // run so Formation tests can rely on it. Drop the trigger and
        // every dependent test goes silent (no-hit) instead of failing
        // loudly — worst-case for a sanctions test suite.
        let provider = StubOfacSlsProvider;
        let req = OfacSlsRequest::Screen {
            subject: SanctionsSubject::parse("BLOCKED Holdings AB").unwrap(),
        };
        let resp = provider
            .screen(&req, &CallContext::default())
            .await
            .unwrap();
        assert_eq!(resp.records.len(), 1);
        assert_eq!(resp.records[0].content.match_type, MatchType::Exact);
    }

    #[tokio::test]
    async fn stub_returns_empty_for_clean_subject() {
        // Intent: a clean screen must produce zero records (not one
        // with `match_score = 0`). The downstream "any hits?" check is
        // `records.is_empty()`; if the stub starts returning empty
        // hits, the contract silently inverts.
        let provider = StubOfacSlsProvider;
        let req = OfacSlsRequest::Screen {
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
        // Intent: replay-from-audit contract — the recorded
        // request_hash must equal content_hash(canonical-JSON request)
        // so an auditor can re-derive the hash from the stored
        // request and confirm provenance.
        let provider = StubOfacSlsProvider;
        let req = OfacSlsRequest::Screen {
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
