// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + stub implementation.
//!
//! The real provider (HTTP against Bolagsverket's Näringslivsregistret
//! or Foretagsapi) is deferred behind a future `live` feature flag.
//! The stub here is what tests and Formation harnesses exercise —
//! deterministic, no network, no credentials.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::BolagsverketError;
use crate::types::{Address, Company, CompanyStatus, LegalForm, OrgNumber};

/// What a caller asks for. Two shapes:
/// - `Lookup` — fetch a single company by org number
/// - `SearchByName` — fuzzy-match registered companies by name fragment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BolagsverketRequest {
    Lookup { org_number: OrgNumber },
    SearchByName { name_fragment: String, limit: usize },
}

impl FactPayload for BolagsverketRequest {
    const FAMILY: &'static str = "embassy.bolagsverket.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BolagsverketResponse {
    pub records: Vec<Observation<Company>>,
}

#[async_trait]
pub trait BolagsverketProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &BolagsverketRequest,
        ctx: &CallContext,
    ) -> Result<BolagsverketResponse, BolagsverketError>;
}

/// Deterministic stub. No network, no auth. Returns one canned
/// [`Company`] per Lookup request; for SearchByName, returns up to
/// `limit` synthetic companies whose org numbers and names are derived
/// from the request hash.
#[derive(Debug, Clone, Default)]
pub struct StubBolagsverketProvider;

#[async_trait]
impl BolagsverketProvider for StubBolagsverketProvider {
    fn name(&self) -> &'static str {
        "stub_bolagsverket"
    }

    async fn fetch(
        &self,
        request: &BolagsverketRequest,
        _ctx: &CallContext,
    ) -> Result<BolagsverketResponse, BolagsverketError> {
        let hash_input = serde_json::to_string(request).map_err(|e| {
            BolagsverketError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&hash_input);

        let companies = match request {
            BolagsverketRequest::Lookup { org_number } => {
                vec![canned_company(org_number.clone(), 0)]
            }
            BolagsverketRequest::SearchByName {
                name_fragment,
                limit,
            } => {
                // Generate `limit` synthetic companies. Org numbers are
                // derived from a Luhn-valid base sequence so they all
                // pass OrgNumber::parse if a caller re-parses them.
                (0..*limit)
                    .map(|idx| {
                        let org = synthetic_org_number(&request_hash, idx);
                        let mut c = canned_company(org, idx);
                        c.name = format!("{name_fragment} #{idx} AB");
                        c
                    })
                    .collect()
            }
        };

        let records = companies
            .into_iter()
            .enumerate()
            .map(|(idx, company)| Observation {
                observation_id: format!("obs:bolagsverket:{request_hash}:{idx}"),
                request_hash: request_hash.clone(),
                vendor: "stub_bolagsverket".to_string(),
                model: "stub".to_string(),
                latency_ms: 5,
                cost_estimate: None,
                tokens: None,
                content: company,
                raw_response: None,
            })
            .collect();

        Ok(BolagsverketResponse { records })
    }
}

fn canned_company(org_number: OrgNumber, idx: usize) -> Company {
    Company {
        org_number,
        name: format!("Stub Aktiebolag #{idx}"),
        legal_form: LegalForm::Aktiebolag,
        status: CompanyStatus::Active,
        registered_at: "2020-01-15".to_string(),
        registered_address: Some(Address {
            street: "Storgatan 1".into(),
            postal_code: "111 22".into(),
            city: "Stockholm".into(),
            country: "Sverige".into(),
        }),
    }
}

/// Generates a deterministic Luhn-valid 10-digit org number from a
/// request hash + index. The base 9 digits are derived from the hash,
/// then a Luhn check digit is appended so the resulting OrgNumber
/// passes validation when consumers re-parse it.
fn synthetic_org_number(request_hash: &str, idx: usize) -> OrgNumber {
    // Mix idx into the hash slice so multiple synthetic results don't
    // collide on the same number.
    let mixed = format!("{request_hash}{idx:04}");
    let base_u64 = u64::from_str_radix(&mixed[..12], 16).unwrap_or(0);
    let base_nine = format!("{:09}", base_u64 % 1_000_000_000);
    let check = luhn_check_digit(&base_nine);
    let full = format!("{base_nine}{check}");
    OrgNumber::parse(full).expect("synthetic org number must Luhn-validate by construction")
}

fn luhn_check_digit(base_nine: &str) -> u32 {
    let sum: u32 = base_nine
        .chars()
        .rev()
        .enumerate()
        .map(|(i, c)| {
            let d = c.to_digit(10).unwrap_or(0);
            if i % 2 == 0 {
                let doubled = d * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                d
            }
        })
        .sum();
    (10 - sum % 10) % 10
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volvo() -> OrgNumber {
        OrgNumber::parse("556074-3089").unwrap()
    }

    #[tokio::test]
    async fn stub_lookup_returns_one_observation() {
        // Intent: Lookup is point-query semantics — exactly one record
        // returned for a hit, zero for a miss. If the stub ever fans
        // out, every caller that expects "single result" semantics
        // breaks.
        let provider = StubBolagsverketProvider;
        let req = BolagsverketRequest::Lookup {
            org_number: volvo(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
        assert_eq!(resp.records[0].content.org_number, volvo());
    }

    #[tokio::test]
    async fn stub_search_returns_requested_limit() {
        // Intent: SearchByName must honor the `limit` parameter. If
        // the stub silently returned a different count, pagination
        // logic on the caller would diverge from production.
        let provider = StubBolagsverketProvider;
        let req = BolagsverketRequest::SearchByName {
            name_fragment: "Volvo".into(),
            limit: 3,
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(
            resp.records.len(),
            3,
            "limit=3 must produce exactly 3 records"
        );
    }

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        // Intent: same audit replay contract as the SEC port — the
        // request_hash recorded on the Observation must equal
        // content_hash(canonical-JSON request). Without this, replay
        // cannot rebind an observation_id to its originating request.
        let provider = StubBolagsverketProvider;
        let req = BolagsverketRequest::Lookup {
            org_number: volvo(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_search_results_have_luhn_valid_org_numbers() {
        // Intent: the synthetic org numbers the stub produces must be
        // Luhn-valid so callers that re-parse them (round-trip
        // through serde or a downstream filter) don't trip the
        // validation we ship to keep garbage out of the audit log.
        // The test is the load-bearing guarantee for the Luhn helper
        // in the stub generator.
        let provider = StubBolagsverketProvider;
        let req = BolagsverketRequest::SearchByName {
            name_fragment: "Acme".into(),
            limit: 5,
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        for record in resp.records {
            let raw = record.content.org_number.as_str().to_string();
            OrgNumber::parse(&raw)
                .expect("stub-generated org number must re-parse cleanly (Luhn-valid)");
        }
    }

    #[tokio::test]
    async fn stub_is_deterministic_for_identical_requests() {
        let provider = StubBolagsverketProvider;
        let req = BolagsverketRequest::SearchByName {
            name_fragment: "Volvo".into(),
            limit: 2,
        };
        let a = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let b = provider.fetch(&req, &CallContext::default()).await.unwrap();
        for (ra, rb) in a.records.iter().zip(b.records.iter()) {
            assert_eq!(ra.observation_id, rb.observation_id);
            assert_eq!(ra.request_hash, rb.request_hash);
            assert_eq!(ra.content, rb.content);
        }
    }
}
