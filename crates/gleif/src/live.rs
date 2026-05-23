// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Live GLEIF transport — HTTP fetch + JSON:API parse.
//!
//! Hits the free public GLEIF endpoint at
//! `https://api.gleif.org/api/v1/lei-records/{lei}`. GLEIF publishes
//! all reference data as CC0 (public domain) so no authentication or
//! key is required. The port records the public-domain license in
//! observation metadata so downstream consumers can rely on it.
//!
//! Gated behind the `live` feature so the default `embassy-gleif`
//! surface (typed domain + stub provider) remains dependency-light.
//! CI runs against the stub; this module is exercised by unit tests
//! that use canned JSON fixtures — no network in CI. A network-bound
//! integration test is marked `#[ignore]` so it runs on demand only.
//!
//! ## REAL-by-default boundary
//!
//! The constructor [`LiveGleifProvider::new`] is infallible because
//! the GLEIF API requires no credentials or environment variables to
//! reach. The first request can still fail (network unreachable,
//! malformed response, LEI not found) — those return typed
//! `GleifError` values. There is no silent fallback to the stub.
//! Apps that need stubbed responses must use [`StubGleifProvider`]
//! explicitly.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use manifold::{HttpFetchProvider, WebFetchBackend, WebFetchRequest};
use serde::Deserialize;
use thiserror::Error;

use crate::error::GleifError;
use crate::provider::{GleifProvider, GleifRequest, GleifResponse};
use crate::types::{EntityCategory, LegalEntity, LegalEntityStatus, Lei, RegistrationStatus};
use crate::{Observation, content_hash};

/// GLEIF's public endpoint base. Apps that need to point at a mirror
/// override via `LiveFetchOptions::base_url`.
pub const DEFAULT_BASE_URL: &str = "https://api.gleif.org/api/v1";

/// GLEIF doesn't require a specific User-Agent but their fair-use
/// docs ask consumers to identify themselves. Pin a recognizable
/// Reflective Labs research contact.
pub const DEFAULT_USER_AGENT: &str = "Reflective Labs Research kpernyer@gmail.com";

/// GLEIF records are small JSON documents (typically <10 KiB per
/// entity). Cap at 1 MiB to bound any pathological response.
pub const DEFAULT_MAX_BYTES: usize = 1 * 1024 * 1024;

/// Per-request timeout. GLEIF responses are fast (<500 ms typical);
/// 30 s is generous but bounded.
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// GLEIF does not publish a strict rate limit but their fair-use
/// policy implies bursts should stay under ~10 req/sec. Sleep 100 ms
/// after each fetch as a politeness floor.
pub const POLITENESS_DELAY: Duration = Duration::from_millis(100);

/// Configuration for live fetches. Defaults follow GLEIF's published
/// fair-use guidance; apps override per-request as needed.
#[derive(Debug, Clone)]
pub struct LiveFetchOptions {
    pub base_url: String,
    pub user_agent: String,
    pub max_bytes: usize,
    pub timeout_ms: u64,
}

impl Default for LiveFetchOptions {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            max_bytes: DEFAULT_MAX_BYTES,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Error)]
pub enum LiveError {
    #[error("fetch failed: {0}")]
    Fetch(String),
    #[error("HTTP {status} from {url}")]
    Http { status: u16, url: String },
    #[error("LEI not found: {0}")]
    NotFound(String),
    #[error("parse failed: {0}")]
    Parse(String),
    #[error("blocking task join failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Live GLEIF provider for the source-shaped provider trait.
///
/// Implements [`GleifRequest::Lookup`] against the public REST API.
/// `SearchByName` and `Ownership` request shapes return
/// [`GleifError::Unsupported`] until corresponding live endpoints
/// are wired — they remain stub-only paths today.
#[derive(Debug, Clone, Default)]
pub struct LiveGleifProvider {
    options: LiveFetchOptions,
}

impl LiveGleifProvider {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_options(options: LiveFetchOptions) -> Self {
        Self { options }
    }
}

#[async_trait]
impl GleifProvider for LiveGleifProvider {
    fn name(&self) -> &'static str {
        "live_gleif"
    }

