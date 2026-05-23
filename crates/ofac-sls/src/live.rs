// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Live OFAC SDN transport — CSV download + in-memory name match.
//!
//! Downloads the canonical Specially Designated Nationals list from
//! the US Treasury at
//! `https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/SDN.CSV`
//! on first lookup and caches the parsed entries for the lifetime of
//! the provider. The data is public-domain US government information;
//! no auth or registration required.
//!
//! Gated behind the `live` feature; default-features builds remain
//! dependency-light and rely on [`StubOfacSlsProvider`].
//!
//! ## REAL-by-default boundary
//!
//! [`LiveOfacSlsProvider::new`] is infallible — no env-var or path is
//! required. The first lookup performs the network fetch; failures
//! surface as typed [`OfacSlsError::Transport`] errors, never silent
//! fallback to stub. The cache lives for the provider's lifetime;
//! production deployments that need fresh data should re-create the
//! provider on a schedule (OFAC publishes updates roughly daily).
//!
//! ## Matching policy
//!
//! - **Exact** (`match_score = 0.99`): case-insensitive equality of
//!   the canonical name after whitespace trimming.
//! - **Fuzzy** (`match_score = 0.80`): case-insensitive substring
//!   match in either direction (query in SDN name, or SDN name in
//!   query). Phonetic / Levenshtein matching is a future addition;
//!   substring catches the common "Acme Corp" vs "ACME CORPORATION"
//!   shape today.
//! - **Alias**: not yet exercised by this provider; would require
//!   pulling ALT.CSV and joining on `ENT_NUM`. Returned as `Alias`
//!   when implemented; today only Exact/Fuzzy are emitted.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use manifold::{HttpFetchProvider, WebFetchBackend, WebFetchRequest};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::error::OfacSlsError;
use crate::provider::{OfacSlsProvider, OfacSlsRequest, OfacSlsResponse};
use crate::types::{MatchType, SanctionsHit, SubjectType};
use crate::{Observation, content_hash};

/// US Treasury's canonical SDN CSV endpoint. Apps that need to point
/// at a mirror or cached copy override via `LiveFetchOptions::sdn_url`.
pub const DEFAULT_SDN_URL: &str =
    "https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/SDN.CSV";

/// OFAC fair-use guidance asks consumers to identify themselves in
/// their User-Agent.
pub const DEFAULT_USER_AGENT: &str = "Reflective Labs Research kpernyer@gmail.com";

/// SDN.CSV is ~7 MiB today (~10 k entries). Manifold's
/// `WebFetchRequest` enforces an 8 MiB ceiling; sit right under it.
/// When the SDN crosses 8 MiB OFAC publishes a paginated alternative
/// and this provider must switch to it.
pub const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Generous timeout — the fetch happens at most once per provider
/// lifetime but the file is large enough that a slow link could
/// stretch the call past the default reqwest timeout.
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;

/// Politeness floor between OFAC fetches. We only do one fetch per
/// provider lifetime today, but keep the delay so future
/// implementations that refresh the cache stay polite.
pub const POLITENESS_DELAY: Duration = Duration::from_millis(150);

/// Exact-match confidence — case-insensitive whole-string equality.
pub const SCORE_EXACT: f32 = 0.99;

/// Fuzzy-match confidence — substring containment in either direction.
pub const SCORE_FUZZY: f32 = 0.80;

#[derive(Debug, Clone)]
pub struct LiveFetchOptions {
    pub sdn_url: String,
    pub user_agent: String,
    pub max_bytes: usize,
    pub timeout_ms: u64,
}

