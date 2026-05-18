// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::ArxivError;

fn is_new_style(s: &str) -> bool {
    if let Some((prefix, rest)) = s.split_once('.') {
        if prefix.len() == 4 && prefix.chars().all(|c| c.is_ascii_digit()) {
            let base = rest.split('v').next().unwrap_or(rest);
            let ver = rest.get(base.len()..).unwrap_or("");
            let ver_ok = ver.is_empty()
                || (ver.starts_with('v')
                    && ver.len() > 1
                    && ver[1..].chars().all(|c| c.is_ascii_digit()));
            return (base.len() >= 4 && base.len() <= 5)
                && base.chars().all(|c| c.is_ascii_digit())
                && ver_ok;
        }
    }
    false
}

fn is_old_style(s: &str) -> bool {
    if let Some((archive, num)) = s.split_once('/') {
        return !archive.is_empty()
            && archive.chars().all(|c| c.is_ascii_alphabetic() || c == '-')
            && num.len() == 7
            && num.chars().all(|c| c.is_ascii_digit());
    }
    false
}

embassy_pack::simple_id!(
    /// The canonical identifier for this port's domain.
    ///
    /// Two accepted formats:
    /// - New-style: `YYMM.NNNNN[vN]` — 4-digit year+month, dot, 4-5 digits,
    ///   optional version suffix `v` followed by digits. Example: `2301.00001` or
    ///   `2301.00001v2`.
    /// - Old-style: `<archive>/<YYMMNNN>` — alphabetic archive prefix, slash,
    ///   7 digits. Example: `hep-th/9901001`.
    ArxivId, ArxivError,
    |s: &str| {
        if !is_new_style(s) && !is_old_style(s) {
            return Err(ArxivError::InvalidIdentifier(
                "invalid ArxivId: expected new-style 'YYMM.NNNNN[vN]' or old-style 'archive/YYMMNNN'".into(),
            ));
        }
        Ok(s.to_string())
    }
);

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `arxiv_id` + `title` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paper {
    pub arxiv_id: ArxivId,
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(ArxivId::parse("").is_err());
        assert!(ArxivId::parse("   ").is_err());
    }

    #[test]
    fn valid_arxiv_ids_parse() {
        // Intent: both the new-style YYMM.NNNNN and old-style archive/YYMMNNN
        // formats must be accepted so existing citation data from either era
        // can pass through the boundary unchanged.
        assert_eq!(ArxivId::parse("2301.00001").unwrap().as_str(), "2301.00001");
        assert_eq!(ArxivId::parse("2301.00001v2").unwrap().as_str(), "2301.00001v2");
        assert_eq!(ArxivId::parse("hep-th/9901001").unwrap().as_str(), "hep-th/9901001");
        assert_eq!(ArxivId::parse("math/0503001").unwrap().as_str(), "math/0503001");
    }

    #[test]
    fn invalid_arxiv_ids_rejected() {
        // Intent: malformed IDs that don't match either documented format must
        // be caught here rather than causing a cryptic 404 from the arXiv API.
        assert!(ArxivId::parse("STUB-001").is_err());      // old stub value
        assert!(ArxivId::parse("2301.1").is_err());        // too few digits after dot
        assert!(ArxivId::parse("2301.123456").is_err());   // too many digits after dot
        assert!(ArxivId::parse("23010001").is_err());      // missing dot, no slash
    }

    #[test]
    fn new_style_with_version_suffix_parses() {
        // Intent: version suffixes like v2 are part of the published arXiv
        // identifier spec and must not be stripped or rejected.
        assert!(ArxivId::parse("2301.00001v2").is_ok());
        assert!(ArxivId::parse("2301.00001v10").is_ok());
        assert!(ArxivId::parse("2301.00001v").is_err()); // bare 'v' without digits
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Paper {
            arxiv_id: ArxivId::parse("2301.00001").unwrap(),
            title: "Stub Paper".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Paper = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
