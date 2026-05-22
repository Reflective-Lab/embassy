// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Live USAspending transport — HTTP fetch + JSON parse.
//!
//! Hits the free public USAspending endpoint at
//! `https://api.usaspending.gov/api/v2/awards/{award_id}/`. Federal
//! spend data is open by statute (FFATA / DATA Act); no auth or key
//! is required.
//!
//! Gated behind the `live` feature so the default `embassy-usaspending`
//! surface (typed domain + stub provider) stays dependency-light.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use manifold::{HttpFetchProvider, WebFetchBackend, WebFetchRequest};
use serde::Deserialize;
use thiserror::Error;

use crate::error::UsaspendingError;
use crate::provider::{UsaspendingProvider, UsaspendingRequest, UsaspendingResponse};
use crate::types::{AwardId, FederalAward};
use crate::{Observation, content_hash};

pub const DEFAULT_BASE_URL: &str = "https://api.usaspending.gov/api/v2";
pub const DEFAULT_USER_AGENT: &str = "Reflective Labs Research kpernyer@gmail.com";
pub const DEFAULT_MAX_BYTES: usize = 2 * 1024 * 1024;
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const POLITENESS_DELAY: Duration = Duration::from_millis(100);

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
    #[error("award not found: {0}")]
    NotFound(String),
    #[error("blocking task join failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Live USAspending provider for the source-shaped provider trait.
#[derive(Debug, Clone, Default)]
pub struct LiveUsaspendingProvider {
    options: LiveFetchOptions,
}

impl LiveUsaspendingProvider {
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
impl UsaspendingProvider for LiveUsaspendingProvider {
    fn name(&self) -> &'static str {
        "live_usaspending"
    }