impl Default for LiveFetchOptions {
    fn default() -> Self {
        Self {
            sdn_url: DEFAULT_SDN_URL.to_string(),
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
    #[error("CSV parse failed at line {line}: {message}")]
    Parse { line: usize, message: String },
    #[error("blocking task join failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Parsed SDN entry. Fields lifted from
/// `OFAC SDN.CSV` columns: ENT_NUM, SDN_Name, SDN_Type, Program,
/// Title, Call_Sign, Vess_type, Tonnage, GRT, Vess_flag, Vess_owner,
/// Remarks. We keep only the columns the typed [`SanctionsHit`]
/// surface consumes.
#[derive(Debug, Clone)]
struct SdnEntry {
    name: String,
    sdn_type: SubjectType,
    programs: Vec<String>,
}

/// Live OFAC SDN provider. Downloads the SDN CSV on first request
/// and caches the parsed entries for the lifetime of the provider.
#[derive(Debug, Clone)]
pub struct LiveOfacSlsProvider {
    options: LiveFetchOptions,
    cache: Arc<RwLock<Option<Vec<SdnEntry>>>>,
}

impl Default for LiveOfacSlsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveOfacSlsProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            options: LiveFetchOptions::default(),
            cache: Arc::new(RwLock::new(None)),
        }
    }

    #[must_use]
    pub fn with_options(options: LiveFetchOptions) -> Self {
        Self {
            options,
            cache: Arc::new(RwLock::new(None)),
        }
    }

    async fn ensure_cache(&self) -> Result<(), OfacSlsError> {
        {
            let read = self.cache.read().await;
            if read.is_some() {
                return Ok(());
            }
        }
        let mut write = self.cache.write().await;
        if write.is_some() {
            return Ok(());
        }
        let body = fetch_sdn(&self.options).await.map_err(live_error)?;
        let entries = parse_sdn_csv(&body).map_err(live_error)?;
        *write = Some(entries);
        Ok(())
    }
}

#[async_trait]
impl OfacSlsProvider for LiveOfacSlsProvider {
    fn name(&self) -> &'static str {
        "live_ofac_sls"
    }

    async fn screen(
        &self,
        request: &OfacSlsRequest,
        _ctx: &embassy_pack::CallContext,
    ) -> Result<OfacSlsResponse, OfacSlsError> {
        let started = Instant::now();
        self.ensure_cache().await?;
        let cache = self.cache.read().await;
        let entries = cache
            .as_ref()
            .expect("ensure_cache populates the cache or returns Err");

        let OfacSlsRequest::Screen { subject } = request;
        let query = subject.name.trim().to_string();
        if query.is_empty() {
            return Err(OfacSlsError::InvalidSubject("empty subject name".into()));
        }
        let query_lc = query.to_ascii_lowercase();

        let request_json = serde_json::to_string(request).map_err(|e| {
            OfacSlsError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&request_json);
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        let mut records: Vec<Observation<SanctionsHit>> = Vec::new();
        for entry in entries.iter() {
            let name_lc = entry.name.to_ascii_lowercase();
            let (match_type, match_score) = if name_lc == query_lc {
                (MatchType::Exact, SCORE_EXACT)
            } else if name_lc.contains(&query_lc) || query_lc.contains(&name_lc) {
                (MatchType::Fuzzy, SCORE_FUZZY)
            } else {
                continue;
            };

            let hit = SanctionsHit {
                subject_name: entry.name.clone(),
                match_score,
                match_type,
                subject_type: entry.sdn_type,
                list_name: "OFAC SDN".to_string(),
                list_program: entry.programs.first().cloned(),
                listed_at: None,
                aliases: Vec::new(),
                jurisdictions: vec!["US".to_string()],
            };
            records.push(Observation {
                observation_id: format!(
                    "obs:ofac-sls-live:{request_hash}:{}",
                    records.len()
                ),
                request_hash: request_hash.clone(),
                vendor: self.name().to_string(),
                model: "ofac-sdn-csv-v1".to_string(),
                latency_ms,
                cost_estimate: None,
                tokens: None,
                content: hit,
                raw_response: None,
            });
        }

        Ok(OfacSlsResponse { records })
    }
}

fn live_error(err: LiveError) -> OfacSlsError {
    OfacSlsError::Transport(err.to_string())
}

