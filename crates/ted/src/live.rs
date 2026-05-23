// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Live TED transport — cursor-paginated POST search.
//!
//! Calls the TED v3 search API at
//! `https://api.ted.europa.eu/v3.0/notices/search`. The endpoint is
//! free and unauthenticated for public reads; rate limiting applies
//! per IP. The TED API path has evolved across releases; the URL is
//! configurable via [`LiveFetchOptions::search_url`] so consumers can
//! pin it as the endpoint changes.
//!
//! Two request shapes are supported:
//! - [`TedRequest::Lookup`] — search with an `ND=<id>` query filter
//!   and return the first matching notice (or `NotFound`).
//! - [`TedRequest::SearchByCountry`] — paginate through results in
//!   the given country up to the caller's `limit`. Uses
//!   [`manifold::pagination::paginate`] with a cursor strategy on
//!   the `iterationNextToken` field of the response.
//!
//! ## REAL-by-default boundary
//!
//! - [`LiveTedProvider::new`] is infallible — TED requires no
//!   credentials. The first request can still fail (network down,
//!   schema drift); failures are typed `TedError::Transport`. Never
//!   silent stub fallback.
//! - Pagination is capped at 50 pages by default (5,000 records at
//!   the 100-per-page default size). Caps surface as typed errors
//!   via [`manifold::pagination::PaginationError::MaxPagesReached`]
//!   so audit can detect partial pulls.

use std::time::Instant;

use async_trait::async_trait;
use manifold::pagination::{PaginationConfig, paginate};
use manifold::{HttpFetchProvider, WebFetchRequest, WebFetchResponse};
use serde::Deserialize;
use thiserror::Error;

use crate::error::TedError;
use crate::provider::{TedProvider, TedRequest, TedResponse};
use crate::types::{ProcurementNotice, ProcurementType, TedNoticeId};
use crate::{Observation, content_hash};

/// TED v3 search endpoint. The API path is `/v3/` (not `/v3.0/` — the
/// latter responds 404). Override via [`LiveFetchOptions::search_url`]
/// if TED publishes a new path.
pub const DEFAULT_SEARCH_URL: &str = "https://api.ted.europa.eu/v3/notices/search";

pub const DEFAULT_USER_AGENT: &str = "Reflective Labs Research kpernyer@gmail.com";
pub const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;
pub const DEFAULT_PAGE_SIZE: usize = 100;
pub const DEFAULT_MAX_PAGES: usize = 50;

#[derive(Debug, Clone)]
pub struct LiveFetchOptions {
    pub search_url: String,
    pub user_agent: String,
    pub max_bytes: usize,
    pub timeout_ms: u64,
    pub page_size: usize,
    pub max_pages: usize,
}

impl Default for LiveFetchOptions {
    fn default() -> Self {
        Self {
            search_url: DEFAULT_SEARCH_URL.to_string(),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            max_bytes: DEFAULT_MAX_BYTES,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            page_size: DEFAULT_PAGE_SIZE,
            max_pages: DEFAULT_MAX_PAGES,
        }
    }
}

#[derive(Debug, Error)]
pub enum LiveError {
    #[error("fetch failed: {0}")]
    Fetch(String),
    #[error("pagination failed: {0}")]
    Pagination(String),
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    #[error("notice {0} not found")]
    NotFound(String),
    #[error("response parse failed: {0}")]
    Parse(String),
    #[error("blocking task join failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

#[derive(Debug, Clone, Default)]
pub struct LiveTedProvider {
    options: LiveFetchOptions,
}

impl LiveTedProvider {
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
impl TedProvider for LiveTedProvider {
    fn name(&self) -> &'static str {
        "live_ted"
    }

