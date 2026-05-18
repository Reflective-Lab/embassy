// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::PubmedError;

embassy_pack::simple_id!(
    /// The canonical identifier for this port's domain.
    ///
    /// Format: one or more ASCII digits, e.g. `38765432`.
    Pmid, PubmedError,
    |s: &str| {
        if !s.chars().all(|c| c.is_ascii_digit()) {
            return Err(PubmedError::InvalidIdentifier(
                "invalid Pmid: must contain only ASCII digits".into(),
            ));
        }
        Ok(s.to_string())
    }
);

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `pmid` + `title` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Article {
    pub pmid: Pmid,
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(Pmid::parse("").is_err());
        assert!(Pmid::parse("   ").is_err());
    }

    #[test]
    fn valid_pmid_parses() {
        // Intent: a well-formed PubMed ID must round-trip cleanly so
        // downstream consumers receive the canonical string they passed in.
        assert_eq!(Pmid::parse("38765432").unwrap().as_str(), "38765432");
        assert_eq!(Pmid::parse("1").unwrap().as_str(), "1");
    }

    #[test]
    fn invalid_pmid_rejected() {
        // Intent: PMIDs are purely numeric; any non-digit character is a
        // sign the caller has the wrong format and must be rejected at
        // the boundary before it reaches PubMed's API.
        assert!(Pmid::parse("PMID38765432").is_err());  // text prefix
        assert!(Pmid::parse("38765 432").is_err());     // internal space
        assert!(Pmid::parse("387abc").is_err());        // letters mixed in
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Article {
            pmid: Pmid::parse("38765432").unwrap(),
            title: "Stub Article".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Article = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
