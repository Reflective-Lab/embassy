// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + deterministic stub.
//!
//! Scope reminder: this port queries only the publicly disclosable
//! tax-administrative status surface (F-skatt + VAT + employer
//! registration booleans). See `lib.rs` for the full legal scoping
//! note.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::SkatteverketError;
use crate::types::{SwedishOrgNumber, SwedishTaxStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SkatteverketRequest {
    /// Query the public tax-administrative status of a Swedish
    /// organisation. This is the only operation in the public
    /// surface; no amount, history, or personal queries.
    Status { org_number: SwedishOrgNumber },
}

impl FactPayload for SkatteverketRequest {
    const FAMILY: &'static str = "embassy.skatteverket.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkatteverketResponse {
    pub records: Vec<Observation<SwedishTaxStatus>>,
}

#[async_trait]
pub trait SkatteverketProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn status(
        &self,
        request: &SkatteverketRequest,
        ctx: &CallContext,
    ) -> Result<SkatteverketResponse, SkatteverketError>;
}

/// Deterministic stub. No network, no auth.
///
/// Stub rule: any well-formed org number returns Active-everything.
/// Org numbers starting with `0` return a "no F-skatt, no VAT, no
/// employer" state so tests can assert the negative path.
#[derive(Debug, Clone, Default)]
pub struct StubSkatteverketProvider;

#[async_trait]
impl SkatteverketProvider for StubSkatteverketProvider {
    fn name(&self) -> &'static str {
        "stub_skatteverket"
    }

    async fn status(
        &self,
        request: &SkatteverketRequest,
        _ctx: &CallContext,
    ) -> Result<SkatteverketResponse, SkatteverketError> {
        let hash_input = serde_json::to_string(request).map_err(|e| {
            SkatteverketError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&hash_input);

        let SkatteverketRequest::Status { org_number } = request;
        let inactive = org_number.as_str().starts_with('0');
        let status = SwedishTaxStatus {
            org_number: org_number.clone(),
            has_f_skatt: !inactive,
            vat_registered: !inactive,
            employer_registered: !inactive,
            as_of: "2026-05-17T00:00:00Z".to_string(),
        };

        let obs = Observation {
            observation_id: format!("obs:skatteverket:{request_hash}"),
            request_hash,
            vendor: "stub_skatteverket".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: status,
            raw_response: None,
        };

        Ok(SkatteverketResponse { records: vec![obs] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_returns_active_status_for_well_formed_orgnr() {
        // Intent: the happy path — a registered AB with active F-skatt,
        // VAT, and employer registration. The downstream "can I pay
        // this counterparty without withholding?" check is
        // `has_f_skatt == true`; this guards that path stays alive.
        let provider = StubSkatteverketProvider;
        let req = SkatteverketRequest::Status {
            org_number: SwedishOrgNumber::parse("5560743089").unwrap(),
        };
        let resp = provider
            .status(&req, &CallContext::default())
            .await
            .unwrap();
        let s = &resp.records[0].content;
        assert!(s.has_f_skatt);
        assert!(s.vat_registered);
        assert!(s.employer_registered);
    }

    #[tokio::test]
    async fn stub_returns_inactive_status_for_zero_prefix() {
        // Intent: the negative path — a lapsed registration. Drives
        // the "must withhold preliminary tax" branch downstream. If
        // the stub conflates the two paths, the withholding rule
        // breaks silently.
        let provider = StubSkatteverketProvider;
        let req = SkatteverketRequest::Status {
            org_number: SwedishOrgNumber::parse("0560743089").unwrap(),
        };
        let resp = provider
            .status(&req, &CallContext::default())
            .await
            .unwrap();
        let s = &resp.records[0].content;
        assert!(!s.has_f_skatt);
        assert!(!s.vat_registered);
        assert!(!s.employer_registered);
    }

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        let provider = StubSkatteverketProvider;
        let req = SkatteverketRequest::Status {
            org_number: SwedishOrgNumber::parse("5560743089").unwrap(),
        };
        let resp = provider
            .status(&req, &CallContext::default())
            .await
            .unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }
}