    async fn lookup(
        &self,
        request: &TedRequest,
        _ctx: &embassy_pack::CallContext,
    ) -> Result<TedResponse, TedError> {
        let started = Instant::now();
        let opts = self.options.clone();
        let request_clone = request.clone();
        let pages = tokio::task::spawn_blocking(move || run_search(&opts, &request_clone))
            .await
            .map_err(|e| TedError::Transport(format!("join: {e}")))?
            .map_err(live_error)?;

        let notices: Vec<ProcurementNotice> = pages
            .iter()
            .flat_map(|p| parse_page_notices(&p.body))
            .collect();

        let request_json = serde_json::to_string(request)
            .map_err(|e| TedError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&request_json);
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        let records: Vec<Observation<ProcurementNotice>> = match request {
            TedRequest::Lookup { notice_id } => {
                let notice = notices
                    .into_iter()
                    .find(|n| &n.notice_id == notice_id)
                    .ok_or_else(|| TedError::NotFound(notice_id.as_str().to_string()))?;
                vec![Observation {
                    observation_id: format!("obs:ted-live:{request_hash}"),
                    request_hash,
                    vendor: self.name().to_string(),
                    model: "ted-search-v3".to_string(),
                    latency_ms,
                    cost_estimate: None,
                    tokens: None,
                    content: notice,
                    raw_response: None,
                }]
            }
            TedRequest::SearchByCountry { limit, .. }
            | TedRequest::SearchByBuyerName { limit, .. } => notices
                .into_iter()
                .take(*limit)
                .enumerate()
                .map(|(i, notice)| Observation {
                    observation_id: format!("obs:ted-live:{request_hash}:{i}"),
                    request_hash: request_hash.clone(),
                    vendor: self.name().to_string(),
                    model: "ted-search-v3".to_string(),
                    latency_ms,
                    cost_estimate: None,
                    tokens: None,
                    content: notice,
                    raw_response: None,
                })
                .collect(),
        };

        Ok(TedResponse { records })
    }
}

fn live_error(err: LiveError) -> TedError {
    match err {
        LiveError::NotFound(id) => TedError::NotFound(id),
        other => TedError::Transport(other.to_string()),
    }
}

/// Run the paginated search synchronously inside a blocking task.
/// Returns every page body fetched so the caller can parse + cap.
fn run_search(
    opts: &LiveFetchOptions,
    request: &TedRequest,
) -> Result<Vec<WebFetchResponse>, LiveError> {
    let backend = HttpFetchProvider::new()
        .map_err(|e| LiveError::Fetch(e.to_string()))?
        .with_user_agent(&opts.user_agent);

    // For Lookup, the per-page count of pages we ever need to fetch
    // is 1 (the query filter `publication-number=<id>` is tight). For
    // SearchByCountry, we set a soft target via the advance closure
    // (stop once we have `limit` notices) and a hard cap via
    // PaginationConfig.max_pages.
    let body0 = build_search_body(request, opts.page_size, None);
    let initial = build_fetch_request(&opts.search_url, opts, &body0)?;

    // The advance closure caps the loop in two ways:
    // 1. Lookup ever needs more than one page → return None after
    //    the first response.
    // 2. SearchByCountry tracks cumulative notice count and returns
    //    None once `limit` is reached. The PaginationConfig cap is
    //    a safety net that fires only on truly large pulls.
    let opts_for_closure = opts.clone();
    let req_for_closure = request.clone();
    let target_limit = match request {
        TedRequest::Lookup { .. } => 1,
        TedRequest::SearchByCountry { limit, .. } | TedRequest::SearchByBuyerName { limit, .. } => {
            *limit
        }
    };
    let mut cumulative = 0usize;
    let pages = paginate(
        &backend,
        initial,
        |prior| {
            cumulative += parse_page_notices(&prior.body).len();
            // Lookup is tight: one page is always enough.
            if matches!(req_for_closure, TedRequest::Lookup { .. }) {
                return None;
            }
            // SearchByCountry: stop once we've covered the target.
            if cumulative >= target_limit {
                return None;
            }
            let token = match extract_next_token(&prior.body) {
                Ok(Some(t)) => t,
                Ok(None) => return None,
                Err(e) => return Some(Err(e)),
            };
            let body =
                build_search_body(&req_for_closure, opts_for_closure.page_size, Some(&token));
            let req = build_fetch_request(&opts_for_closure.search_url, &opts_for_closure, &body)
                .map_err(|e| e.to_string());
            Some(req)
        },
        PaginationConfig::default().with_max_pages(opts.max_pages),
    )
    .map_err(|e| LiveError::Pagination(e.to_string()))?;

    Ok(pages)
}

fn build_fetch_request(
    url: &str,
    opts: &LiveFetchOptions,
    body: &str,
) -> Result<WebFetchRequest, LiveError> {
    WebFetchRequest::new(url)
        .map_err(|e| LiveError::Fetch(e.to_string()))?
        .with_max_bytes(opts.max_bytes)
        .map_err(|e| LiveError::Fetch(e.to_string()))?
        .with_timeout_ms(opts.timeout_ms)
        .map_err(|e| LiveError::Fetch(e.to_string()))
        .map(|r| {
            r.with_header("Content-Type", "application/json")
                .with_header("Accept", "application/json")
                .with_body(body.to_string())
        })
}

/// Build a TED v3 search request body using the eForms business-term
/// field names. Required parameters:
/// - `query`: expert-search filter, e.g. `publication-number=*` or
///   `organisation-country-buyer=SWE` (uses 3-letter ISO codes).
/// - `fields`: list of eForms business terms to project.
/// - `limit`: page size cap.
/// - `paginationMode`: must be `"ITERATION"` for cursor tokens to
///   appear in the response. Without it the server returns a single
///   non-resumable page even when more data exists upstream.
fn build_search_body(request: &TedRequest, page_size: usize, cursor: Option<&str>) -> String {
    let query = match request {
        TedRequest::Lookup { notice_id } => {
            format!("publication-number={}", notice_id.as_str())
        }
        TedRequest::SearchByCountry { country, .. } => {
            // TED v3 uses ISO 3166-1 alpha-3 country codes; the typed
            // `SearchByCountry.country` is documented as alpha-2 so
            // bridge here at the boundary.
            let alpha3 = alpha2_to_alpha3(country);
            format!("organisation-country-buyer={alpha3}")
        }
        TedRequest::SearchByBuyerName { buyer_name, .. } => {
            // TED v3 `buyer-name=` is exact-match (no wildcards). The
            // caller must supply the canonical legal name as it
            // appears in publications.
            format!("buyer-name={buyer_name}")
        }
    };
    let cursor_field = match cursor {
        Some(token) => format!(r#","iterationNextToken":"{}""#, escape_json_string(token)),
        None => String::new(),
    };
    format!(
        r#"{{"query":"{query}","fields":["publication-number","notice-title","buyer-name","notice-type","organisation-country-buyer","deadline-receipt-tender-date-lot"],"limit":{page_size},"paginationMode":"ITERATION"{cursor_field}}}"#,
        query = escape_json_string(&query),
        cursor_field = cursor_field,
        page_size = page_size,
    )
}

/// Convert an ISO 3166-1 alpha-2 country code to alpha-3 for TED's
/// query syntax. EU + EEA + UK coverage; unknown inputs are uppercased
/// and passed through so the API rejects them (rather than silently
/// degrading to a different country).
fn alpha2_to_alpha3(alpha2: &str) -> String {
    match alpha2.to_ascii_uppercase().as_str() {
        "AT" => "AUT".into(),
        "BE" => "BEL".into(),
        "BG" => "BGR".into(),
        "CY" => "CYP".into(),
        "CZ" => "CZE".into(),
        "DE" => "DEU".into(),
        "DK" => "DNK".into(),
        "EE" => "EST".into(),
        "ES" => "ESP".into(),
        "FI" => "FIN".into(),
        "FR" => "FRA".into(),
        "GR" | "EL" => "GRC".into(),
        "HR" => "HRV".into(),
        "HU" => "HUN".into(),
        "IE" => "IRL".into(),
        "IT" => "ITA".into(),
        "LT" => "LTU".into(),
        "LU" => "LUX".into(),
        "LV" => "LVA".into(),
        "MT" => "MLT".into(),
        "NL" => "NLD".into(),
        "PL" => "POL".into(),
        "PT" => "PRT".into(),
        "RO" => "ROU".into(),
        "SE" => "SWE".into(),
        "SI" => "SVN".into(),
        "SK" => "SVK".into(),
        "GB" | "UK" => "GBR".into(),
        "NO" => "NOR".into(),
        "IS" => "ISL".into(),
        "LI" => "LIE".into(),
        "CH" => "CHE".into(),
        other => other.to_string(),
    }
}

/// Inverse: alpha-3 → alpha-2 for surfacing back through the typed
/// `ProcurementNotice.country` field (documented as alpha-2).
fn alpha3_to_alpha2(alpha3: &str) -> String {
    match alpha3.to_ascii_uppercase().as_str() {
        "AUT" => "AT".into(),
        "BEL" => "BE".into(),
        "BGR" => "BG".into(),
        "CYP" => "CY".into(),
        "CZE" => "CZ".into(),
        "DEU" => "DE".into(),
        "DNK" => "DK".into(),
        "EST" => "EE".into(),
        "ESP" => "ES".into(),
        "FIN" => "FI".into(),
        "FRA" => "FR".into(),
        "GRC" => "EL".into(),
        "HRV" => "HR".into(),
        "HUN" => "HU".into(),
        "IRL" => "IE".into(),
        "ITA" => "IT".into(),
        "LTU" => "LT".into(),
        "LUX" => "LU".into(),
        "LVA" => "LV".into(),
        "MLT" => "MT".into(),
        "NLD" => "NL".into(),
        "POL" => "PL".into(),
        "PRT" => "PT".into(),
        "ROU" => "RO".into(),
        "SWE" => "SE".into(),
        "SVN" => "SI".into(),
        "SVK" => "SK".into(),
        "GBR" => "GB".into(),
        "NOR" => "NO".into(),
        "ISL" => "IS".into(),
        "LIE" => "LI".into(),
        "CHE" => "CH".into(),
        other => other.to_string(),
    }
}

fn escape_json_string(s: &str) -> String {
    s.replace('\\', r"\\").replace('"', r#"\""#)
}

/// Extract the `iterationNextToken` from a response body. Returns
/// `Ok(None)` when there is no next page (token absent or empty);
/// returns `Err(msg)` if the response body isn't JSON.
fn extract_next_token(body: &str) -> Result<Option<String>, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("response not JSON: {e}"))?;
    let token = parsed.get("iterationNextToken").and_then(|v| v.as_str());
    Ok(token.filter(|t| !t.is_empty()).map(String::from))
}

/// Parse one page of search response into typed notices. Skips
/// malformed entries rather than failing the whole page — TED's
/// schema has long-tail variability and a partial result is more
/// useful than a hard failure.
fn parse_page_notices(body: &str) -> Vec<ProcurementNotice> {
    let parsed: WirePage = match serde_json::from_str(body) {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };
    parsed
        .notices
        .into_iter()
        .filter_map(|wire| wire.into_procurement_notice().ok())
        .collect()
}

#[derive(Debug, Deserialize)]
struct WirePage {
    #[serde(default)]
    notices: Vec<WireNotice>,
    #[serde(default, rename = "iterationNextToken")]
    _iteration_next_token: Option<String>,
    #[serde(default, rename = "totalNoticeCount")]
    _total_notice_count: Option<u64>,
}

/// TED v3 response shape. Uses eForms business-term field names —
/// these replaced the old short codes (ND/TI/AA/RC/TD/DT) in the v3
/// migration. Multi-language fields come as objects keyed by 3-letter
/// language code, each value an array of strings.
#[derive(Debug, Deserialize)]
struct WireNotice {
    #[serde(rename = "publication-number")]
    publication_number: Option<String>,
    #[serde(rename = "notice-title")]
    notice_title: Option<serde_json::Value>,
    #[serde(rename = "buyer-name")]
    buyer_name: Option<serde_json::Value>,
    /// Buyer's country as ISO 3166-1 alpha-3 ("SWE", "DEU", ...).
    /// Surfaces back through the typed `country` field as alpha-2.
    #[serde(rename = "organisation-country-buyer")]
    organisation_country_buyer: Option<serde_json::Value>,
    /// eForms notice-type code, e.g. `"cn-standard"`, `"pin-standard"`,
    /// `"can-standard"`. String, not numeric.
    #[serde(rename = "notice-type")]
    notice_type: Option<serde_json::Value>,
    #[serde(rename = "deadline-receipt-tender-date-lot")]
    deadline_receipt_tender_date_lot: Option<serde_json::Value>,
}

impl WireNotice {
    fn into_procurement_notice(self) -> Result<ProcurementNotice, LiveError> {
        let pubnum = self
            .publication_number
            .ok_or_else(|| LiveError::Parse("missing publication-number".into()))?;
        let notice_id = TedNoticeId::parse(&pubnum)
            .map_err(|e| LiveError::Parse(format!("invalid publication-number `{pubnum}`: {e}")))?;
        let title = first_string(self.notice_title.as_ref())
            .unwrap_or_else(|| "(untitled)".to_string());
        let contracting_authority = first_string(self.buyer_name.as_ref())
            .unwrap_or_else(|| "(unknown authority)".to_string());
        let country = match first_string(self.organisation_country_buyer.as_ref()) {
            Some(alpha3) => alpha3_to_alpha2(&alpha3),
            None => "??".to_string(),
        };
        let procurement_type = match first_string(self.notice_type.as_ref()) {
            Some(s) => parse_procurement_type(&s),
            None => ProcurementType::Other,
        };
        let deadline = first_string(self.deadline_receipt_tender_date_lot.as_ref());

        Ok(ProcurementNotice {
            notice_id,
            contracting_authority,
            title,
            country,
            procurement_type,
            deadline,
        })
    }
}

/// TED fields are often multi-language objects like
/// `{"eng": "...", "swe": "..."}` or arrays. Pick the first non-empty
/// string value we can find, in document order.
fn first_string(value: Option<&serde_json::Value>) -> Option<String> {
    let v = value?;
    match v {
        serde_json::Value::String(s) => (!s.is_empty()).then(|| s.clone()),
        serde_json::Value::Array(arr) => arr.iter().find_map(|item| first_string(Some(item))),
        serde_json::Value::Object(map) => map.values().find_map(|item| first_string(Some(item))),
        _ => None,
    }
}

/// Map TED's eForms `notice-type` codes to typed enum. The codes are
/// stable across the v3 release. Unknown codes fall through to
/// `ProcurementType::Other` rather than failing parse — TED publishes
/// long-tail variants for design contests, corrigenda, ex-ante
/// transparency, etc. that we don't enumerate.
///
/// Common prefixes (per the eForms 2024 taxonomy):
/// - `pin-*` — prior information notices (`pin-standard`, `pin-rtl`).
/// - `cn-*` — contract notices (`cn-standard`, `cn-social`, `cn-desg`).
/// - `can-*` — contract award notices (`can-standard`, `can-social`).
/// - `mod-*` — contract modifications.
fn parse_procurement_type(code: &str) -> ProcurementType {
    let lower = code.trim().to_ascii_lowercase();
    if lower.starts_with("pin-") {
        ProcurementType::PriorInformation
    } else if lower.starts_with("cn-") {
        ProcurementType::ContractNotice
    } else if lower.starts_with("can-") {
        ProcurementType::ContractAwardNotice
    } else if lower.starts_with("mod-") {
        ProcurementType::Modification
    } else {
        ProcurementType::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One-notice fixture mirroring the TED v3 response shape we
    /// consume. Real responses carry ~1830 possible fields; only the
    /// ones the typed surface uses appear here. Multi-language text
    /// fields come as objects keyed by 3-letter language code with
    /// array values — captured exactly.
    const FIXTURE_SINGLE: &str = r#"{
        "notices": [{
            "publication-number": "123456-2026",
            "notice-title": {
                "eng": ["Construction of municipal water mains"],
                "swe": ["Bygg av kommunala vattenledningar"]
            },
            "buyer-name": {"swe": ["Stockholms stad"]},
            "organisation-country-buyer": "SWE",
            "notice-type": "cn-standard",
            "deadline-receipt-tender-date-lot": "2026-09-15T23:59:59+02:00"
        }],
        "totalNoticeCount": 1,
        "iterationNextToken": null,
        "timedOut": false
    }"#;

    const FIXTURE_PAGE_ONE: &str = r#"{
        "notices": [
            {"publication-number": "100001-2026", "notice-title": {"eng": ["Notice 1"]}, "buyer-name": "Authority A", "organisation-country-buyer": "DEU", "notice-type": "cn-standard"},
            {"publication-number": "100002-2026", "notice-title": {"eng": ["Notice 2"]}, "buyer-name": "Authority B", "organisation-country-buyer": "DEU", "notice-type": "can-standard"}
        ],
        "iterationNextToken": "page2token",
        "totalNoticeCount": 3
    }"#;

