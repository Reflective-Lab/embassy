// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Live SEC EDGAR transport — HTTP fetch + Item-section extraction.
//!
//! Lifted out of `fathom-sparc-ingest::sec` so any app that needs SEC
//! filings shares one implementation of the SEC contract (User-Agent,
//! rate-limit politeness, multi-selector heading heuristic). The SPARC
//! synthesis layer (`RiskFactorSection` with drift/language analysis)
//! stays in the app; only the SEC-contract bits live here.
//!
//! Gated behind the `live` feature so the default sec-edgar surface
//! (typed domain + stub provider) remains dependency-light. CI runs
//! against the stub; this module is exercised by integration tests
//! that use canned HTML fixtures — no network in CI.
//!
//! ## What SEC contract owns vs what the app owns
//!
//! - **SEC contract (here)**: User-Agent requirement, 10 req/sec
//!   rate-limit politeness, the three observed 10-K markup-selector
//!   patterns, the Item-N section locator heuristic, multi-selector
//!   fallback chain with min/max heading-count plausibility bounds.
//! - **App (synthesis layer)**: turning the extracted headings into
//!   higher-level shapes like SPARC's `RiskFactorSection` with drift
//!   signals, language-feature signals, sentence-vs-heading
//!   granularity reconciliation against any other source (e.g., the
//!   HuggingFace bulk dataset).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use manifold::{
    ExtractedNode, HtmlExtractBackend, HttpFetchProvider, ScraperHtmlBackend, WebFetchBackend,
    WebFetchRequest,
};
use serde::Deserialize;
use thiserror::Error;

use crate::error::SecEdgarError;
use crate::provider::{SecEdgarProvider, SecEdgarRequest, SecEdgarResponse};
use crate::types::{AccessionNumber, Cik, Filing, FilingSection, FormType};
use crate::{Observation, content_hash};

/// SEC's published fair-use policy requires submissions identify the
/// requester. Pin a recognizable Reflective Labs research contact;
/// apps with their own SEC-registered UA can override via the
/// `LiveFetchOptions::user_agent` field.
pub const DEFAULT_USER_AGENT: &str = "Reflective Labs Research kpernyer@gmail.com";

/// 10-Ks are routinely 1.5–8 MB of HTML; manifold's `WebFetchByteLimit`
/// caps at 8 MiB which is the SEC contract size headroom.
pub const DEFAULT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Per-request timeout for the HTTP fetch. SEC's primary docs are
/// large; 60 s is generous but bounded.
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// SEC fair-use limit is 10 req/sec workspace-wide. Sleep 120 ms after
/// each fetch as a politeness floor; apps that operate at higher
/// concurrency should use a centralized rate-limiter in addition.
pub const POLITENESS_DELAY: Duration = Duration::from_millis(120);

/// Observed 10-K heading markup patterns across Apple / Microsoft /
/// NVIDIA filings. Order matters — the first selector to yield a
/// plausible heading count wins, on the principle that narrower
/// matches are less likely to include false-positive body paragraphs.
pub const DEFAULT_SELECTORS: &[&str] = &[
    // Apple — italic + bold (font-weight:700) headings.
    r#"span[style*="font-style:italic"][style*="font-weight:700"]"#,
    // MSFT — plain bold (font-weight:bold).
    r#"span[style*="font-weight:bold"]"#,
    // NVDA — weight-700 without italic.
    r#"span[style*="font-weight:700"]"#,
];

/// Lower bound on plausible heading count for a 10-K Item 1A section.
/// Below this and the selector is almost certainly missing real
/// headings. Calibrated against the observed S&P 100 distribution.
pub const DEFAULT_MIN_HEADINGS: usize = 15;
/// Upper bound — above this the selector is probably matching body
/// paragraphs, not headings.
pub const DEFAULT_MAX_HEADINGS: usize = 40;
pub const DEFAULT_MIN_HEADING_LEN: usize = 30;
pub const DEFAULT_MAX_HEADING_LEN: usize = 300;

#[derive(Debug, Error)]
pub enum LiveError {
    #[error("fetch failed: {0}")]
    Fetch(String),
    #[error("extract failed: {0}")]
    Extract(String),
    #[error("item {item_id} section not found in document")]
    SectionNotFound { item_id: String },
    #[error(
        "no extractor pattern produced a plausible heading count (\
        {min}–{max} inclusive); best selector yielded {best} headings"
    )]
    NoPlausiblePattern { best: usize, min: usize, max: usize },
    #[error("HTTP {status} from {url}")]
    Http { status: u16, url: String },
    #[error("blocking task join failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Configuration knobs for a live fetch. Defaults follow the SEC