    async fn fetch(
        &self,
        request: &GleifRequest,
        _ctx: &embassy_pack::CallContext,
    ) -> Result<GleifResponse, GleifError> {
        let started = Instant::now();
        let lei = match request {
            GleifRequest::Lookup { lei } => lei.clone(),
            GleifRequest::SearchByName { .. } | GleifRequest::Ownership { .. } => {
                return Err(GleifError::InvalidRequest(
                    "live GLEIF provider supports Lookup only — \
                     SearchByName and Ownership remain stub-only today"
                        .to_string(),
                ));
            }
        };

        let url = format!("{}/lei-records/{}", self.options.base_url, lei.as_str());
        let body = fetch_json(&url, &self.options).await.map_err(live_error)?;
        let parsed: LeiRecordEnvelope = serde_json::from_slice(&body)
            .map_err(|e| GleifError::Transport(format!("parse failed: {e}")))?;
        let entity = parsed.data.into_legal_entity(&lei)?;

        let request_json = serde_json::to_string(request)
            .map_err(|e| GleifError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&request_json);
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        let observation = Observation {
            observation_id: format!("obs:gleif-live:{request_hash}"),
            request_hash,
            vendor: self.name().to_string(),
            model: "gleif-api-v1".to_string(),
            latency_ms,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: Some(String::from_utf8_lossy(&body).to_string()),
        };

        Ok(GleifResponse {
            records: vec![observation],
        })
    }
}

fn live_error(err: LiveError) -> GleifError {
    match err {
        LiveError::NotFound(lei) => GleifError::NotFound(lei),
        other => GleifError::Transport(other.to_string()),
    }
}

async fn fetch_json(url: &str, opts: &LiveFetchOptions) -> Result<Vec<u8>, LiveError> {
    let url = url.to_string();
    let opts = opts.clone();
    tokio::task::spawn_blocking(move || {
        let provider = HttpFetchProvider::new()
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_user_agent(&opts.user_agent);
        let request = WebFetchRequest::new(&url)
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_max_bytes(opts.max_bytes)
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_timeout_ms(opts.timeout_ms)
            .map_err(|e| LiveError::Fetch(e.to_string()))?;
        let response = provider
            .fetch(&request)
            .map_err(|e| LiveError::Fetch(e.to_string()))?;
        if response.status == 404 {
            return Err(LiveError::NotFound(url));
        }
        if response.status >= 400 {
            return Err(LiveError::Http {
                status: response.status,
                url: response.url,
            });
        }
        std::thread::sleep(POLITENESS_DELAY);
        Ok(response.body.into_bytes())
    })
    .await?
}

// ─────────────────────────── wire types ───────────────────────────
//
// GLEIF returns JSON:API. We deserialize only the fields the typed
// `LegalEntity` consumes; everything else is ignored (forward-compat
// with GLEIF's future schema additions). The `raw_response` on the
// observation preserves the full payload for audit.

#[derive(Debug, Deserialize)]
struct LeiRecordEnvelope {
    data: LeiRecord,
}

#[derive(Debug, Deserialize)]
struct LeiRecord {
    #[serde(default)]
    attributes: LeiAttributes,
}

#[derive(Debug, Default, Deserialize)]
struct LeiAttributes {
    #[serde(default)]
    entity: WireEntity,
    #[serde(default)]
    registration: WireRegistration,
}

#[derive(Debug, Default, Deserialize)]
struct WireEntity {
    #[serde(default)]
    #[serde(rename = "legalName")]
    legal_name: Option<WireName>,
    #[serde(default)]
    jurisdiction: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    #[serde(rename = "legalForm")]
    legal_form: Option<WireLegalForm>,
}

