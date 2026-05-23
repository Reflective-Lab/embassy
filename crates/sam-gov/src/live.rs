// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Live SAM.gov transport.
//!
//! Hits the public Entity Information API at
//! `https://api.sam.gov/entity-information/v3/entities`. Requires a
//! free api.data.gov key registered at
//! `https://api.data.gov/signup/`. Per the REAL-by-default standard:
//! [`LiveSamGovProvider::from_env`] reads `SAM_GOV_API_KEY` and fails
//! loud if the var is missing — never silent fallback to the stub.
//!
//! Gated behind the `live` feature.
//!
//! ## REAL-by-default boundary
//!
//! - [`Self::try_new(api_key)`] — explicit credential; rejects empty
//!   / whitespace keys at the boundary so a placeholder key produces a
//!   typed error before the first HTTP call.
//! - [`Self::from_env`] — reads `SAM_GOV_API_KEY`. Recommended path.
//! - The api_key is carried in the URL `?api_key=` query parameter
//!   per SAM.gov's documented contract. It's URL-encoded by the
//!   provider so keys containing reserved characters survive.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use manifold::{HttpFetchProvider, WebFetchBackend, WebFetchRequest};
use serde::Deserialize;
use thiserror::Error;

use crate::error::SamGovError;
use crate::provider::{SamGovProvider, SamGovRequest, SamGovResponse};
use crate::types::{ContractorRegistration, RegistrationStatus, Uei};
use crate::{Observation, content_hash};

pub const DEFAULT_BASE_URL: &str = "https://api.sam.gov/entity-information/v3/entities";
pub const DEFAULT_USER_AGENT: &str = "Reflective Labs Research kpernyer@gmail.com";
pub const DEFAULT_MAX_BYTES: usize = 2 * 1024 * 1024;
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const POLITENESS_DELAY: Duration = Duration::from_millis(120);

/// Environment variable consulted by [`LiveSamGovProvider::from_env`].
pub const API_KEY_ENV: &str = "SAM_GOV_API_KEY";

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
    #[error("UEI not found: {0}")]
    NotFound(String),
    #[error("api_key invalid or revoked")]
    Unauthorized,
    #[error("api_key rate-limit exceeded")]
    RateLimited,
    #[error("blocking task join failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Live SAM.gov provider. Reads contractor registrations via the
/// public Entity Information API. Requires `SAM_GOV_API_KEY`.
#[derive(Debug, Clone)]
pub struct LiveSamGovProvider {
    api_key: String,
    options: LiveFetchOptions,
}

impl LiveSamGovProvider {
    /// REAL-by-default constructor. Rejects empty / whitespace keys so
    /// a placeholder credential fails at construction rather than on
    /// the first call.
    pub fn try_new(api_key: impl Into<String>) -> Result<Self, SamGovError> {
        let api_key: String = api_key.into();
        if api_key.trim().is_empty() {
            return Err(SamGovError::Transport(format!(
                "{API_KEY_ENV} is empty or whitespace"
            )));
        }
        Ok(Self {
            api_key,
            options: LiveFetchOptions::default(),
        })
    }

    /// Read the api_key from `SAM_GOV_API_KEY`. Recommended path.
    /// Loud failure when the env var is missing or empty — never
    /// silent stub fallback.
    pub fn from_env() -> Result<Self, SamGovError> {
        let api_key = std::env::var(API_KEY_ENV).map_err(|_| {
            SamGovError::Transport(format!(
                "{API_KEY_ENV} not set; register a free key at \
                 https://api.data.gov/signup/ and add it to your env \
                 (or .envrc + direnv allow)."
            ))
        })?;
        Self::try_new(api_key)
    }

    #[must_use]
    pub fn with_options(mut self, options: LiveFetchOptions) -> Self {
        self.options = options;
        self
    }
}

