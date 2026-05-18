// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + stub implementation.
//!
//! The real provider (HTTP against `https://api.gleif.org/api/v1/lei-records`)
//! is deferred behind a future `live` feature. The stub here is what
//! tests and Formation harnesses exercise — deterministic, no network,
//! no credentials.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::GleifError;
use crate::types::{EntityCategory, LegalEntity, LegalEntityStatus, Lei, RegistrationStatus};

/// What a caller asks for. Three shapes:
/// - `Lookup` — fetch a single entity by LEI
/// - `SearchByName` — fuzzy match registered entities by legal-name fragment
/// - `Ownership` — fetch the parent/child relationships rooted at an LEI
///
/// Each shape is its own variant so a Formation can dispatch on the
/// request kind without parsing a free-form spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GleifRequest {
    Lookup {
        lei: Lei,
    },
    SearchByName {
        name_fragment: String,
        limit: usize,
    },
    /// Fetches the entity itself + its direct parent + its known child
    /// LEIs. The stub returns one entity per LEI without the children
    /// resolved; the live provider will follow the relationship graph
    /// according to GLEIF's pagination rules.
    Ownership {
        lei: Lei,
    },
}

impl FactPayload for GleifRequest {
    const FAMILY: &'static str = "embassy.gleif.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GleifResponse {
    pub records: Vec<Observation<LegalEntity>>,
}

#[async_trait]
pub trait GleifProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &GleifRequest,
        ctx: &CallContext,
    ) -> Result<GleifResponse, GleifError>;
}

/// Deterministic stub. No network, no auth. Returns canned
/// [`LegalEntity`] records whose LEIs are synthesized to be
/// mod-97-valid so callers that re-parse them succeed.
#[derive(Debug, Clone, Default)]
pub struct StubGleifProvider;

#[async_trait]
impl GleifProvider for StubGleifProvider {
    fn name(&self) -> &'static str {
        "stub_gleif"
    }

    async fn fetch(
        &self,
        request: &GleifRequest,
        _ctx: &CallContext,
    ) -> Result<GleifResponse, GleifError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| GleifError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        let entities = match request {
            GleifRequest::Lookup { lei } => {
                vec![canned_entity(lei.clone(), 0)]
            }
            GleifRequest::SearchByName {
                name_fragment,
                limit,
            } => (0..*limit)
                .map(|idx| {
                    let lei = synthetic_lei(&request_hash, idx);
                    let mut entity = canned_entity(lei, idx);
                    entity.legal_name = format!("{name_fragment} Holdings #{idx} AB");
                    entity
                })
                .collect(),
            GleifRequest::Ownership { lei } => vec![canned_entity(lei.clone(), 0)],
        };

        let records = entities
            .into_iter()
            .enumerate()
            .map(|(idx, entity)| Observation {
                observation_id: format!("obs:gleif:{request_hash}:{idx}"),
                request_hash: request_hash.clone(),
                vendor: "stub_gleif".to_string(),
                model: "stub".to_string(),
                latency_ms: 5,
                cost_estimate: None,
                tokens: None,
                content: entity,
                raw_response: None,
            })
            .collect();

        Ok(GleifResponse { records })
    }
}

fn canned_entity(lei: Lei, idx: usize) -> LegalEntity {
    LegalEntity {
        lei,
        legal_name: format!("Stub Legal Entity #{idx}"),
        jurisdiction: "SE".to_string(),
        legal_form_code: "XTIQ".to_string(),
        registration_status: RegistrationStatus::Issued,
        entity_status: LegalEntityStatus::Active,
        entity_category: EntityCategory::General,
        direct_parent_lei: None,
    }
}

