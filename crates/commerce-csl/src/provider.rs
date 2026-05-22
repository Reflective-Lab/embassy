// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + deterministic stub.
//!
//! Real provider: `https://api.trade.gov/consolidated_screening_list/search`.
//! Stub here returns synthetic hits driven by the queried name.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::CommerceCslError;
use crate::types::{MatchType, SanctionsHit, SanctionsSubject, SubjectType};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommerceCslRequest {
    Screen { subject: SanctionsSubject },
}

impl FactPayload for CommerceCslRequest {
    const FAMILY: &'static str = "embassy.commerce_csl.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommerceCslResponse {
    pub records: Vec<Observation<SanctionsHit>>,
}

#[async_trait]
pub trait CommerceCslProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn screen(
        &self,
        request: &CommerceCslRequest,
        ctx: &CallContext,
    ) -> Result<CommerceCslResponse, CommerceCslError>;
}

/// Deterministic stub.
///
/// Stub rule: a subject whose name contains "BLOCKED" returns one
/// synthetic hit on the BIS Entity List; otherwise empty (clean
/// screen). Trigger matches the sister sanctions ports.
#[derive(Debug, Clone, Default)]
pub struct StubCommerceCslProvider;

#[async_trait]
impl CommerceCslProvider for StubCommerceCslProvider {
    fn name(&self) -> &'static str {
        "stub_commerce_csl"
    }

    async fn screen(
        &self,
        request: &CommerceCslRequest,
        _ctx: &CallContext,
    ) -> Result<CommerceCslResponse, CommerceCslError> {
        let hash_input = serde_json::to_string(request).map_err(|e| {
            CommerceCslError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&hash_input);

        let CommerceCslRequest::Screen { subject } = request;
        let mut records = Vec::new();
        if subject.name.to_ascii_uppercase().contains("BLOCKED") {
            let hit = SanctionsHit {
                subject_name: subject.name.clone(),
                match_score: 0.97,
                match_type: MatchType::Exact,
                subject_type: SubjectType::Entity,
                list_name: "Commerce CSL".to_string(),
                list_program: Some("BIS Entity List".to_string()),
                listed_at: Some("2026-01-01".to_string()),
                aliases: Vec::new(),
                jurisdictions: vec!["US".to_string()],
            };
            records.push(Observation {
                observation_id: format!("obs:commerce_csl:{request_hash}"),
                request_hash: request_hash.clone(),
                vendor: "stub_commerce_csl".to_string(),
                model: "stub".to_string(),
                latency_ms: 5,
                cost_estimate: None,
                tokens: None,
                content: hit,
                raw_response: None,
            });
        }

        Ok(CommerceCslResponse { records })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_returns_hit_for_blocked_subject() {
        let provider = StubCommerceCslProvider;
        let req = CommerceCslRequest::Screen {
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
        // Intent: clean screen produces zero records, matching the
        // sister sanctions ports' no-hit contract.
        let provider = StubCommerceCslProvider;
        let req = CommerceCslRequest::Screen {
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
        let provider = StubCommerceCslProvider;
        let req = CommerceCslRequest::Screen {
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