async fn fetch_sdn(opts: &LiveFetchOptions) -> Result<String, LiveError> {
    let opts = opts.clone();
    tokio::task::spawn_blocking(move || {
        let provider = HttpFetchProvider::new()
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_user_agent(&opts.user_agent);
        let request = WebFetchRequest::new(&opts.sdn_url)
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_max_bytes(opts.max_bytes)
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_timeout_ms(opts.timeout_ms)
            .map_err(|e| LiveError::Fetch(e.to_string()))?;
        let response = provider
            .fetch(&request)
            .map_err(|e| LiveError::Fetch(e.to_string()))?;
        if response.status >= 400 {
            return Err(LiveError::Http {
                status: response.status,
                url: response.url,
            });
        }
        std::thread::sleep(POLITENESS_DELAY);
        Ok(response.body)
    })
    .await?
}

/// Parse the SDN.CSV body into entries. OFAC's format:
///
/// ```text
/// "1","NORTHERN PROPERTIES, S.A.","-0-","SDNTK","-0-",...
/// ```
///
/// Columns 0-11: ENT_NUM, SDN_Name, SDN_Type, Program, Title,
/// Call_Sign, Vess_type, Tonnage, GRT, Vess_flag, Vess_owner, Remarks.
/// The `-0-` token is OFAC's null marker. No header row. Some names
/// contain commas inside the quoted field, so the parser must respect
/// CSV quoting rather than naively split on `,`.
fn parse_sdn_csv(body: &str) -> Result<Vec<SdnEntry>, LiveError> {
    let mut entries = Vec::new();
    for (line_idx, line) in body.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields = parse_csv_row(line).map_err(|m| LiveError::Parse {
            line: line_idx + 1,
            message: m,
        })?;
        if fields.len() < 4 {
            // Not all rows have all columns populated, but the first
            // four (ENT_NUM through Program) should always be present.
            continue;
        }
        let name = fields[1].clone();
        if name.is_empty() || name == "-0-" {
            continue;
        }
        let sdn_type = match fields[2].as_str() {
            "individual" => SubjectType::Individual,
            "vessel" => SubjectType::Vessel,
            "aircraft" => SubjectType::Aircraft,
            _ => SubjectType::Entity,
        };
        let programs = if fields[3] == "-0-" {
            Vec::new()
        } else {
            fields[3].split(';').map(|s| s.trim().to_string()).collect()
        };
        entries.push(SdnEntry {
            name,
            sdn_type,
            programs,
        });
    }
    Ok(entries)
}