/// contract; apps override per-request when they have specific
/// requirements (their own UA, a higher size cap, etc.).
#[derive(Debug, Clone)]
pub struct LiveFetchOptions {
    pub user_agent: String,
    pub max_bytes: usize,
    pub timeout_ms: u64,
}

impl Default for LiveFetchOptions {
    fn default() -> Self {
        Self {
            user_agent: DEFAULT_USER_AGENT.to_string(),
            max_bytes: DEFAULT_MAX_BYTES,
            timeout_ms: DEFAULT_TIMEOUT_MS,
        }
    }
}

/// Live SEC EDGAR provider for the source-shaped provider trait.
///
/// This provider resolves filing metadata, fetches the primary filing
/// document from SEC EDGAR, extracts Item 1A for 10-K filings, and
/// returns typed [`Observation<Filing>`] records through the same
/// [`SecEdgarProvider`] trait that tests use with
/// [`crate::StubSecEdgarProvider`].
#[derive(Debug, Clone, Default)]
pub struct LiveSecEdgarProvider {
    options: LiveFetchOptions,
}

impl LiveSecEdgarProvider {
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
impl SecEdgarProvider for LiveSecEdgarProvider {
    fn name(&self) -> &'static str {
        "live_sec_edgar"
    }

    async fn fetch(
        &self,
        request: &SecEdgarRequest,
        _ctx: &embassy_pack::CallContext,
    ) -> Result<SecEdgarResponse, SecEdgarError> {
        let started = Instant::now();
        let descriptor = match request {
            SecEdgarRequest::Filing {
                cik,
                accession_number,
            } => self
                .resolve_filing(cik, accession_number)
                .await
                .map_err(sec_error)?,
            SecEdgarRequest::LatestByForm { cik, form_type } => self
                .resolve_latest_by_form(cik, form_type)
                .await
                .map_err(sec_error)?,
        };

        let html = fetch_filing_html(&descriptor.primary_url, &self.options)
            .await
            .map_err(sec_error)?;

        let filing = build_filing(&descriptor, &html)?;
        let request_json = serde_json::to_string(request)
            .map_err(|e| SecEdgarError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&request_json);
        let latency_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let raw_response = serde_json::json!({
            "detail_url": descriptor.detail_url,
            "primary_url": descriptor.primary_url,
            "primary_document": descriptor.primary_document,
            "html_bytes": html.len(),
            "sections": filing.sections.keys().collect::<Vec<_>>(),
        })
        .to_string();

        let observation = Observation {
            observation_id: format!("obs:sec-edgar-live:{request_hash}"),
            request_hash,
            vendor: self.name().to_string(),
            model: "sec-edgar-live-v1".to_string(),
            latency_ms,
            cost_estimate: None,
            tokens: None,
            content: filing,
            raw_response: Some(raw_response),
        };

        Ok(SecEdgarResponse {
            records: vec![observation],
        })
    }
}

impl LiveSecEdgarProvider {
    async fn resolve_filing(
        &self,
        cik: &Cik,
        accession_number: &AccessionNumber,
    ) -> Result<FilingDescriptor, LiveError> {
        let detail_url = filing_detail_url(cik, accession_number);
        let index_html = fetch_text(&detail_url, &self.options).await?;
        descriptor_from_index(cik, accession_number, &detail_url, &index_html)
    }

    async fn resolve_latest_by_form(
        &self,
        cik: &Cik,
        form_type: &FormType,
    ) -> Result<FilingDescriptor, LiveError> {
        let submissions_url = submissions_url(cik);
        let json = fetch_text(&submissions_url, &self.options).await?;
        descriptor_from_submissions(cik, form_type, &json)
    }
}

/// Fetch an SEC primary-doc HTML from the given URL, observing the
/// SEC's User-Agent, byte-cap, and rate-limit politeness contract.
///
/// Returns the raw HTML body on success. Callers feed the body into
/// [`locate_item_section`] + [`extract_section_headings`].
///
/// `url` must be the direct URL to the primary HTML document (e.g.,
/// `https://www.sec.gov/Archives/edgar/data/320193/...aapl-...htm`).
/// Resolving CIK + accession → primary-doc URL is a separate concern
/// (EDGAR's submissions-index lookup) that this helper does not own.
pub async fn fetch_filing_html(url: &str, opts: &LiveFetchOptions) -> Result<String, LiveError> {
    fetch_text(url, opts).await
}