    const FIXTURE_PAGE_TWO: &str = r#"{
        "notices": [
            {"publication-number": "100003-2026", "notice-title": {"eng": ["Notice 3"]}, "buyer-name": "Authority C", "organisation-country-buyer": "DEU", "notice-type": "cn-standard"}
        ],
        "iterationNextToken": "",
        "totalNoticeCount": 3
    }"#;

    #[test]
    fn parse_single_notice_maps_all_typed_fields() {
        // Intent: every field on the typed surface must survive the
        // wire → typed conversion. Dropping any one (deadline,
        // country, procurement_type) breaks downstream consumers.
        // Country must round-trip from TED's alpha-3 ("SWE") to the
        // typed surface's alpha-2 ("SE").
        let notices = parse_page_notices(FIXTURE_SINGLE);
        assert_eq!(notices.len(), 1);
        let n = &notices[0];
        assert_eq!(n.notice_id.as_str(), "123456-2026");
        assert_eq!(n.title, "Construction of municipal water mains");
        assert_eq!(n.contracting_authority, "Stockholms stad");
        assert_eq!(n.country, "SE");
        assert_eq!(n.procurement_type, ProcurementType::ContractNotice);
        assert_eq!(n.deadline.as_deref(), Some("2026-09-15T23:59:59+02:00"));
    }

    #[test]
    fn alpha2_alpha3_round_trip_for_eu_states() {
        // Intent: every EU-27 + EEA + UK country code must round-trip
        // cleanly. A break here means TED's response country can't be
        // matched against the typed surface — a silent downstream gap.
        for cc in [
            "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "ES", "FI", "FR", "EL", "HR", "HU",
            "IE", "IT", "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK", "GB",
            "NO", "IS", "LI", "CH",
        ] {
            let alpha3 = alpha2_to_alpha3(cc);
            assert_ne!(alpha3, cc, "alpha-2 `{cc}` must map to a different alpha-3");
            assert_eq!(
                alpha3_to_alpha2(&alpha3),
                cc,
                "round-trip broken for `{cc}`"
            );
        }
    }

    #[test]
    fn extract_next_token_recognises_empty_string_as_no_more_pages() {
        // Intent: an empty token must terminate pagination — TED
        // sometimes returns `""` instead of omitting the field.
        assert_eq!(
            extract_next_token(r#"{"iterationNextToken":""}"#).unwrap(),
            None
        );
        assert_eq!(
            extract_next_token(r#"{"iterationNextToken":null}"#).unwrap(),
            None
        );
        assert_eq!(
            extract_next_token(r#"{"iterationNextToken":"abc"}"#)
                .unwrap()
                .as_deref(),
            Some("abc")
        );
    }

    #[test]
    fn extract_next_token_non_json_response_surfaces_error() {
        // Intent: an HTML error page or a truncated response must
        // surface as a typed error, not silently terminate the loop.
        assert!(extract_next_token("<html>nope</html>").is_err());
    }

    #[test]
    fn procurement_type_maps_eforms_prefixes_and_falls_through_for_unknown() {
        // Intent: eForms taxonomy uses string codes with stable
        // prefixes (`cn-*`, `can-*`, `pin-*`, `mod-*`). Capture the
        // prefix, not specific codes, so the parser keeps working
        // when TED introduces new taxonomy entries.
        assert_eq!(
            parse_procurement_type("cn-standard"),
            ProcurementType::ContractNotice
        );
        assert_eq!(
            parse_procurement_type("cn-social"),
            ProcurementType::ContractNotice
        );
        assert_eq!(
            parse_procurement_type("can-standard"),
            ProcurementType::ContractAwardNotice
        );
        assert_eq!(
            parse_procurement_type("pin-standard"),
            ProcurementType::PriorInformation
        );
        assert_eq!(
            parse_procurement_type("pin-rtl"),
            ProcurementType::PriorInformation
        );
        assert_eq!(
            parse_procurement_type("mod-1"),
            ProcurementType::Modification
        );
        assert_eq!(
            parse_procurement_type("design-contest"),
            ProcurementType::Other
        );
        assert_eq!(parse_procurement_type(""), ProcurementType::Other);
    }

    #[test]
    fn search_body_includes_pagination_mode_and_cursor() {
        // Intent: pagination correctness on TED v3 — the request must
        // carry `paginationMode: "ITERATION"` for the server to issue
        // continuation tokens at all, and the cursor must round-trip.
        let req = TedRequest::SearchByCountry {
            country: "DE".to_string(),
            limit: 100,
        };
        let body = build_search_body(&req, 50, None);
        assert!(body.contains(r#""query":"organisation-country-buyer=DEU""#));
        assert!(body.contains(r#""limit":50"#));
        assert!(body.contains(r#""paginationMode":"ITERATION""#));
        assert!(!body.contains("iterationNextToken"));

        let with_cursor = build_search_body(&req, 50, Some("abc123"));
        assert!(with_cursor.contains(r#""iterationNextToken":"abc123""#));
    }

    #[test]
    fn search_body_for_lookup_uses_publication_number_filter() {
        // Intent: a notice-id lookup must filter to that exact
        // publication-number so a single result comes back even
        // though the search endpoint is broader than a point-query API.
        let id = TedNoticeId::parse("999888-2026").unwrap();
        let req = TedRequest::Lookup { notice_id: id };
        let body = build_search_body(&req, 100, None);
        assert!(body.contains(r#""query":"publication-number=999888-2026""#));
    }

    #[test]
    fn first_string_walks_multilang_objects() {
        // Intent: TED uses multi-language objects and arrays for
        // strings. The helper must descend into them rather than
        // returning empty.
        let multilang = serde_json::json!({"eng": "english title", "swe": "svensk titel"});
        let s = first_string(Some(&multilang));
        // Order in JSON object is preserved in serde_json's Map; the
        // first key inserted ("eng") is returned.
        assert_eq!(s.as_deref(), Some("english title"));

        let array = serde_json::json!(["", "first non-empty"]);
        assert_eq!(
            first_string(Some(&array)).as_deref(),
            Some("first non-empty")
        );
    }

    #[test]
    fn malformed_notice_in_page_is_skipped_not_fatal() {
        // Intent: TED has long-tail schema variability; a partial
        // page of typed notices is more useful than zero. The page
        // parser drops malformed entries and keeps the rest.
        let mixed = r#"{
            "notices": [
                {"publication-number": "notvalid", "notice-title": "bad", "buyer-name": "x", "organisation-country-buyer": "DEU", "notice-type": "cn-standard"},
                {"publication-number": "200002-2026", "notice-title": "good", "buyer-name": "y", "organisation-country-buyer": "DEU", "notice-type": "cn-standard"}
            ],
            "iterationNextToken": null
        }"#;
        let notices = parse_page_notices(mixed);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].notice_id.as_str(), "200002-2026");
    }

    #[test]
    fn parse_two_pages_yields_combined_notices() {
        // Intent: the lookup path joins paged results. Three notices
        // total across two pages must produce three typed notices in
        // document order.
        let mut all: Vec<ProcurementNotice> = Vec::new();
        all.extend(parse_page_notices(FIXTURE_PAGE_ONE));
        all.extend(parse_page_notices(FIXTURE_PAGE_TWO));
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].notice_id.as_str(), "100001-2026");
        assert_eq!(all[2].notice_id.as_str(), "100003-2026");
    }

    /// Live network test — disabled by default. Run with:
    ///     TED_LIVE_TEST=1 cargo test -p converge-embassy-ted \
    ///         --features live -- --ignored live_search
    ///
    /// Verified against `api.ted.europa.eu/v3/notices/search` on
    /// 2026-05-23. If TED moves the path again, this test will fail
    /// loudly and the fix is to update DEFAULT_SEARCH_URL.
    #[tokio::test]
    #[ignore = "live network call to TED v3 API; opt-in only"]
    async fn live_search() {
        if std::env::var("TED_LIVE_TEST").is_err() {
            eprintln!("Set TED_LIVE_TEST=1 to run the network-bound live test.");
            return;
        }
        let provider = LiveTedProvider::new();
        let req = TedRequest::SearchByCountry {
            country: "SE".to_string(),
            limit: 5,
        };
        let resp = provider
            .lookup(&req, &embassy_pack::CallContext::default())
            .await
            .expect("live TED search must succeed; update DEFAULT_SEARCH_URL if TED moved");
        // SE has tens of thousands of notices — must get at least 1.
        assert!(
            !resp.records.is_empty(),
            "expected >=1 Swedish notice, got 0"
        );
        assert!(resp.records.len() <= 5, "exceeded requested limit");
        // First record must have a real publication number and the
        // typed surface alpha-2 country mapping must work.
        let first = &resp.records[0].content;
        assert!(!first.notice_id.as_str().is_empty());
        assert_eq!(first.country, "SE", "alpha-3 → alpha-2 conversion failed");
        assert_eq!(resp.records[0].vendor, "live_ted");
    }
}
