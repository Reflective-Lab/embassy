// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Live EU Consolidated Financial Sanctions transport.
//!
//! ## Source choice
//!
//! The EU's canonical FSF feed at `https://webgate.ec.europa.eu/fsd/fsf`
//! requires an EU-Login account (free but registration-gated). To keep
//! the default `live` provider runnable without credentials, this
//! module fetches from **OpenSanctions' mirror of the EU FSF** at
//! `https://data.opensanctions.org/datasets/latest/eu_fsf/targets.simple.csv`
//! — re-published under CC-BY 4.0 from the canonical source.
//!
//! For compliance use that requires the authoritative source, override
//! `LiveFetchOptions::sanctions_url` (and `user_agent` / auth headers)
//! to point at the EU's own endpoint after registering for an
//! EU-Login. The on-the-wire CSV shape from OpenSanctions and the EU
//! FSF differ; the parser here is tuned to OpenSanctions' simple-CSV
//! columns (`id, schema, name, aliases, ...`). A custom URL pointing
//! at the EU's native XML would need a corresponding parser.
//!
//! ## REAL-by-default boundary
//!
//! Same shape as the OFAC live provider: lazy load on first
//! `screen()`, cache for the provider lifetime, exact +
//! substring-fuzzy name matching. Failures surface as typed
//! `EuSanctionsError::Transport` — never silent fallback to stub.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use manifold::{HttpFetchProvider, WebFetchBackend, WebFetchRequest};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::error::EuSanctionsError;
use crate::provider::{EuSanctionsProvider, EuSanctionsRequest, EuSanctionsResponse};
use crate::types::{MatchType, SanctionsHit, SubjectType};
use crate::{Observation, content_hash};

pub const DEFAULT_SANCTIONS_URL: &str =
    "https://data.opensanctions.org/datasets/latest/eu_fsf/targets.simple.csv";

pub const DEFAULT_USER_AGENT: &str = "Reflective Labs Research kpernyer@gmail.com";
pub const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;
pub const POLITENESS_DELAY: Duration = Duration::from_millis(150);

pub const SCORE_EXACT: f32 = 0.99;
pub const SCORE_FUZZY: f32 = 0.80;

#[derive(Debug, Clone)]
pub struct LiveFetchOptions {
    pub sanctions_url: String,
    pub user_agent: String,
    pub max_bytes: usize,
    pub timeout_ms: u64,
}