/// Generates a deterministic mod-97-valid LEI from a request hash + index.
///
/// Base 18 characters drawn from `[A-Z0-9]` (with chars 5-6 forced to
/// `"00"` per ISO 17442), then the two-character check digits computed
/// to satisfy `numeric % 97 == 1`. The result re-parses cleanly when
/// callers feed it back through [`Lei::parse`] — same guarantee
/// `synthetic_org_number` provides in the Bolagsverket stub.
fn synthetic_lei(request_hash: &str, idx: usize) -> Lei {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut mixed = format!("{request_hash}{idx:04}");
    while mixed.len() < 18 {
        mixed.push('0');
    }
    let mixed = mixed.as_bytes();
    let mut base = [b'0'; 18];
    for i in 0..18 {
        base[i] = ALPHABET[(mixed[i] as usize) % ALPHABET.len()];
    }
    // Reserved chars 5-6 must be "00".
    base[4] = b'0';
    base[5] = b'0';
    // base is constructed from ASCII digits and letters only — from_utf8 cannot fail.
    let base_str = std::str::from_utf8(&base)
        .unwrap_or_else(|_| unreachable!("base is ASCII by construction"));
    let check = compute_mod_97_check(base_str);
    let full = format!("{base_str}{check:02}");
    Lei::parse(&full).unwrap_or_else(|e| {
        debug_assert!(false, "synthetic_lei produced invalid LEI: {e}");
        unreachable!("synthetic LEI construction invariant violated")
    })
}

/// Compute the ISO 17442 mod-97-10 check digits for an 18-character base.
///
/// Algorithm: convert the 18-char base to its numeric representation
/// (A=10, B=11, ..., Z=35), append "00", and compute `(value * 100) % 97`.
/// The check digits are `98 - that residue` — when the original base is
/// re-paired with the check digits, the full 20-char value satisfies
/// `numeric % 97 == 1`.
fn compute_mod_97_check(base_18: &str) -> u8 {
    let mut residue: u64 = 0;
    for ch in base_18.chars() {
        let value: u64 = if ch.is_ascii_digit() {
            u64::from(ch as u8 - b'0')
        } else {
            u64::from(ch as u8 - b'A') + 10
        };
        let shift = if value >= 10 { 100 } else { 10 };
        residue = (residue * shift + value) % 97;
    }
    // Account for the trailing "00" that will become the check digits.
    residue = (residue * 100) % 97;
    let check = (98 - residue) % 97;
    debug_assert!(check < 256, "mod-97 result must fit in u8");
    check as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apple_lei() -> Lei {
        Lei::parse("HWUPKR0MPOU8FGXBT394").unwrap()
    }

    #[tokio::test]
    async fn stub_lookup_returns_one_observation() {
        // Intent: Lookup is point-query semantics — exactly one
        // record returned for a hit. If the stub ever fans out
        // (paginated, multi-record), callers that expect
        // single-result semantics break.
        let provider = StubGleifProvider;
        let req = GleifRequest::Lookup { lei: apple_lei() };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
        assert_eq!(resp.records[0].content.lei, apple_lei());
    }

    #[tokio::test]
    async fn stub_search_honours_limit() {
        // Intent: SearchByName must honour `limit`. Silently
        // returning a different count would diverge pagination
        // logic on the caller from production behaviour.
        let provider = StubGleifProvider;
        let req = GleifRequest::SearchByName {
            name_fragment: "Volvo".to_string(),
            limit: 4,
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 4);
    }

    #[tokio::test]
    async fn stub_search_results_have_valid_lei_checksums() {
        // Intent: the synthetic LEIs the stub produces must be
        // mod-97-valid so a caller that re-parses them (round-trip
        // through serde or a downstream LEI filter) does not trip
        // the validation we ship at the port boundary. The test is
        // the load-bearing guarantee for the synthetic-LEI helper.
        let provider = StubGleifProvider;
        let req = GleifRequest::SearchByName {
            name_fragment: "Acme".to_string(),
            limit: 6,
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        for record in resp.records {
            let raw = record.content.lei.as_str().to_string();
            Lei::parse(&raw).expect("stub-generated LEI must re-parse (mod-97 valid)");
        }
    }

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        // Intent: the audit replay contract — recorded request_hash
        // must equal content_hash(canonical-JSON request). Without
        // this, replay cannot rebind an observation_id to its
        // originating request.
        let provider = StubGleifProvider;
        let req = GleifRequest::Lookup { lei: apple_lei() };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_is_deterministic_for_identical_requests() {
        let provider = StubGleifProvider;
        let req = GleifRequest::SearchByName {
            name_fragment: "Volvo".to_string(),
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