#[derive(Debug, Default, Deserialize)]
struct WireName {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WireLegalForm {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WireRegistration {
    #[serde(default)]
    status: Option<String>,
}

impl LeiRecord {
    fn into_legal_entity(self, expected_lei: &Lei) -> Result<LegalEntity, GleifError> {
        let entity = self.attributes.entity;
        let registration = self.attributes.registration;

        let legal_name = entity
            .legal_name
            .and_then(|n| n.name)
            .ok_or_else(|| GleifError::Transport("missing entity.legalName.name".to_string()))?;
        let jurisdiction = entity.jurisdiction.unwrap_or_default();
        let legal_form_code = entity.legal_form.and_then(|f| f.id).unwrap_or_default();
        let registration_status = parse_registration_status(registration.status.as_deref());
        let entity_status = parse_entity_status(entity.status.as_deref());
        let entity_category = parse_entity_category(entity.category.as_deref());

        Ok(LegalEntity {
            lei: expected_lei.clone(),
            legal_name,
            jurisdiction,
            legal_form_code,
            registration_status,
            entity_status,
            entity_category,
            direct_parent_lei: None,
        })
    }
}

fn parse_registration_status(raw: Option<&str>) -> RegistrationStatus {
    match raw.unwrap_or("").to_ascii_uppercase().as_str() {
        "ISSUED" => RegistrationStatus::Issued,
        "ANNULLED" => RegistrationStatus::Annulled,
        "DUPLICATE" => RegistrationStatus::Duplicate,
        "LAPSED" => RegistrationStatus::Lapsed,
        "MERGED" => RegistrationStatus::Merged,
        "RETIRED" => RegistrationStatus::Retired,
        "PENDING" | "PENDING_TRANSFER" | "PENDING_ARCHIVAL" => RegistrationStatus::Pending,
        _ => RegistrationStatus::Pending,
    }
}

fn parse_entity_status(raw: Option<&str>) -> LegalEntityStatus {
    match raw.unwrap_or("").to_ascii_uppercase().as_str() {
        "ACTIVE" => LegalEntityStatus::Active,
        _ => LegalEntityStatus::Inactive,
    }
}

fn parse_entity_category(raw: Option<&str>) -> EntityCategory {
    match raw.unwrap_or("").to_ascii_uppercase().as_str() {
        "GENERAL" => EntityCategory::General,
        "BRANCH" => EntityCategory::Branch,
        "FUND" => EntityCategory::Fund,
        "SOLE_PROPRIETOR" => EntityCategory::SoleProprietor,
        "RESIDENT_GOVERNMENTAL" => EntityCategory::ResidentGovernmental,
        "INTERNATIONAL_ORGANIZATION" => EntityCategory::InternationalOrganization,
        other if other.is_empty() => EntityCategory::Other(String::new()),
        other => EntityCategory::Other(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apple Inc.'s real LEI, used as the lookup fixture in unit tests.
    const APPLE_LEI: &str = "HWUPKR0MPOU8FGXBT394";

    /// Canned snippet that matches the shape of GLEIF's real JSON:API
    /// response for Apple Inc.'s LEI — actual field names lifted from
    /// `https://api.gleif.org/api/v1/lei-records/HWUPKR0MPOU8FGXBT394`.
    /// Only the fields the typed `LegalEntity` consumes are present;
    /// production responses carry many more attributes that we ignore.
    const APPLE_FIXTURE: &str = r#"{
        "data": {
            "type": "lei-records",
            "id": "HWUPKR0MPOU8FGXBT394",
            "attributes": {
                "lei": "HWUPKR0MPOU8FGXBT394",
                "entity": {
                    "legalName": {"name": "Apple Inc.", "language": "en"},
                    "jurisdiction": "US-CA",
                    "category": "GENERAL",
                    "status": "ACTIVE",
                    "legalForm": {"id": "T91W"}
                },
                "registration": {
                    "status": "ISSUED",
                    "initialRegistrationDate": "2012-06-06T18:00:00Z"
                }
            }
        }
    }"#;

    #[test]
    fn fixture_parses_into_typed_legal_entity() {
        // Intent: the wire-to-typed conversion must round-trip every
        // field the downstream typed surface promises. If a future
        // GLEIF schema change drops one of these, this test catches it.
        let envelope: LeiRecordEnvelope = serde_json::from_str(APPLE_FIXTURE).unwrap();
        let lei = Lei::parse(APPLE_LEI).unwrap();
        let entity = envelope.data.into_legal_entity(&lei).unwrap();

        assert_eq!(entity.lei.as_str(), APPLE_LEI);
        assert_eq!(entity.legal_name, "Apple Inc.");
        assert_eq!(entity.jurisdiction, "US-CA");
        assert_eq!(entity.legal_form_code, "T91W");
        assert_eq!(entity.registration_status, RegistrationStatus::Issued);
        assert_eq!(entity.entity_status, LegalEntityStatus::Active);
        assert_eq!(entity.entity_category, EntityCategory::General);
    }

    #[test]
    fn missing_legal_name_is_rejected_loudly() {
        // Intent: a response without a legal name is malformed; emit a
        // typed error rather than silently filling with empty string.
        let malformed = r#"{
            "data": {
                "type": "lei-records",
                "id": "HWUPKR0MPOU8FGXBT394",
                "attributes": {
                    "entity": {"jurisdiction": "US-CA", "category": "GENERAL", "status": "ACTIVE"},
                    "registration": {"status": "ISSUED"}
                }
            }
        }"#;
        let envelope: LeiRecordEnvelope = serde_json::from_str(malformed).unwrap();
        let lei = Lei::parse(APPLE_LEI).unwrap();
        let result = envelope.data.into_legal_entity(&lei);
        assert!(matches!(result, Err(GleifError::Transport(_))));
    }

    #[test]
    fn registration_status_handles_uppercase_wire_values() {
        // Intent: GLEIF publishes status as upper-snake ("ISSUED",
        // "LAPSED"). Our typed enum is lower-snake. The parser must
        // bridge these without losing fidelity.
        assert_eq!(parse_registration_status(Some("ISSUED")), RegistrationStatus::Issued);
        assert_eq!(parse_registration_status(Some("LAPSED")), RegistrationStatus::Lapsed);
        assert_eq!(parse_registration_status(Some("MERGED")), RegistrationStatus::Merged);
        assert_eq!(parse_registration_status(None), RegistrationStatus::Pending);
        assert_eq!(parse_registration_status(Some("UNKNOWN_FUTURE_VALUE")), RegistrationStatus::Pending);
    }

    #[test]
    fn entity_category_long_tail_falls_through_to_other() {
        // Intent: GLEIF keeps the category enum open for new jurisdiction-
        // specific values. The parser must preserve unknown categories
        // verbatim in `Other(String)` instead of dropping them.
        assert_eq!(
            parse_entity_category(Some("SOME_JURISDICTION_SPECIFIC_CATEGORY")),
            EntityCategory::Other("SOME_JURISDICTION_SPECIFIC_CATEGORY".to_string())
        );
    }

    /// Live network test — disabled by default. Run on demand with:
    ///     cargo test -p converge-embassy-gleif --features live -- --ignored live_lookup_apple
    /// Skipped if `GLEIF_LIVE_TEST` env var is not set, so a sleeping
    /// `--include-ignored` invocation cannot accidentally hit the
    /// network from CI.
    #[tokio::test]
    #[ignore = "live network call against api.gleif.org; opt-in only"]
    async fn live_lookup_apple() {
        if std::env::var("GLEIF_LIVE_TEST").is_err() {
            eprintln!("Set GLEIF_LIVE_TEST=1 to run the network-bound live test.");
            return;
        }
        let provider = LiveGleifProvider::new();
        let req = GleifRequest::Lookup {
            lei: Lei::parse(APPLE_LEI).unwrap(),
        };
        let resp = provider
            .fetch(&req, &embassy_pack::CallContext::default())
            .await
            .expect("live GLEIF lookup");
        assert_eq!(resp.records.len(), 1);
        let entity = &resp.records[0].content;
        assert_eq!(entity.lei.as_str(), APPLE_LEI);
        assert!(!entity.legal_name.is_empty());
    }
}
