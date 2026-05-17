// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + deterministic stub.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::SamGovError;
use crate::types::{ContractorRegistration, RegistrationStatus, Uei};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SamGovRequest {
    Lookup { uei: Uei },
}

impl FactPayload for SamGovRequest {
    const FAMILY: &'static str = "embassy.sam_gov.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamGovResponse {
    pub records: Vec<Observation<ContractorRegistration>>,
}

#[async_trait]
pub trait SamGovProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn lookup(
        &self,
        request: &SamGovRequest,
        ctx: &CallContext,
    ) -> Result<SamGovResponse, SamGovError>;
}

/// Deterministic stub. No network, no auth.
///
/// Stub rule: any well-formed UEI returns one synthetic Active
/// registration. UEIs whose first character is `X` return Expired;
/// UEIs whose first character is `Z` return Inactive — covers the
/// three operationally distinct status paths.
#[derive(Debug, Clone, Default)]
pub struct StubSamGovProvider;

#[async_trait]
impl SamGovProvider for StubSamGovProvider {
    fn name(&self) -> &'static str {
        "stub_sam_gov"
    }

    async fn lookup(
        &self,
        request: &SamGovRequest,
        _ctx: &CallContext,
    ) -> Result<SamGovResponse, SamGovError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| SamGovError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let SamGovRequest::Lookup { uei } = request;
        let status = match uei.as_str().chars().next() {
            Some('X') => RegistrationStatus::Expired,
            Some('Z') => RegistrationStatus::Inactive,
            _ => RegistrationStatus::Active,
        };
        let expiration_date = match status {
            RegistrationStatus::Active => Some("2027-01-01".to_string()),
            RegistrationStatus::Expired => Some("2024-01-01".to_string()),
            RegistrationStatus::Inactive | RegistrationStatus::Submitted => None,
        };

        let registration = ContractorRegistration {
            uei: uei.clone(),
            legal_name: format!("Stub Federal Contractor ({})", uei.as_str()),
            registration_status: status,
            expiration_date,
            cage_code: Some("1STUB".to_string()),
        };

        let obs = Observation {
            observation_id: format!("obs:sam_gov:{request_hash}"),
            request_hash,
            vendor: "stub_sam_gov".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: registration,
            raw_response: None,
        };

        Ok(SamGovResponse { records: vec![obs] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_distinguishes_three_status_paths() {
        // Intent: Active / Expired / Inactive drive operationally
        // distinct downstream decisions (eligible / lapsed-renewable /
        // gone). The stub must cover all three so Formation tests
        // can assert each branch.
        let provider = StubSamGovProvider;
        for (prefix, expected) in [
            ('A', RegistrationStatus::Active),
            ('X', RegistrationStatus::Expired),
            ('Z', RegistrationStatus::Inactive),
        ] {
            let uei_str: String = std::iter::once(prefix)
                .chain("BC123XYZ789".chars())
                .collect();
            let req = SamGovRequest::Lookup {
                uei: Uei::parse(&uei_str).unwrap(),
            };
            let resp = provider
                .lookup(&req, &CallContext::default())
                .await
                .unwrap();
            assert_eq!(
                resp.records[0].content.registration_status, expected,
                "prefix `{prefix}` must yield status {expected:?}"
            );
        }
    }

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        let provider = StubSamGovProvider;
        let req = SamGovRequest::Lookup {
            uei: Uei::parse("ABC123XYZ789").unwrap(),
        };
        let resp = provider
            .lookup(&req, &CallContext::default())
            .await
            .unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }
}
