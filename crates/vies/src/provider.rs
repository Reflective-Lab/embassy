// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + stub implementation.
//!
//! The real provider (SOAP against `https://ec.europa.eu/taxation_customs/`
//! `vies/services/checkVatService`) lives behind a future `live` feature.
//! The stub here is what tests and Formation harnesses exercise —
//! deterministic, no network, no credentials.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::ViesError;
use crate::types::{MemberStateStatus, VatNumber, VatValidation};

/// What a caller asks for. VIES is single-purpose: validate one VAT
/// number. The Request enum keeps the variant-tagged shape every other
/// embassy port uses so Formations can dispatch uniformly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ViesRequest {
    Validate { vat_number: VatNumber },
}

impl FactPayload for ViesRequest {
    const FAMILY: &'static str = "embassy.vies.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViesResponse {
    pub records: Vec<Observation<VatValidation>>,
}

#[async_trait]
pub trait ViesProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn validate(
        &self,
        request: &ViesRequest,
        ctx: &CallContext,
    ) -> Result<ViesResponse, ViesError>;
}

/// Deterministic stub. No network, no auth. Returns a synthetic
/// [`VatValidation`] whose `valid` field follows a deterministic rule
/// on the input (so tests can predict outcomes without mocking).
#[derive(Debug, Clone, Default)]
pub struct StubViesProvider;

#[async_trait]
impl ViesProvider for StubViesProvider {
    fn name(&self) -> &'static str {
        "stub_vies"
    }

    async fn validate(
        &self,
        request: &ViesRequest,
        _ctx: &CallContext,
    ) -> Result<ViesResponse, ViesError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| ViesError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let ViesRequest::Validate { vat_number } = request;

        // Deterministic stub rule:
        // - National portion containing "INVALID" → `valid = false`,
        //   member state Available (the disclosure: "we checked and
        //   it's not registered").
        // - National portion containing "OFFLINE" → `valid = false`,
        //   member state Unavailable (different status — call again).
        // - Everything else → `valid = true`.
        let national = &vat_number.national;
        let (valid, status) = if national.contains("INVALID") {
            (false, MemberStateStatus::Available)
        } else if national.contains("OFFLINE") {
            (false, MemberStateStatus::Unavailable)
        } else {
            (true, MemberStateStatus::Available)
        };

        let consultation_number = match status {
            MemberStateStatus::Available => Some(format!("STUB-{request_hash}")),
            MemberStateStatus::Unavailable => None,
        };

        let validation = VatValidation {
            vat_number: vat_number.clone(),
            valid,
            member_state_status: status,
            consultation_number,
            registered_name: if valid {
                Some(format!("Stub Aktiebolag ({})", vat_number.country.as_str()))
            } else {
                None
            },
            registered_address: if valid {
                Some("Stub Storgatan 1, Stockholm".to_string())
            } else {
                None
            },
            validated_at: "2026-05-17T00:00:00Z".to_string(),
        };

        let obs = Observation {
            observation_id: format!("obs:vies:{request_hash}"),
            request_hash,
            vendor: "stub_vies".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: validation,
            raw_response: None,
        };

        Ok(ViesResponse { records: vec![obs] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volvo() -> VatNumber {
        VatNumber::parse("SE556074308901").unwrap()
    }

    #[tokio::test]
    async fn stub_returns_one_observation_per_request() {
        // Intent: single validation → single observation. If the stub
        // ever fans out, every caller's "did this one VAT pass?" check
        // breaks.
        let provider = StubViesProvider;
        let req = ViesRequest::Validate {
            vat_number: volvo(),
        };
        let resp = provider
            .validate(&req, &CallContext::default())
            .await
            .unwrap();
        assert_eq!(resp.records.len(), 1);
    }

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        // Intent: the audit-replay contract — recorded request_hash
        // must equal content_hash(canonical-JSON request). Without
        // this, a VAT validation observation can't be rebound to its
        // originating request, and the consultation number loses its
        // audit weight.
        let provider = StubViesProvider;
        let req = ViesRequest::Validate {
            vat_number: volvo(),
        };
        let resp = provider
            .validate(&req, &CallContext::default())
            .await
            .unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_distinguishes_invalid_from_unavailable() {
        // Intent: the two failure modes (DB says no vs DB offline) are
        // operationally different. A `valid = false` Available means
        // "stop — this VAT does not exist." A `valid = false`
        // Unavailable means "we don't know yet; try again later." If
        // the stub or the live provider ever conflate them, callers
        // make wrong decisions on retry vs hard-stop.
        let provider = StubViesProvider;

        let invalid_req = ViesRequest::Validate {
            vat_number: VatNumber::parse("SE99INVALID01").unwrap(),
        };
        let offline_req = ViesRequest::Validate {
            vat_number: VatNumber::parse("SE99OFFLINE01").unwrap(),
        };

        let invalid_resp = provider
            .validate(&invalid_req, &CallContext::default())
            .await
            .unwrap();
        let offline_resp = provider
            .validate(&offline_req, &CallContext::default())
            .await
            .unwrap();

        assert!(!invalid_resp.records[0].content.valid);
        assert_eq!(
            invalid_resp.records[0].content.member_state_status,
            MemberStateStatus::Available,
            "INVALID case must report Available — we got a definitive `no`"
        );
        assert!(
            invalid_resp.records[0]
                .content
                .consultation_number
                .is_some()
        );

        assert!(!offline_resp.records[0].content.valid);
        assert_eq!(
            offline_resp.records[0].content.member_state_status,
            MemberStateStatus::Unavailable,
            "OFFLINE case must report Unavailable — try again later, not a definitive no"
        );
        assert!(
            offline_resp.records[0]
                .content
                .consultation_number
                .is_none(),
            "Unavailable answers have no audit weight — no consultation number"
        );
    }

    #[tokio::test]
    async fn stub_valid_response_includes_consultation_number() {
        // Intent: for any `valid = true` answer, the consultation
        // number must be present — that's the proof-of-check tax
        // authorities accept later. If a refactor stops piping it
        // through, the validation observation loses its audit weight
        // and the whole port becomes hearsay.
        let provider = StubViesProvider;
        let req = ViesRequest::Validate {
            vat_number: volvo(),
        };
        let resp = provider
            .validate(&req, &CallContext::default())
            .await
            .unwrap();
        assert!(resp.records[0].content.valid);
        assert!(
            resp.records[0].content.consultation_number.is_some(),
            "valid VIES checks must carry a consultation_number — the audit hook"
        );
    }

    #[tokio::test]
    async fn stub_is_deterministic_for_identical_requests() {
        let provider = StubViesProvider;
        let req = ViesRequest::Validate {
            vat_number: volvo(),
        };
        let a = provider
            .validate(&req, &CallContext::default())
            .await
            .unwrap();
        let b = provider
            .validate(&req, &CallContext::default())
            .await
            .unwrap();
        assert_eq!(a.records[0].observation_id, b.records[0].observation_id);
        assert_eq!(a.records[0].request_hash, b.records[0].request_hash);
        assert_eq!(a.records[0].content, b.records[0].content);
    }
}