#[async_trait]
impl SamGovProvider for LiveSamGovProvider {
    fn name(&self) -> &'static str {
        "live_sam_gov"
    }

    async fn lookup(
        &self,
        request: &SamGovRequest,
        _ctx: &embassy_pack::CallContext,
    ) -> Result<SamGovResponse, SamGovError> {
        let started = Instant::now();
        let SamGovRequest::Lookup { uei } = request;

        let url = build_lookup_url(&self.options.base_url, uei, &self.api_key);
        let body = fetch_json(&url, &self.options).await.map_err(live_error)?;

        let parsed: EntitiesEnvelope = serde_json::from_slice(&body)
            .map_err(|e| SamGovError::Transport(format!("parse failed: {e}")))?;
        let entity = parsed
            .first_entity_for(uei)
            .ok_or_else(|| SamGovError::NotFound(uei.as_str().to_string()))?;
        let registration = entity.into_contractor_registration(uei.clone());

        let request_json = serde_json::to_string(request)
            .map_err(|e| SamGovError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&request_json);
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        // Don't include the api_key in the audit record. Strip the
        // query string from the URL we record.
        let scrubbed_url = scrub_api_key(&url);
        let observation = Observation {
            observation_id: format!("obs:sam-gov-live:{request_hash}"),
            request_hash,
            vendor: self.name().to_string(),
            model: "sam-gov-entities-v3".to_string(),
            latency_ms,
            cost_estimate: None,
            tokens: None,
            content: registration,
            raw_response: Some(format!("(fetched from {scrubbed_url})")),
        };

        Ok(SamGovResponse {
            records: vec![observation],
        })
    }
}

fn live_error(err: LiveError) -> SamGovError {
    match err {
        LiveError::NotFound(uei) => SamGovError::NotFound(uei),
        LiveError::Unauthorized => SamGovError::Transport(format!(
            "{API_KEY_ENV} rejected by api.sam.gov (401/403); check the key is current"
        )),
        LiveError::RateLimited => SamGovError::RateLimited,
        other => SamGovError::Transport(other.to_string()),
    }
}

fn build_lookup_url(base_url: &str, uei: &Uei, api_key: &str) -> String {
    format!(
        "{base}?ueiSAM={uei}&samRegistered=Yes&api_key={key}",
        base = base_url,
        uei = urlencoding::encode(uei.as_str()),
        key = urlencoding::encode(api_key),
    )
}

/// Strip the `api_key=...` query parameter so the URL is safe to
/// store in the observation's audit field. Keeps everything else.
fn scrub_api_key(url: &str) -> String {
    if let Some((path, query)) = url.split_once('?') {
        let scrubbed: Vec<&str> = query
            .split('&')
            .filter(|p| !p.starts_with("api_key="))
            .collect();
        if scrubbed.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{}", scrubbed.join("&"))
        }
    } else {
        url.to_string()
    }
}

async fn fetch_json(url: &str, opts: &LiveFetchOptions) -> Result<Vec<u8>, LiveError> {
    let url = url.to_string();
    let opts = opts.clone();
    tokio::task::spawn_blocking(move || {
        let provider = HttpFetchProvider::new()
            .into_live_fetch_provider()?
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
        match response.status {
            200 => {
                std::thread::sleep(POLITENESS_DELAY);
                Ok(response.body.into_bytes())
            }
            401 | 403 => Err(LiveError::Unauthorized),
            404 => Err(LiveError::NotFound(scrub_api_key(&response.url))),
            429 => Err(LiveError::RateLimited),
            _ => Err(LiveError::Http {
                status: response.status,
                url: scrub_api_key(&response.url),
            }),
        }
    })
    .await?
}

trait IntoLiveFetchProvider {
    fn into_live_fetch_provider(self) -> Result<HttpFetchProvider, LiveError>;
}

impl IntoLiveFetchProvider for HttpFetchProvider {
    fn into_live_fetch_provider(self) -> Result<HttpFetchProvider, LiveError> {
        Ok(self)
    }
}

impl<E: std::fmt::Display> IntoLiveFetchProvider for Result<HttpFetchProvider, E> {
    fn into_live_fetch_provider(self) -> Result<HttpFetchProvider, LiveError> {
        self.map_err(|err| LiveError::Fetch(err.to_string()))
    }
}

// ─────────────────────────── wire types ───────────────────────────

#[derive(Debug, Deserialize)]
struct EntitiesEnvelope {
    #[serde(rename = "entityData", default)]
    entity_data: Vec<WireEntity>,
    #[serde(default)]
    #[serde(rename = "totalRecords")]
    _total_records: Option<u64>,
}

impl EntitiesEnvelope {
    fn first_entity_for(self, uei: &Uei) -> Option<WireEntity> {
        let target = uei.as_str();
        self.entity_data
            .into_iter()
            .find(|e| e.entity_registration.uei_sam.as_deref() == Some(target))
    }
}

#[derive(Debug, Deserialize)]
struct WireEntity {
    #[serde(rename = "entityRegistration", default)]
    entity_registration: WireRegistration,
    #[serde(rename = "coreData", default)]
    _core_data: WireCoreData,
}