async fn fetch_text(url: &str, opts: &LiveFetchOptions) -> Result<String, LiveError> {
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

#[derive(Debug, Clone)]
struct FilingDescriptor {
    cik: Cik,
    accession_number: AccessionNumber,
    form_type: FormType,
    filed_at: String,
    primary_document: String,
    primary_url: String,
    detail_url: String,
}

fn build_filing(descriptor: &FilingDescriptor, html: &str) -> Result<Filing, SecEdgarError> {
    let mut sections = BTreeMap::new();
    if matches!(descriptor.form_type, FormType::Form10K) {
        let section = locate_item_section(html, "1A", "1B").ok_or_else(|| {
            SecEdgarError::Transport(format!(
                "Item 1A section not found in SEC filing {}",
                descriptor.primary_url
            ))
        })?;
        sections.insert(
            "1A".to_string(),
            FilingSection {
                id: "1A".to_string(),
                title: "Risk Factors".to_string(),
                body: section.to_string(),
            },
        );
    }

    Ok(Filing {
        cik: descriptor.cik.clone(),
        accession_number: descriptor.accession_number.clone(),
        form_type: descriptor.form_type.clone(),
        filed_at: descriptor.filed_at.clone(),
        sections,
    })
}

fn sec_error(err: LiveError) -> SecEdgarError {
    match err {
        LiveError::Http { status: 429, .. } => SecEdgarError::RateLimited,
        LiveError::Http { status, url } => {
            SecEdgarError::Transport(format!("HTTP {status} from {url}"))
        }
        other => SecEdgarError::Transport(other.to_string()),
    }
}

fn filing_detail_url(cik: &Cik, accession_number: &AccessionNumber) -> String {
    format!(
        "https://www.sec.gov/Archives/edgar/data/{}/{accession}-index.htm",
        cik_path_segment(cik),
        accession = accession_number.as_str()
    )
}

fn submissions_url(cik: &Cik) -> String {
    format!("https://data.sec.gov/submissions/CIK{}.json", cik.as_str())
}

fn primary_document_url(cik: &Cik, accession_number: &AccessionNumber, document: &str) -> String {
    format!(
        "https://www.sec.gov/Archives/edgar/data/{}/{}/{}",
        cik_path_segment(cik),
        accession_path_segment(accession_number),
        document
    )
}

fn cik_path_segment(cik: &Cik) -> &str {
    let trimmed = cik.as_str().trim_start_matches('0');
    if trimmed.is_empty() { "0" } else { trimmed }
}

fn accession_path_segment(accession_number: &AccessionNumber) -> String {
    accession_number.as_str().replace('-', "")
}

fn descriptor_from_index(
    cik: &Cik,
    accession_number: &AccessionNumber,
    detail_url: &str,
    index_html: &str,
) -> Result<FilingDescriptor, LiveError> {
    let form_label = parse_form_label(index_html).ok_or_else(|| {
        LiveError::Extract(format!(
            "could not parse form label from SEC filing index {detail_url}"
        ))
    })?;
    let filed_at = parse_filing_date(index_html).ok_or_else(|| {
        LiveError::Extract(format!(
            "could not parse filing date from SEC filing index {detail_url}"
        ))
    })?;
    let href = parse_primary_document_href(index_html).ok_or_else(|| {
        LiveError::Extract(format!(
            "could not parse primary document from SEC filing index {detail_url}"
        ))
    })?;
    let primary_url = normalize_sec_document_href(&href)?;
    let primary_document = primary_url
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string();

    Ok(FilingDescriptor {
        cik: cik.clone(),
        accession_number: accession_number.clone(),
        form_type: FormType::from_label(form_label),
        filed_at,
        primary_document,
        primary_url,
        detail_url: detail_url.to_string(),
    })
}

fn parse_form_label(index_html: &str) -> Option<&str> {
    let start = index_html.find("<strong>Form ")? + "<strong>Form ".len();
    let rest = &index_html[start..];
    let end = rest.find("</strong>")?;
    Some(rest[..end].trim())
}

fn parse_filing_date(index_html: &str) -> Option<String> {
    let marker = r#"<div class="infoHead">Filing Date</div>"#;
    let start = index_html.find(marker)? + marker.len();
    let rest = &index_html[start..];
    let info_start = rest.find(r#"<div class="info">"#)? + r#"<div class="info">"#.len();
    let info_rest = &rest[info_start..];
    let info_end = info_rest.find("</div>")?;
    Some(info_rest[..info_end].trim().to_string())
}

fn parse_primary_document_href(index_html: &str) -> Option<String> {
    let ix_marker = "/ix?doc=";
    if let Some(start) = index_html.find(ix_marker).map(|pos| pos + ix_marker.len()) {
        let rest = &index_html[start..];
        let end = rest.find('"')?;
        return Some(rest[..end].to_string());
    }

    let table_start = index_html.find("Document Format Files").unwrap_or(0);
    let rest = &index_html[table_start..];
    let archive_marker = r#"href="/Archives/"#;
    let start = rest.find(archive_marker)? + r#"href=""#.len();
    let href_rest = &rest[start..];
    let end = href_rest.find('"')?;
    Some(href_rest[..end].to_string())
}

fn normalize_sec_document_href(href: &str) -> Result<String, LiveError> {
    if href.starts_with("https://www.sec.gov/Archives/") {
        return Ok(href.to_string());
    }
    if let Some(inner) = href.strip_prefix("/ix?doc=") {
        return normalize_sec_document_href(inner);
    }
    if href.starts_with("/Archives/") {
        return Ok(format!("https://www.sec.gov{href}"));
    }
    Err(LiveError::Extract(format!(
        "unsupported SEC primary document href `{href}`"
    )))
}

#[derive(Debug, Deserialize)]
struct Submissions {
    filings: SubmissionFilings,
}

#[derive(Debug, Deserialize)]
struct SubmissionFilings {
    recent: RecentFilings,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecentFilings {
    accession_number: Vec<String>,
    filing_date: Vec<String>,
    form: Vec<String>,
    primary_document: Vec<String>,
}

fn descriptor_from_submissions(
    cik: &Cik,
    form_type: &FormType,
    json: &str,
) -> Result<FilingDescriptor, LiveError> {
    let submissions: Submissions =
        serde_json::from_str(json).map_err(|e| LiveError::Extract(e.to_string()))?;
    let target = form_type.as_label();
    let recent = submissions.filings.recent;
    let idx = recent
        .form
        .iter()
        .position(|form| form == target)
        .ok_or_else(|| {
            LiveError::Extract(format!(
                "no recent SEC filing found for CIK {} and form {target}",
                cik.as_str()
            ))
        })?;

    let accession = recent
        .accession_number
        .get(idx)
        .ok_or_else(|| LiveError::Extract("missing accessionNumber entry".to_string()))?;
    let accession_number = AccessionNumber::parse(accession)
        .map_err(|e| LiveError::Extract(format!("invalid SEC accession in submissions: {e}")))?;
    let filed_at = recent
        .filing_date
        .get(idx)
        .ok_or_else(|| LiveError::Extract("missing filingDate entry".to_string()))?
        .clone();
    let primary_document = recent
        .primary_document
        .get(idx)
        .ok_or_else(|| LiveError::Extract("missing primaryDocument entry".to_string()))?
        .clone();
    let primary_url = primary_document_url(cik, &accession_number, &primary_document);
    let detail_url = filing_detail_url(cik, &accession_number);

    Ok(FilingDescriptor {
        cik: cik.clone(),
        accession_number,
        form_type: form_type.clone(),
        filed_at,
        primary_document,
        primary_url,
        detail_url,
    })
}

/// Returns the slice of `html` covering the requested item section
/// (`"1A"`, `"7"`, etc.) or `None` if the section bounds are missing.
///
/// SEC 10-Ks typically reference each item three times: a TOC entry,
/// a forward-looking-statement reference, and the actual section
/// heading. This helper takes the **third** occurrence as the section
/// start and the **last** occurrence of the next item as the section
/// end. Two boundary markers tried per item: plain space (`Item 1A`)
/// and non-breaking space (`Item\u{a0}1A`) — both are observed in
/// filings.
///
/// `next_item_id` is the next sequential SEC item — caller supplies it
/// because the SEC vocabulary isn't strictly numerical (`1`, `1A`,
/// `1B`, `2`, …). For Item 1A the caller passes `"1B"`.
pub fn locate_item_section<'a>(
    html: &'a str,
    item_id: &str,
    next_item_id: &str,
) -> Option<&'a str> {
    let needle = format!("Item {item_id}");
    let nbsp_needle = format!("Item\u{a0}{item_id}");
    let next_needle = format!("Item {next_item_id}");
    let next_nbsp_needle = format!("Item\u{a0}{next_item_id}");

    let mut positions_start: Vec<usize> = find_all(html, &needle)
        .chain(find_all(html, &nbsp_needle))
        .collect();
    let positions_end: Vec<usize> = find_all(html, &next_needle)
        .chain(find_all(html, &next_nbsp_needle))
        .collect();
    if positions_start.len() < 3 || positions_end.is_empty() {
        return None;
    }
    positions_start.sort_unstable();
    let start = positions_start[2];
    let end = positions_end
        .iter()
        .copied()
        .filter(|&p| p > start)
        .max()
        .unwrap_or(html.len());
    Some(&html[start..end])
}

fn find_all<'a>(haystack: &'a str, needle: &'a str) -> impl Iterator<Item = usize> + 'a {
    let mut start = 0;
    std::iter::from_fn(move || {
        let pos = haystack[start..].find(needle)?;
        let abs = start + pos;
        start = abs + needle.len();
        Some(abs)
    })
}

/// Heading-extraction configuration. Defaults follow the calibrated
/// 10-K Item-1A bounds; apps override for items with different
/// plausibility ranges.
#[derive(Debug, Clone)]
pub struct HeadingExtractOptions {
    pub selectors: Vec<String>,
    pub min_headings: usize,
    pub max_headings: usize,
    pub min_heading_len: usize,
    pub max_heading_len: usize,
}

impl Default for HeadingExtractOptions {
    fn default() -> Self {
        Self {
            selectors: DEFAULT_SELECTORS.iter().map(|s| (*s).to_string()).collect(),
            min_headings: DEFAULT_MIN_HEADINGS,
            max_headings: DEFAULT_MAX_HEADINGS,
            min_heading_len: DEFAULT_MIN_HEADING_LEN,
            max_heading_len: DEFAULT_MAX_HEADING_LEN,
        }
    }
}

/// Walk the configured selector chain over the section HTML and return
/// the heading strings produced by the first selector to yield a
/// plausible count. Filters by per-heading length bounds and the
/// observed convention that 10-K risk-factor headings end with a
/// period.
pub fn extract_section_headings(
    section_html: &str,
    opts: &HeadingExtractOptions,
) -> Result<Vec<String>, LiveError> {
    let backend = ScraperHtmlBackend::new();
    let mut best: Vec<String> = Vec::new();
    let mut max_seen = 0usize;
    for selector in &opts.selectors {
        let nodes = backend
            .extract(section_html, &[selector.as_str()])
            .map_err(|e| LiveError::Extract(e.to_string()))?;
        let candidates: Vec<String> = nodes
            .into_iter()
            .map(|n: ExtractedNode| n.text)
            .filter(|text| {
                let len = text.len();
                len >= opts.min_heading_len && len <= opts.max_heading_len && text.ends_with('.')
            })
            .collect();
        let count = candidates.len();
        if (opts.min_headings..=opts.max_headings).contains(&count) {
            return Ok(candidates);
        }
        if count > max_seen {
            max_seen = count;
            best = candidates;
        }
    }
    if best.is_empty() || !(opts.min_headings..=opts.max_headings).contains(&max_seen) {
        return Err(LiveError::NoPlausiblePattern {
            best: max_seen,
            min: opts.min_headings,
            max: opts.max_headings,
        });
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    fn apple_cik() -> Cik {
        Cik::parse("0000320193").unwrap()
    }

    fn apple_2025_accession() -> AccessionNumber {
        AccessionNumber::parse("0000320193-25-000079").unwrap()
    }

    #[test]
    fn locate_item_1a_takes_third_marker_as_start_and_last_1b_as_end() {
        // Intent: 10-Ks typically mention each item three times (TOC,
        // FLS reference, real heading). If a future "simplification"
        // takes the first or second marker as the section start, the
        // body of every risk-factor section becomes the TOC blurb
        // instead — every downstream analysis (drift, language, length
        // distribution) becomes meaningless.
        let html = "TOC: Item 1A. Risk Factors p.5. \
            Forward looking: see Item 1A. \
            Item 1A. Risk Factors\n\
            ...real content...\n\
            Item 1B. Unresolved comments";
        let section = locate_item_section(html, "1A", "1B").expect("section present");
        assert!(section.contains("...real content..."));
        assert!(!section.contains("Item 1B"));
    }

    #[test]
    fn locate_item_section_returns_none_when_section_missing() {
        // Intent: if a filing only references Item 1A once (TOC-only,
        // amendment, etc.) the locator must fail loudly — silently
        // returning empty content would let drift analysis flag a
        // change that's really a parser miss.
        let html = "TOC: Item 1A in toc, but only one occurrence";
        assert!(locate_item_section(html, "1A", "1B").is_none());
    }

    #[test]
    fn locate_item_section_works_for_arbitrary_item_ids() {
        // Intent: the helper isn't 1A-specific. Same heuristic should
        // resolve Item 7 (MD&A) → Item 7A (quant disclosures), or any
        // item pair the caller supplies. Lifting this generically out
        // of SPARC means future ports won't reinvent the wheel for
        // each new section.
        let html = "TOC: Item 7. MD&A. \
            FLS: see Item 7. \
            Item 7. Management's Discussion\n\
            ...analysis content...\n\
            Item 7A. Quantitative disclosures";
        let section = locate_item_section(html, "7", "7A").expect("section present");
        assert!(section.contains("...analysis content..."));
    }

    #[test]
    fn extract_section_headings_picks_pattern_yielding_plausible_count() {
        // Intent: with 16 italic+bold headings present, the first
        // selector (Apple style) must claim the match. If a future
        // refactor changed the order of preference or shortened the
        // plausibility band, this test fails.
        let mut section = String::new();
        for i in 0..16 {
            write!(
                section,
                r#"<span style="font-style:italic;font-weight:700">Risk factor heading number {i:02} ends with a period.</span>"#
            )
            .unwrap();
        }
        let headings = extract_section_headings(&section, &HeadingExtractOptions::default())
            .expect("plausible count");
        assert_eq!(headings.len(), 16);
        assert!(headings[0].ends_with("period."));
    }

    #[test]
    fn extract_section_headings_rejects_below_plausibility_floor() {
        // Intent: with 5 headings (< MIN_HEADINGS = 15), the helper
        // must error — otherwise an under-extracted section would feed
        // bogus "risk factor count dropped 70%" signals to drift
        // analysis. Loud failure beats silent corruption.
        let mut section = String::new();
        for i in 0..5 {
            write!(
                section,
                r#"<span style="font-weight:bold">Risk factor heading number {i:02} ends with a period.</span>"#
            )
            .unwrap();
        }
        let err = extract_section_headings(&section, &HeadingExtractOptions::default())
            .expect_err("must fail");
        match err {
            LiveError::NoPlausiblePattern { best, min, max } => {
                assert_eq!(best, 5);
                assert_eq!(min, 15);
                assert_eq!(max, 40);
            }
            other => panic!("expected NoPlausiblePattern, got {other:?}"),
        }
    }

    #[test]
    fn extract_section_headings_filters_out_too_short_strings() {
        // Intent: candidate text shorter than MIN_HEADING_LEN (30) is
        // body fragments, not real headings. Filtering at the helper
        // boundary keeps the count + content honest for downstream.
        let mut section =
            r#"<span style="font-style:italic;font-weight:700">Short.</span>"#.to_string();
        for i in 0..16 {
            write!(
                section,
                r#"<span style="font-style:italic;font-weight:700">Plausibly-long risk factor heading {i:02} that ends with a period.</span>"#
            )
            .unwrap();
        }
        let headings = extract_section_headings(&section, &HeadingExtractOptions::default())
            .expect("16 plausible after filter");
        assert_eq!(
            headings.len(),
            16,
            "the too-short candidate must be filtered out"
        );
        assert!(headings.iter().all(|h| h.len() >= 30));
    }

    #[test]
    fn descriptor_from_index_parses_primary_ixbrl_document() {
        let html = r#"
            <strong>Form 10-K</strong>
            <div class="infoHead">Filing Date</div>
            <div class="info">2025-10-31</div>
            <table class="tableFile">
              <tr>
                <td scope="row">1</td>
                <td scope="row">10-K</td>
                <td scope="row"><a href="/ix?doc=/Archives/edgar/data/320193/000032019325000079/aapl-20250927.htm">aapl-20250927.htm</a></td>
                <td scope="row">10-K</td>
              </tr>
            </table>
        "#;

        let descriptor = descriptor_from_index(
            &apple_cik(),
            &apple_2025_accession(),
            "https://www.sec.gov/Archives/edgar/data/320193/0000320193-25-000079-index.htm",
            html,
        )
        .expect("descriptor");

        assert_eq!(descriptor.form_type, FormType::Form10K);
        assert_eq!(descriptor.filed_at, "2025-10-31");
        assert_eq!(descriptor.primary_document, "aapl-20250927.htm");
        assert_eq!(
            descriptor.primary_url,
            "https://www.sec.gov/Archives/edgar/data/320193/000032019325000079/aapl-20250927.htm"
        );
    }

    #[test]
    fn descriptor_from_index_parses_direct_archive_document() {
        let html = r#"
            <strong>Form 8-K</strong>
            <div class="infoHead">Filing Date</div>
            <div class="info">2025-10-31</div>
            <p>Document Format Files</p>
            <table class="tableFile">
              <tr>
                <td scope="row">1</td>
                <td scope="row">8-K</td>
                <td scope="row"><a href="/Archives/edgar/data/320193/000032019325000077/aapl-20251030.htm">aapl-20251030.htm</a></td>
                <td scope="row">8-K</td>
              </tr>
            </table>
        "#;

        let descriptor = descriptor_from_index(
            &apple_cik(),
            &AccessionNumber::parse("0000320193-25-000077").unwrap(),
            "https://www.sec.gov/Archives/edgar/data/320193/0000320193-25-000077-index.htm",
            html,
        )
        .expect("descriptor");

        assert_eq!(descriptor.form_type, FormType::Form8K);
        assert_eq!(descriptor.primary_document, "aapl-20251030.htm");
        assert_eq!(
            descriptor.primary_url,
            "https://www.sec.gov/Archives/edgar/data/320193/000032019325000077/aapl-20251030.htm"
        );
    }

    #[test]
    fn descriptor_from_submissions_picks_latest_matching_form() {
        let json = r#"{
          "filings": {
            "recent": {
              "accessionNumber": [
                "0000320193-26-000013",
                "0000320193-25-000079"
              ],
              "filingDate": [
                "2026-01-29",
                "2025-10-31"
              ],
              "form": [
                "10-Q",
                "10-K"
              ],
              "primaryDocument": [
                "aapl-20251227.htm",
                "aapl-20250927.htm"
              ]
            }
          }
        }"#;

        let descriptor =
            descriptor_from_submissions(&apple_cik(), &FormType::Form10K, json).expect("10-K");

        assert_eq!(descriptor.accession_number.as_str(), "0000320193-25-000079");
        assert_eq!(descriptor.filed_at, "2025-10-31");
        assert_eq!(
            descriptor.primary_url,
            "https://www.sec.gov/Archives/edgar/data/320193/000032019325000079/aapl-20250927.htm"
        );
    }

    #[test]
    fn build_filing_embeds_item_1a_section_for_10k() {
        let descriptor = FilingDescriptor {
            cik: apple_cik(),
            accession_number: apple_2025_accession(),
            form_type: FormType::Form10K,
            filed_at: "2025-10-31".to_string(),
            primary_document: "aapl-20250927.htm".to_string(),
            primary_url:
                "https://www.sec.gov/Archives/edgar/data/320193/000032019325000079/aapl-20250927.htm"
                    .to_string(),
            detail_url:
                "https://www.sec.gov/Archives/edgar/data/320193/0000320193-25-000079-index.htm"
                    .to_string(),
        };

        let mut html =
            "TOC: Item 1A. Risk Factors. FLS: Item 1A. Item 1A. Risk Factors".to_string();
        for i in 0..16 {
            write!(
                html,
                r#"<span style="font-style:italic;font-weight:700">Plausibly-long risk factor heading {i:02} that ends with a period.</span>"#
            )
            .unwrap();
        }
        html.push_str("Item 1B. Unresolved comments");

        let filing = build_filing(&descriptor, &html).expect("filing");
        let section = filing.sections.get("1A").expect("section");

        assert_eq!(filing.form_type, FormType::Form10K);
        assert_eq!(section.title, "Risk Factors");
        assert!(
            section
                .body
                .contains("Plausibly-long risk factor heading 00")
        );
    }
}