impl Default for LiveFetchOptions {
    fn default() -> Self {
        Self {
            sanctions_url: DEFAULT_SANCTIONS_URL.to_string(),
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

#[derive(Debug, Clone)]
struct EuEntry {
    name: String,
    aliases: Vec<String>,
    schema: SubjectType,
    countries: Vec<String>,
    sanctions: Option<String>,
    first_seen: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveEuSanctionsProvider {
    options: LiveFetchOptions,
    cache: Arc<RwLock<Option<Vec<EuEntry>>>>,
}

impl Default for LiveEuSanctionsProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveEuSanctionsProvider {
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

    async fn ensure_cache(&self) -> Result<(), EuSanctionsError> {
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
        let body = fetch(&self.options).await.map_err(live_error)?;
        let entries = parse_csv(&body).map_err(live_error)?;
        *write = Some(entries);
        Ok(())
    }
}

#[async_trait]
impl EuSanctionsProvider for LiveEuSanctionsProvider {
    fn name(&self) -> &'static str {
        "live_eu_sanctions"
    }

    async fn screen(
        &self,
        request: &EuSanctionsRequest,
        _ctx: &embassy_pack::CallContext,
    ) -> Result<EuSanctionsResponse, EuSanctionsError> {
        let started = Instant::now();
        self.ensure_cache().await?;
        let cache = self.cache.read().await;
        let entries = cache
            .as_ref()
            .expect("ensure_cache populates the cache or returns Err");

        let EuSanctionsRequest::Screen { subject } = request;
        let query = subject.name.trim().to_string();
        if query.is_empty() {
            return Err(EuSanctionsError::InvalidSubject("empty subject name".into()));
        }
        let query_lc = query.to_ascii_lowercase();

        let request_json = serde_json::to_string(request).map_err(|e| {
            EuSanctionsError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&request_json);
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

        let mut records: Vec<Observation<SanctionsHit>> = Vec::new();
        for entry in entries.iter() {
            let name_lc = entry.name.to_ascii_lowercase();
            let alias_match = entry
                .aliases
                .iter()
                .any(|a| a.to_ascii_lowercase() == query_lc);
            let (match_type, match_score) = if name_lc == query_lc {
                (MatchType::Exact, SCORE_EXACT)
            } else if alias_match {
                (MatchType::Alias, SCORE_EXACT)
            } else if name_lc.contains(&query_lc) || query_lc.contains(&name_lc) {
                (MatchType::Fuzzy, SCORE_FUZZY)
            } else {
                continue;
            };

            let hit = SanctionsHit {
                subject_name: entry.name.clone(),
                match_score,
                match_type,
                subject_type: entry.schema,
                list_name: "EU Consolidated FSF".to_string(),
                list_program: entry.sanctions.clone(),
                listed_at: entry.first_seen.clone(),
                aliases: entry.aliases.clone(),
                jurisdictions: if entry.countries.is_empty() {
                    vec!["EU".to_string()]
                } else {
                    entry.countries.clone()
                },
            };
            records.push(Observation {
                observation_id: format!(
                    "obs:eu-sanctions-live:{request_hash}:{}",
                    records.len()
                ),
                request_hash: request_hash.clone(),
                vendor: self.name().to_string(),
                model: "opensanctions-eu-fsf-csv-v1".to_string(),
                latency_ms,
                cost_estimate: None,
                tokens: None,
                content: hit,
                raw_response: None,
            });
        }

        Ok(EuSanctionsResponse { records })
    }
}

fn live_error(err: LiveError) -> EuSanctionsError {
    EuSanctionsError::Transport(err.to_string())
}

async fn fetch(opts: &LiveFetchOptions) -> Result<String, LiveError> {
    let opts = opts.clone();
    tokio::task::spawn_blocking(move || {
        let provider = HttpFetchProvider::new()
            .map_err(|e| LiveError::Fetch(e.to_string()))?
            .with_user_agent(&opts.user_agent);
        let request = WebFetchRequest::new(&opts.sanctions_url)
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

/// Parse OpenSanctions' "simple" CSV. Header row:
///   id, schema, name, aliases, birth_date, countries, addresses,
///   identifiers, sanctions, phones, emails, dataset, last_seen,
///   first_seen
/// We need: name (col 2), aliases (col 3; `;`-separated), schema
/// (col 1; map to SubjectType), countries (col 5; `;`-separated),
/// sanctions (col 8), first_seen (col 13).
fn parse_csv(body: &str) -> Result<Vec<EuEntry>, LiveError> {
    let mut entries = Vec::new();
    let mut lines = body.lines().enumerate();
    // Skip header.
    let _ = lines.next();
    for (line_idx, line) in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields = parse_csv_row(line).map_err(|m| LiveError::Parse {
            line: line_idx + 1,
            message: m,
        })?;
        if fields.len() < 9 {
            continue;
        }
        let schema = match fields[1].as_str() {
            "Person" => SubjectType::Individual,
            "Vessel" => SubjectType::Vessel,
            "Airplane" | "Aircraft" => SubjectType::Aircraft,
            _ => SubjectType::Entity,
        };
        let name = fields[2].clone();
        if name.is_empty() {
            continue;
        }
        let aliases: Vec<String> = fields[3]
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect();
        let countries: Vec<String> = fields[5]
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .collect();
        let sanctions = if fields[8].is_empty() {
            None
        } else {
            Some(fields[8].clone())
        };
        let first_seen = fields.get(13).filter(|s| !s.is_empty()).cloned();

        entries.push(EuEntry {
            name,
            aliases,
            schema,
            countries,
            sanctions,
            first_seen,
        });
    }
    Ok(entries)
}

fn parse_csv_row(line: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut buf = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, in_quotes) {
            ('"', _) => {
                if in_quotes && chars.peek() == Some(&'"') {
                    // Escaped doubled-quote inside quoted field.
                    buf.push('"');
                    chars.next();
                } else {
                    in_quotes = !in_quotes;
                }
            }
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

    const FIXTURE_CSV: &str = "id,schema,name,aliases,birth_date,countries,addresses,identifiers,sanctions,phones,emails,dataset,last_seen,first_seen\n\
acme-ru-001,Organization,\"Acme Sanctioned LLC\",\"ACME RU;Acme Russia\",,\"ru\",,,\"Council Regulation (EU) 269/2014\",,,eu_fsf,2026-04-01,2024-03-01\n\
ivan-001,Person,\"Ivanov, Ivan\",\"Ivan I. Ivanov\",1970-01-01,\"ru\",,,Council Regulation (EU) 269/2014,,,eu_fsf,2026-04-01,2014-07-22\n\
vessel-001,Vessel,\"DARK TANKER\",\"\",,,,,\"Council Regulation (EU) 833/2014\",,,eu_fsf,2026-04-01,2023-12-05";

    #[test]
    fn fixture_parses_three_entries() {
        let entries = parse_csv(FIXTURE_CSV).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "Acme Sanctioned LLC");
        assert_eq!(entries[0].schema, SubjectType::Entity);
        assert_eq!(entries[0].aliases.len(), 2);
        assert_eq!(entries[1].schema, SubjectType::Individual);
        assert_eq!(entries[2].schema, SubjectType::Vessel);
    }

    #[tokio::test]
    async fn screen_alias_match_returns_alias_hit() {
        // Intent: a hit on the alias column must be tagged MatchType::Alias
        // (not Exact / Fuzzy). Downstream review routes aliases differently
        // from primary-name hits.
        let provider = LiveEuSanctionsProvider::new();
        *provider.cache.write().await = Some(parse_csv(FIXTURE_CSV).unwrap());
        let req = EuSanctionsRequest::Screen {
            subject: SanctionsSubject::parse("ACME RU").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
        assert_eq!(resp.records[0].content.match_type, MatchType::Alias);
        assert_eq!(resp.records[0].content.subject_name, "Acme Sanctioned LLC");
    }

    #[tokio::test]
    async fn screen_exact_match_returns_exact_hit() {
        let provider = LiveEuSanctionsProvider::new();
        *provider.cache.write().await = Some(parse_csv(FIXTURE_CSV).unwrap());
        let req = EuSanctionsRequest::Screen {
            subject: SanctionsSubject::parse("dark tanker").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
        assert_eq!(resp.records[0].content.match_type, MatchType::Exact);
        assert_eq!(resp.records[0].content.subject_type, SubjectType::Vessel);
    }

    #[tokio::test]
    async fn screen_clean_returns_zero_records() {
        let provider = LiveEuSanctionsProvider::new();
        *provider.cache.write().await = Some(parse_csv(FIXTURE_CSV).unwrap());
        let req = EuSanctionsRequest::Screen {
            subject: SanctionsSubject::parse("Volvo AB").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert!(resp.records.is_empty());
    }

    /// Live network test — disabled by default. Run with:
    ///     EU_SANCTIONS_LIVE_TEST=1 cargo test \
    ///         -p converge-embassy-eu-sanctions --features live -- --ignored live_screen
    #[tokio::test]
    #[ignore = "live network call to OpenSanctions mirror; opt-in only"]
    async fn live_screen() {
        if std::env::var("EU_SANCTIONS_LIVE_TEST").is_err() {
            eprintln!("Set EU_SANCTIONS_LIVE_TEST=1 to run the network-bound live test.");
            return;
        }
        let provider = LiveEuSanctionsProvider::new();
        let req = EuSanctionsRequest::Screen {
            subject: SanctionsSubject::parse("Volvo AB").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        // Volvo is not on the EU FSF; expect zero records.
        assert!(resp.records.is_empty());
    }
}