#[derive(Debug, Default, Deserialize)]
struct WireRegistration {
    #[serde(default, rename = "ueiSAM")]
    uei_sam: Option<String>,
    #[serde(default, rename = "legalBusinessName")]
    legal_business_name: Option<String>,
    #[serde(default, rename = "cageCode")]
    cage_code: Option<String>,
    #[serde(default, rename = "registrationStatus")]
    registration_status: Option<String>,
    #[serde(default, rename = "registrationExpirationDate")]
    registration_expiration_date: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WireCoreData {
    #[serde(default, rename = "entityInformation")]
    _entity_information: Option<serde_json::Value>,
}

impl WireEntity {
    fn into_contractor_registration(self, uei: Uei) -> ContractorRegistration {
        let reg = self.entity_registration;
        ContractorRegistration {
            uei,
            legal_name: reg
                .legal_business_name
                .unwrap_or_else(|| "(unknown legal name)".to_string()),
            registration_status: parse_status(reg.registration_status.as_deref()),
            expiration_date: reg.registration_expiration_date,
            cage_code: reg.cage_code,
        }
    }
}

fn parse_status(raw: Option<&str>) -> RegistrationStatus {
    match raw.unwrap_or("").to_ascii_lowercase().as_str() {
        "active" => RegistrationStatus::Active,
        "expired" => RegistrationStatus::Expired,
        "inactive" => RegistrationStatus::Inactive,
        "submitted" | "submitted - new" => RegistrationStatus::Submitted,
        _ => RegistrationStatus::Inactive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canned snippet matching api.sam.gov's `entity-information/v3/entities`
    /// response shape for one Active registration.
    const FIXTURE_ACTIVE: &str = r#"{
        "totalRecords": 1,
        "entityData": [{
            "entityRegistration": {
                "ueiSAM": "ABC123XYZ789",
                "legalBusinessName": "ACME FEDERAL CONTRACTOR LLC",
                "cageCode": "1A2B3",
                "registrationStatus": "Active",
                "registrationExpirationDate": "2027-06-01"
            },
            "coreData": {}
        }]
    }"#;

    const FIXTURE_EXPIRED: &str = r#"{
        "totalRecords": 1,
        "entityData": [{
            "entityRegistration": {
                "ueiSAM": "DEF456GHI789",
                "legalBusinessName": "Lapsed Holdings Inc.",
                "registrationStatus": "Expired",
                "registrationExpirationDate": "2024-01-15"
            },
            "coreData": {}
        }]
    }"#;

    const FIXTURE_EMPTY: &str = r#"{ "totalRecords": 0, "entityData": [] }"#;

    #[test]
    fn try_new_rejects_empty_key_loudly() {
        // Intent: a placeholder or whitespace key must fail at
        // construction; the alternative is a confusing 401 several
        // layers later that's harder to diagnose.
        assert!(LiveSamGovProvider::try_new("").is_err());
        assert!(LiveSamGovProvider::try_new("   ").is_err());
        assert!(LiveSamGovProvider::try_new("real-key").is_ok());
    }

    // Note: a test that verifies `from_env()`'s exact diagnostic when
    // the env var is unset would need to call `std::env::remove_var`,
    // which is `unsafe` in edition 2024 and forbidden by the workspace
    // lint. The diagnostic content is exercised indirectly via the
    // `try_new` empty-string test above plus inspection of the
    // formatted message — both keep the user-visible guidance honest.

    #[test]
    fn parse_active_envelope_into_typed_registration() {
        // Intent: every typed field must survive round-trip. Dropping
        // any one (cage_code, expiration_date) breaks downstream
        // eligibility checks.
        let env: EntitiesEnvelope = serde_json::from_str(FIXTURE_ACTIVE).unwrap();
        let uei = Uei::parse("ABC123XYZ789").unwrap();
        let entity = env.first_entity_for(&uei).unwrap();
        let reg = entity.into_contractor_registration(uei);
        assert_eq!(reg.legal_name, "ACME FEDERAL CONTRACTOR LLC");
        assert_eq!(reg.registration_status, RegistrationStatus::Active);
        assert_eq!(reg.expiration_date.as_deref(), Some("2027-06-01"));
        assert_eq!(reg.cage_code.as_deref(), Some("1A2B3"));
    }

