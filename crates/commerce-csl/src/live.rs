// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Live Commerce CSL transport.
//!
//! ## Source choice
//!
//! The canonical Consolidated Screening List feed at
//! `https://api.trade.gov/v3/consolidated_screening_list/search`
//! requires a free api_key from data.trade.gov. To keep the default
//! `live` provider runnable without credentials, this module fetches
//! the **OpenSanctions mirror of the CSL** at
//! `https://data.opensanctions.org/datasets/latest/us_trade_csl/targets.simple.csv`
//! (CC-BY 4.0). Apps with a trade.gov key override
//! `LiveFetchOptions::sanctions_url` to point at the canonical
//! endpoint.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use manifold::{HttpFetchProvider, WebFetchBackend, WebFetchRequest};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::error::CommerceCslError;
use crate::provider::{CommerceCslProvider, CommerceCslRequest, CommerceCslResponse};
use crate::types::{MatchType, SanctionsHit, SubjectType};
use crate::{Observation, content_hash};

pub const DEFAULT_SANCTIONS_URL: &str =
    "https://data.opensanctions.org/datasets/latest/us_trade_csl/targets.simple.csv";

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
struct CslEntry {
    name: String,
    aliases: Vec<String>,
    schema: SubjectType,
    countries: Vec<String>,
    program: Option<String>,
    first_seen: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiveCommerceCslProvider {
    options: LiveFetchOptions,
    cache: Arc<RwLock<Option<Vec<CslEntry>>>>,
}

impl Default for LiveCommerceCslProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveCommerceCslProvider {
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

    async fn ensure_cache(&self) -> Result<(), CommerceCslError> {
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
impl CommerceCslProvider for LiveCommerceCslProvider {
    fn name(&self) -> &'static str {
        "live_commerce_csl"
    }

    async fn screen(
        &self,
        request: &CommerceCslRequest,
        _ctx: &embassy_pack::CallContext,
    ) -> Result<CommerceCslResponse, CommerceCslError> {
        let started = Instant::now();
        self.ensure_cache().await?;
        let cache = self.cache.read().await;
        let entries = cache
            .as_ref()
            .expect("ensure_cache populates the cache or returns Err");

        let CommerceCslRequest::Screen { subject } = request;
        let query = subject.name.trim().to_string();
        if query.is_empty() {
            return Err(CommerceCslError::InvalidSubject("empty subject name".into()));
        }
        let query_lc = query.to_ascii_lowercase();

        let request_json = serde_json::to_string(request).map_err(|e| {
            CommerceCslError::InvalidRequest(format!("non-serializable request: {e}"))
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
                list_name: "US Commerce CSL".to_string(),
                list_program: entry.program.clone(),
                listed_at: entry.first_seen.clone(),
                aliases: entry.aliases.clone(),
                jurisdictions: if entry.countries.is_empty() {
                    vec!["US".to_string()]
                } else {
                    entry.countries.clone()
                },
            };
            records.push(Observation {
                observation_id: format!(
                    "obs:commerce-csl-live:{request_hash}:{}",
                    records.len()
                ),
                request_hash: request_hash.clone(),
                vendor: self.name().to_string(),
                model: "opensanctions-us-trade-csl-csv-v1".to_string(),
                latency_ms,
                cost_estimate: None,
                tokens: None,
                content: hit,
                raw_response: None,
            });
        }

        Ok(CommerceCslResponse { records })
    }
}

fn live_error(err: LiveError) -> CommerceCslError {
    CommerceCslError::Transport(err.to_string())
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

/// OpenSanctions simple-CSV columns:
///   id, schema, name, aliases, birth_date, countries, addresses,
///   identifiers, sanctions, phones, emails, dataset, last_seen,
///   first_seen
fn parse_csv(body: &str) -> Result<Vec<CslEntry>, LiveError> {
    let mut entries = Vec::new();
    let mut lines = body.lines().enumerate();
    let _ = lines.next(); // header
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
        let program = if fields[8].is_empty() {
            None
        } else {
            Some(fields[8].clone())
        };
        let first_seen = fields.get(13).filter(|s| !s.is_empty()).cloned();

        entries.push(CslEntry {
            name,
            aliases,
            schema,
            countries,
            program,
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
denied-corp-01,Organization,\"Denied Corp\",\"DC Ltd;Denied Co\",,\"cn\",,,\"BIS Denied Persons List\",,,us_trade_csl,2026-04-01,2021-03-15\n\
entity-list-01,Organization,\"Entity Listed Inc.\",\"\",,\"ru\",,,\"BIS Entity List\",,,us_trade_csl,2026-04-01,2022-08-30";

    #[test]
    fn fixture_parses_two_entries() {
        let entries = parse_csv(FIXTURE_CSV).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "Denied Corp");
        assert_eq!(entries[0].aliases.len(), 2);
        assert_eq!(entries[1].name, "Entity Listed Inc.");
    }

    #[tokio::test]
    async fn screen_alias_returns_alias_hit() {
        let provider = LiveCommerceCslProvider::new();
        *provider.cache.write().await = Some(parse_csv(FIXTURE_CSV).unwrap());
        let req = CommerceCslRequest::Screen {
            subject: SanctionsSubject::parse("DC Ltd").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
        assert_eq!(resp.records[0].content.match_type, MatchType::Alias);
        assert_eq!(resp.records[0].content.subject_name, "Denied Corp");
    }

    #[tokio::test]
    async fn screen_substring_returns_fuzzy_hit_with_program() {
        let provider = LiveCommerceCslProvider::new();
        *provider.cache.write().await = Some(parse_csv(FIXTURE_CSV).unwrap());
        let req = CommerceCslRequest::Screen {
            subject: SanctionsSubject::parse("Entity Listed").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
        assert_eq!(resp.records[0].content.match_type, MatchType::Fuzzy);
        assert_eq!(
            resp.records[0].content.list_program.as_deref(),
            Some("BIS Entity List")
        );
    }

    #[tokio::test]
    async fn screen_clean_returns_zero_records() {
        let provider = LiveCommerceCslProvider::new();
        *provider.cache.write().await = Some(parse_csv(FIXTURE_CSV).unwrap());
        let req = CommerceCslRequest::Screen {
            subject: SanctionsSubject::parse("Acme Corp").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert!(resp.records.is_empty());
    }

    /// Live network test — disabled by default.
    #[tokio::test]
    #[ignore = "live network call to OpenSanctions mirror; opt-in only"]
    async fn live_screen() {
        if std::env::var("COMMERCE_CSL_LIVE_TEST").is_err() {
            eprintln!("Set COMMERCE_CSL_LIVE_TEST=1 to run the network-bound live test.");
            return;
        }
        let provider = LiveCommerceCslProvider::new();
        let req = CommerceCslRequest::Screen {
            subject: SanctionsSubject::parse("Volvo AB").unwrap(),
        };
        let resp = provider.screen(&req, &CallContext::default()).await.unwrap();
        assert!(resp.records.is_empty());
    }
}
