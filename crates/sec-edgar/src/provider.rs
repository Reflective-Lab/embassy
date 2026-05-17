// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + stub implementation.
//!
//! The real provider (HTTP against `https://www.sec.gov/cgi-bin/browse-edgar`
//! + filing index XML/JSON + section extraction) is deferred behind a
//! future `live` feature flag. The stub here is what tests and
//! Formation harnesses exercise — deterministic, no network, no
//! credentials.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::SecEdgarError;
use crate::types::{AccessionNumber, Cik, Filing, FilingSection, FormType};

/// What a caller asks for. Two shapes:
/// - `Filing` — fetch a single filing by (cik, accession)
/// - `LatestByForm` — fetch the most recent filing of a form type for a CIK
///
/// Each shape is its own variant so a Formation can dispatch on the
/// request kind without parsing a free-form spec. Carries a
/// [`FactPayload`] impl so a `SecEdgarRequest` can ride the kernel as
/// a `Seeds` fact, which is what [`crate::SecFilingSuggestor`] consumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SecEdgarRequest {
    Filing {
        cik: Cik,
        accession_number: AccessionNumber,
    },
    LatestByForm {
        cik: Cik,
        form_type: FormType,
    },
}

impl FactPayload for SecEdgarRequest {
    const FAMILY: &'static str = "embassy.sec_edgar.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecEdgarResponse {
    pub records: Vec<Observation<Filing>>,
}

#[async_trait]
pub trait SecEdgarProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &SecEdgarRequest,
        ctx: &CallContext,
    ) -> Result<SecEdgarResponse, SecEdgarError>;
}

/// Deterministic stub. Given a request, returns a canned [`Filing`]
/// whose section bodies are derived from the request hash so two
/// different requests produce two different filings. No network, no
/// auth, suitable for CI and formation-loop unit tests.
#[derive(Debug, Clone, Default)]
pub struct StubSecEdgarProvider;

#[async_trait]
impl SecEdgarProvider for StubSecEdgarProvider {
    fn name(&self) -> &'static str {
        "stub_sec_edgar"
    }

    async fn fetch(
        &self,
        request: &SecEdgarRequest,
        _ctx: &CallContext,
    ) -> Result<SecEdgarResponse, SecEdgarError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| SecEdgarError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let (cik, accession_number, form_type) = match request {
            SecEdgarRequest::Filing {
                cik,
                accession_number,
            } => (cik.clone(), accession_number.clone(), FormType::Form10K),
            SecEdgarRequest::LatestByForm { cik, form_type } => {
                // Deterministic synthetic accession derived from the
                // request hash so unit tests can assert exact values.
                // SEC accession numbers are digits-only (18 digits in
                // 10-2-6 form), so reinterpret the hex hash as a
                // u64 and project into the 6-digit suffix space.
                let suffix_u64 = u64::from_str_radix(&request_hash[..12], 16).unwrap_or(0);
                let suffix = format!("{:06}", suffix_u64 % 1_000_000);
                (
                    cik.clone(),
                    AccessionNumber::parse(
                        format!("{prefix}-23-{suffix}", prefix = cik.as_str(),),
                    )?,
                    form_type.clone(),
                )
            }
        };

        let mut sections = BTreeMap::new();
        sections.insert(
            "1A".to_string(),
            FilingSection {
                id: "1A".into(),
                title: "Risk Factors".into(),
                body: format!("Stub risk factor content for request {request_hash}."),
            },
        );

        let filing = Filing {
            cik,
            accession_number,
            form_type,
            filed_at: "2026-01-01".into(),
            sections,
        };

        let obs = Observation {
            observation_id: format!("obs:sec-edgar:{request_hash}"),
            request_hash,
            vendor: "stub_sec_edgar".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: filing,
            raw_response: None,
        };

        Ok(SecEdgarResponse { records: vec![obs] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apple_cik() -> Cik {
        Cik::parse("0000320193").unwrap()
    }

    #[tokio::test]
    async fn stub_returns_one_observation_per_request() {
        // Intent: a single request must produce exactly one Observation
        // — if the stub ever started fanning out (or returning zero)
        // every caller's pagination + audit-count assumptions would
        // silently break.
        let provider = StubSecEdgarProvider;
        let req = SecEdgarRequest::LatestByForm {
            cik: apple_cik(),
            form_type: FormType::Form10K,
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }

    #[tokio::test]
    async fn stub_request_hash_round_trips_to_content_hash() {
        // Intent: the audit replay contract — given a request, the
        // recorded request_hash must equal content_hash(serialized request).
        // If a future refactor stops piping the request into the hash
        // input, replay-from-audit breaks: you'd have an
        // observation_id with no way to verify which input produced it.
        let provider = StubSecEdgarProvider;
        let req = SecEdgarRequest::Filing {
            cik: apple_cik(),
            accession_number: AccessionNumber::parse("0000320193-23-000106").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();

        let expected_hash = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(
            resp.records[0].request_hash, expected_hash,
            "request_hash must equal content_hash of the canonical-JSON request"
        );
        assert!(
            resp.records[0].observation_id.ends_with(&expected_hash),
            "observation_id must derive from request_hash so audit can join the two"
        );
    }

    #[tokio::test]
    async fn stub_is_deterministic_for_identical_requests() {
        // Intent: two identical requests must produce identical
        // observations. Without this, tests + replay are unstable.
        let provider = StubSecEdgarProvider;
        let req = SecEdgarRequest::LatestByForm {
            cik: apple_cik(),
            form_type: FormType::Form10K,
        };
        let a = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let b = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(a.records[0].request_hash, b.records[0].request_hash);
        assert_eq!(a.records[0].observation_id, b.records[0].observation_id);
        assert_eq!(a.records[0].content, b.records[0].content);
    }

    #[tokio::test]
    async fn stub_differentiates_distinct_requests() {
        // Intent: requesting two different form types from the same
        // CIK must produce two different observations — request_hash
        // is the audit primary key, two requests sharing one hash
        // would alias each other in any log search.
        let provider = StubSecEdgarProvider;
        let req_10k = SecEdgarRequest::LatestByForm {
            cik: apple_cik(),
            form_type: FormType::Form10K,
        };
        let req_10q = SecEdgarRequest::LatestByForm {
            cik: apple_cik(),
            form_type: FormType::Form10Q,
        };
        let a = provider
            .fetch(&req_10k, &CallContext::default())
            .await
            .unwrap();
        let b = provider
            .fetch(&req_10q, &CallContext::default())
            .await
            .unwrap();
        assert_ne!(
            a.records[0].request_hash, b.records[0].request_hash,
            "different form types must yield different request_hashes"
        );
        assert_ne!(
            a.records[0].content.form_type, b.records[0].content.form_type,
            "the typed content must carry the actual form type the caller asked for"
        );
    }

    #[tokio::test]
    async fn stub_propagates_form_type_into_filing_content() {
        // Intent: the form_type the caller asked for must be the
        // form_type the returned Filing carries. A regression that
        // dropped or coalesced form_type would produce mis-labelled
        // filings — silent corruption in any downstream tagged store.
        let provider = StubSecEdgarProvider;
        let req = SecEdgarRequest::LatestByForm {
            cik: apple_cik(),
            form_type: FormType::Form10Q,
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records[0].content.form_type, FormType::Form10Q);
    }
}