    #[test]
    fn parse_status_handles_mixed_case_wire_values() {
        // Intent: api.sam.gov publishes status in TitleCase ("Active",
        // "Expired"); our typed enum is lower-snake. Bridge case
        // without losing the operational distinction between Expired
        // and Inactive — those drive different downstream decisions.
        assert_eq!(parse_status(Some("Active")), RegistrationStatus::Active);
        assert_eq!(parse_status(Some("EXPIRED")), RegistrationStatus::Expired);
        assert_eq!(parse_status(Some("Inactive")), RegistrationStatus::Inactive);
        assert_eq!(
            parse_status(Some("Submitted - New")),
            RegistrationStatus::Submitted
        );
        // Unknown future status falls through to Inactive — the safer
        // default for federal-award eligibility checks.
        assert_eq!(
            parse_status(Some("Something")),
            RegistrationStatus::Inactive
        );
    }

    #[test]
    fn empty_response_returns_no_entity() {
        let env: EntitiesEnvelope = serde_json::from_str(FIXTURE_EMPTY).unwrap();
        let uei = Uei::parse("ABC123XYZ789").unwrap();
        assert!(env.first_entity_for(&uei).is_none());
    }

    #[test]
    fn parses_expired_registration_without_cage_code() {
        // Intent: many lapsed entities never had a CAGE code. The
        // parser must produce `cage_code = None` rather than empty
        // string — they're operationally different (no DoD history vs
        // unknown).
        let env: EntitiesEnvelope = serde_json::from_str(FIXTURE_EXPIRED).unwrap();
        let uei = Uei::parse("DEF456GHI789").unwrap();
        let reg = env
            .first_entity_for(&uei)
            .unwrap()
            .into_contractor_registration(uei);
        assert_eq!(reg.registration_status, RegistrationStatus::Expired);
        assert!(reg.cage_code.is_none());
    }

    #[test]
    fn scrub_api_key_removes_only_the_api_key_param() {
        // Intent: the audit URL must not leak credentials. The scrubber
        // strips api_key but preserves the rest of the query so the
        // audit record is reproducible (you can grep for the UEI).
        let url = "https://api.sam.gov/entity-information/v3/entities?ueiSAM=ABC123XYZ789&samRegistered=Yes&api_key=secret123";
        let scrubbed = scrub_api_key(url);
        assert!(!scrubbed.contains("api_key"));
        assert!(!scrubbed.contains("secret123"));
        assert!(scrubbed.contains("ueiSAM=ABC123XYZ789"));
        assert!(scrubbed.contains("samRegistered=Yes"));
    }

    #[test]
    fn build_lookup_url_includes_api_key_and_uei() {
        let uei = Uei::parse("ABC123XYZ789").unwrap();
        let url = build_lookup_url(DEFAULT_BASE_URL, &uei, "my-key");
        assert!(url.contains("ueiSAM=ABC123XYZ789"));
        assert!(url.contains("api_key=my-key"));
        assert!(url.contains("samRegistered=Yes"));
    }

    /// Live network test — disabled by default. Run with:
    ///     SAM_GOV_LIVE_TEST=1 cargo test -p converge-embassy-sam-gov \
    ///         --features live -- --ignored live_lookup
    ///
    /// Requires SAM_GOV_API_KEY to be set. The api_key is free at
    /// api.data.gov/signup/.
    #[tokio::test]
    #[ignore = "live network call against api.sam.gov; opt-in only"]
    async fn live_lookup() {
        if std::env::var("SAM_GOV_LIVE_TEST").is_err() {
            eprintln!("Set SAM_GOV_LIVE_TEST=1 to run the network-bound live test.");
            return;
        }
        let provider = LiveSamGovProvider::from_env()
            .expect("SAM_GOV_API_KEY must be set when SAM_GOV_LIVE_TEST=1");
        // A well-known active UEI (NASA — JPL). If the UEI ages out
        // or status changes, the test only asserts the call succeeds
        // and returns *some* typed registration.
        let uei = Uei::parse("HJCUPXEUBJX5").unwrap();
        let req = SamGovRequest::Lookup { uei: uei.clone() };
        let resp = provider
            .lookup(&req, &embassy_pack::CallContext::default())
            .await;
        match resp {
            Ok(r) => assert_eq!(r.records.len(), 1),
            Err(SamGovError::NotFound(_)) => {} // acceptable: UEI may have rotated
            Err(other) => panic!("unexpected live failure: {other}"),
        }
    }
}