    async fn lookup(
        &self,
        request: &UsaspendingRequest,
        _ctx: &embassy_pack::CallContext,
    ) -> Result<UsaspendingResponse, UsaspendingError> {
        let started = Instant::now();
        let UsaspendingRequest::Lookup { award_id } = request;

        let url = format!("{}/awards/{}/", self.options.base_url, award_id.as_str());
        let body = fetch_json(&url, &self.options).await.map_err(live_error)?;
        let parsed: WireAward = serde_json::from_slice(&body)
            .map_err(|e| UsaspendingError::Transport(format!("parse failed: {e}")))?;
        let award = parsed.into_federal_award(award_id.clone());

        let request_json = serde_json::to_string(request).map_err(|e| {
            UsaspendingError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&request_json);
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        let observation = Observation {
            observation_id: format!("obs:usaspending-live:{request_hash}"),
            request_hash,
            vendor: self.name().to_string(),
            model: "usaspending-api-v2".to_string(),
            latency_ms,
            cost_estimate: None,
            tokens: None,
            content: award,
            raw_response: Some(String::from_utf8_lossy(&body).to_string()),
        };

        Ok(UsaspendingResponse {
            records: vec![observation],
        })
    }
}

fn live_error(err: LiveError) -> UsaspendingError {
    match err {
        LiveError::NotFound(id) => UsaspendingError::NotFound(id),
        other => UsaspendingError::Transport(other.to_string()),
    }
}

async fn fetch_json(url: &str, opts: &LiveFetchOptions) -> Result<Vec<u8>, LiveError> {
    let url = url.to_string();
    let opts = opts.clone();
    tokio::task::spawn_blocking(move || {
        let provider = HttpFetchProvider::new().with_user_agent(&opts.user_agent);
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

#[derive(Debug, Default, Deserialize)]
struct WireAward {
    #[serde(default)]
    recipient: WireRecipient,
    #[serde(default)]
    total_obligation: Option<f64>,
    #[serde(default)]
    awarding_agency: WireAgency,
    #[serde(default)]
    period_of_performance: WirePeriod,
}

#[derive(Debug, Default, Deserialize)]
struct WireRecipient {
    #[serde(default)]
    recipient_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WireAgency {
    #[serde(default)]
    toptier_agency: WireToptierAgency,
}

#[derive(Debug, Default, Deserialize)]
struct WireToptierAgency {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WirePeriod {
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
}

impl WireAward {
    fn into_federal_award(self, award_id: AwardId) -> FederalAward {
        // Convert dollars (f64) to micros (i64). Rounding is best-effort;
        // upstream values are typically whole cents.
        let total_obligated_micros = self
            .total_obligation
            .map(|dollars| (dollars * 1_000_000.0).round() as i64)
            .unwrap_or(0);
        FederalAward {
            award_id,
            recipient_name: self
                .recipient
                .recipient_name
                .unwrap_or_else(|| "(unknown recipient)".to_string()),
            total_obligated_micros,
            awarding_agency: self
                .awarding_agency
                .toptier_agency
                .name
                .unwrap_or_else(|| "(unknown agency)".to_string()),
            period_of_performance_start: self.period_of_performance.start_date,
            period_of_performance_end: self.period_of_performance.end_date,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Canned snippet mirroring api.usaspending.gov's award response.
    /// Only the fields the typed `FederalAward` consumes appear; many
    /// other fields are present in real responses and are ignored.
    const FIXTURE: &str = r#"{
        "id": 12345,
        "generated_unique_award_id": "CONT_AWD_TEST_001",
        "recipient": {"recipient_name": "ACME CONTRACTING LLC"},
        "total_obligation": 1234567.89,
        "awarding_agency": {
            "toptier_agency": {"name": "Department of Defense"}
        },
        "period_of_performance": {
            "start_date": "2025-01-15",
            "end_date": "2026-01-14"
        }
    }"#;

    #[test]
    fn fixture_parses_into_typed_federal_award() {
        // Intent: every field on the typed surface must survive the
        // round-trip from JSON. Dropping any one (recipient_name,
        // amount, agency, period) breaks the downstream audit.
        let wire: WireAward = serde_json::from_str(FIXTURE).unwrap();
        let award_id = AwardId::parse("CONT_AWD_TEST_001").unwrap();
        let award = wire.into_federal_award(award_id);
        assert_eq!(award.recipient_name, "ACME CONTRACTING LLC");
        assert_eq!(award.total_obligated_micros, 1_234_567_890_000);
        assert_eq!(award.awarding_agency, "Department of Defense");
        assert_eq!(award.period_of_performance_start.as_deref(), Some("2025-01-15"));
        assert_eq!(award.period_of_performance_end.as_deref(), Some("2026-01-14"));
    }

    #[test]
    fn missing_amount_collapses_to_zero_micros_not_panic() {
        // Intent: a real-world response sometimes omits total_obligation
        // (early-stage awards). The parser must produce a typed value
        // with zero micros, not panic — zero is queryable and a real
        // signal, panic is not.
        let stripped = r#"{
            "recipient": {"recipient_name": "ACME"},
            "awarding_agency": {"toptier_agency": {"name": "DoD"}},
            "period_of_performance": {}
        }"#;
        let wire: WireAward = serde_json::from_str(stripped).unwrap();
        let award_id = AwardId::parse("CONT_AWD_TEST_002").unwrap();
        let award = wire.into_federal_award(award_id);
        assert_eq!(award.total_obligated_micros, 0);
    }

    /// Live network test — disabled by default. Run with:
    ///     USASPENDING_LIVE_TEST=1 cargo test -p converge-embassy-usaspending \
    ///         --features live -- --ignored live_lookup
    #[tokio::test]
    #[ignore = "live network call against api.usaspending.gov; opt-in only"]
    async fn live_lookup() {
        if std::env::var("USASPENDING_LIVE_TEST").is_err() {
            eprintln!("Set USASPENDING_LIVE_TEST=1 to run the network-bound live test.");
            return;
        }
        let provider = LiveUsaspendingProvider::new();
        // The award ID used here is illustrative; the test only verifies
        // the call shape against the real endpoint. A 404 means the
        // award no longer exists and is also acceptable.
        let req = UsaspendingRequest::Lookup {
            award_id: AwardId::parse("CONT_AWD_FA8501-19-D-0001_9700_-NONE-_-NONE-").unwrap(),
        };
        let result = provider.lookup(&req, &embassy_pack::CallContext::default()).await;
        match result {
            Ok(resp) => assert_eq!(resp.records.len(), 1),
            Err(UsaspendingError::NotFound(_)) => {} // acceptable — proves the round-trip
            Err(other) => panic!("unexpected live failure: {other}"),
        }
    }
}
