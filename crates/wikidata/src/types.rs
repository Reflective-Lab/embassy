// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::WikidataError;

embassy_pack::simple_id!(
    /// The canonical identifier for this port's domain.
    ///
    /// Format: `Q` followed by one or more ASCII digits, e.g. `Q42`.
    QId, WikidataError,
    |s: &str| {
        if !s.starts_with('Q') || s.len() < 2 || !s[1..].chars().all(|c| c.is_ascii_digit()) {
            return Err(WikidataError::InvalidIdentifier(
                "invalid QId: must be 'Q' followed by one or more digits".into(),
            ));
        }
        Ok(s.to_string())
    }
);

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `qid` + `label` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub qid: QId,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(QId::parse("").is_err());
        assert!(QId::parse("   ").is_err());
    }

    #[test]
    fn valid_qid_parses() {
        // Intent: a well-formed Wikidata entity ID must round-trip cleanly
        // so downstream consumers receive the canonical string they passed in.
        assert_eq!(QId::parse("Q42").unwrap().as_str(), "Q42");
        assert_eq!(QId::parse("Q12345").unwrap().as_str(), "Q12345");
    }

    #[test]
    fn invalid_qid_rejected() {
        // Intent: the type boundary must reject IDs that don't match the
        // documented Q<digits> format, preventing garbage from entering
        // the audit log or being forwarded to Wikidata's API.
        assert!(QId::parse("q42").is_err());     // lowercase q
        assert!(QId::parse("P42").is_err());     // wrong prefix (property)
        assert!(QId::parse("Q42x").is_err());    // non-digit suffix
    }

    #[test]
    fn qid_with_no_digits_rejected() {
        // Intent: "Q" alone must not be accepted — the digit portion is
        // mandatory to form a resolvable Wikidata entity URI.
        assert!(QId::parse("Q").is_err());
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Entity {
            qid: QId::parse("Q42").unwrap(),
            label: "Douglas Adams".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Entity = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
