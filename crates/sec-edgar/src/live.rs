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

use std::time::Duration;

use manifold::{
    ExtractedNode, HtmlExtractBackend, HttpFetchProvider, ScraperHtmlBackend, WebFetchBackend,
    WebFetchRequest,
};
use thiserror::Error;

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
        use std::fmt::Write as _;
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
        use std::fmt::Write as _;
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
        use std::fmt::Write as _;
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
}