/// Minimal CSV-row parser that respects double-quoted fields with
/// embedded commas. Avoids an extra crate dep for what's a one-shape
/// input. Doesn't handle escaped quotes inside fields (OFAC's SDN.CSV
/// doesn't use them).
fn parse_csv_row(line: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut buf = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, in_quotes) {
            ('"', _) => in_quotes = !in_quotes,
            (',', false) => {
                fields.push(buf.trim().to_string());
                buf.clear();
            }
            (other, _) => buf.push(other),
        }
    }
    if in_quotes {
        return Err("unbalanced quote".to_string());
    }
    fields.push(buf.trim().to_string());
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use embassy_pack::{CallContext, SanctionsSubject};

    /// Canned excerpt of the OFAC SDN.CSV format. Three entries:
    /// an entity with a comma in the name (tests quoted-field parsing),
    /// a vessel, and an individual.
    const FIXTURE_CSV: &str = "\"1\",\"NORTHERN PROPERTIES, S.A.\",\"-0-\",\"SDNTK\",\"-0-\",\"-0-\",\"-0-\",\"-0-\",\"-0-\",\"-0-\",\"-0-\",\"-0-\"\n\
\"2\",\"MV DARK CARGO\",\"vessel\",\"NPWMD\",\"-0-\",\"-0-\",\"general cargo\",\"7000\",\"3500\",\"PA\",\"-0-\",\"-0-\"\n\
\"3\",\"SMITH, John\",\"individual\",\"SDGT\",\"-0-\",\"-0-\",\"-0-\",\"-0-\",\"-0-\",\"-0-\",\"-0-\",\"-0-\"";

    #[test]
    fn csv_row_respects_quoted_commas() {
        // Intent: OFAC names like "NORTHERN PROPERTIES, S.A." contain
        // commas inside quoted fields. A naive split would corrupt
        // them — and a corrupted name silently misses real hits.
        let row = parse_csv_row(
            "\"1\",\"NORTHERN PROPERTIES, S.A.\",\"-0-\",\"SDNTK\"",
        )
        .unwrap();
        assert_eq!(row[0], "1");
        assert_eq!(row[1], "NORTHERN PROPERTIES, S.A.");
        assert_eq!(row[2], "-0-");
        assert_eq!(row[3], "SDNTK");
    }

    #[test]
    fn fixture_parses_three_entries_with_correct_subject_types() {
        // Intent: the SDN_Type column drives downstream review. A
        // vessel listing is operationally different from an entity
        // or individual; mis-typing collapses meaningful distinctions.
        let entries = parse_sdn_csv(FIXTURE_CSV).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "NORTHERN PROPERTIES, S.A.");
        assert_eq!(entries[0].sdn_type, SubjectType::Entity);
        assert_eq!(entries[1].sdn_type, SubjectType::Vessel);
        assert_eq!(entries[2].sdn_type, SubjectType::Individual);
    }

    #[tokio::test]
    async fn screen_exact_match_returns_hit_with_score_one() {
        // Intent: a screened subject whose name equals an SDN entry
        // verbatim (case-insensitive) must return an Exact hit. If
        // the matcher silently downgrades to Fuzzy, downstream review
        // routing breaks.
        let provider = LiveOfacSlsProvider::new();
        let entries = parse_sdn_csv(FIXTURE_CSV).unwrap();
        *provider.cache.write().await = Some(entries);
        let req = OfacSlsRequest::Screen {
            subject: SanctionsSubject::parse("northern properties, s.a.").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
        assert_eq!(resp.records[0].content.match_type, MatchType::Exact);
        assert!((resp.records[0].content.match_score - SCORE_EXACT).abs() < 1e-6);
    }

    #[tokio::test]
    async fn screen_substring_returns_fuzzy_hit() {
        // Intent: a query that's a substring of the listed name
        // (e.g., "Smith" against "Smith, John") should match Fuzzy.
        // Missing this means individual-name screens silently fail.
        let provider = LiveOfacSlsProvider::new();
        let entries = parse_sdn_csv(FIXTURE_CSV).unwrap();
        *provider.cache.write().await = Some(entries);
        let req = OfacSlsRequest::Screen {
            subject: SanctionsSubject::parse("Smith").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
        assert_eq!(resp.records[0].content.match_type, MatchType::Fuzzy);
    }

    #[tokio::test]
    async fn screen_clean_returns_zero_records() {
        // Intent: a clean screen must produce zero records (not one
        // with `match_score = 0`). The downstream "any hits?" check
        // is `records.is_empty()`; if the matcher returns zero-score
        // hits, the contract silently inverts.
        let provider = LiveOfacSlsProvider::new();
        let entries = parse_sdn_csv(FIXTURE_CSV).unwrap();
        *provider.cache.write().await = Some(entries);
        let req = OfacSlsRequest::Screen {
            subject: SanctionsSubject::parse("Volvo AB").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert!(resp.records.is_empty());
    }

    /// Live network test — disabled by default. Run with:
    ///     OFAC_LIVE_TEST=1 cargo test -p converge-embassy-ofac-sls \
    ///         --features live -- --ignored live_screen
    #[tokio::test]
    #[ignore = "live download of OFAC SDN.CSV; opt-in only"]
    async fn live_screen() {
        if std::env::var("OFAC_LIVE_TEST").is_err() {
            eprintln!("Set OFAC_LIVE_TEST=1 to run the network-bound live test.");
            return;
        }
        let provider = LiveOfacSlsProvider::new();
        let req = OfacSlsRequest::Screen {
            subject: SanctionsSubject::parse("Apple Inc.").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        // Apple is not on the SDN; expect zero records.
        assert!(resp.records.is_empty());
    }
}
