// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::OpenAlexError;

embassy_pack::simple_id!(
    /// The canonical identifier for this port's domain.
    ///
    /// Format: one of `W` (work), `A` (author), `I` (institution), `S` (source),
    /// `C` (concept), `P` (publisher), `F` (funder) followed by ASCII digits.
    /// Examples: `W2741809807`, `A5023888391`.
    OpenAlexId, OpenAlexError,
    |s: &str| {
        let first = s
            .chars()
            .next()
            .ok_or_else(|| OpenAlexError::InvalidIdentifier("empty".into()))?;
        if !matches!(first, 'W' | 'A' | 'I' | 'S' | 'C' | 'P' | 'F') {
            return Err(OpenAlexError::InvalidIdentifier(
                "invalid OpenAlexId: must start with W, A, I, S, C, P, or F".into(),
            ));
        }
        if s.len() < 2 || !s[1..].chars().all(|c| c.is_ascii_digit()) {
            return Err(OpenAlexError::InvalidIdentifier(
                "invalid OpenAlexId: entity type prefix must be followed by one or more digits".into(),
            ));
        }
        Ok(s.to_string())
    }
);

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `id` + `title` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    pub id: OpenAlexId,
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(OpenAlexId::parse("").is_err());
        assert!(OpenAlexId::parse("   ").is_err());
    }

    #[test]
    fn valid_openalex_ids_parse() {
        // Intent: all seven documented entity-type prefixes must be accepted
        // so the port can handle works, authors, institutions, sources,
        // concepts, publishers, and funders uniformly.
        assert_eq!(OpenAlexId::parse("W2741809807").unwrap().as_str(), "W2741809807");
        assert_eq!(OpenAlexId::parse("A5023888391").unwrap().as_str(), "A5023888391");
        assert_eq!(OpenAlexId::parse("I27837315").unwrap().as_str(), "I27837315");
        assert_eq!(OpenAlexId::parse("S1983995261").unwrap().as_str(), "S1983995261");
        assert_eq!(OpenAlexId::parse("C2778407487").unwrap().as_str(), "C2778407487");
        assert_eq!(OpenAlexId::parse("P4310320511").unwrap().as_str(), "P4310320511");
        assert_eq!(OpenAlexId::parse("F4320332161").unwrap().as_str(), "F4320332161");
    }

    #[test]
    fn invalid_openalex_ids_rejected() {
        // Intent: any prefix that isn't one of the seven documented entity
        // types must be rejected to prevent silent misrouting in the
        // OpenAlex API.
        assert!(OpenAlexId::parse("STUB-001").is_err());   // old stub value
        assert!(OpenAlexId::parse("X12345").is_err());     // unknown prefix
        assert!(OpenAlexId::parse("w12345").is_err());     // lowercase prefix
        assert!(OpenAlexId::parse("W").is_err());          // no digits
        assert!(OpenAlexId::parse("W123abc").is_err());    // non-digit after prefix
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Work {
            id: OpenAlexId::parse("W2741809807").unwrap(),
            title: "Stub Work".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Work = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
