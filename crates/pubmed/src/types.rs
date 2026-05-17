// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::PubmedError;

/// The canonical identifier for this port's domain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Pmid(String);

impl Pmid {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, PubmedError> {
        let raw = raw.as_ref().trim();
        if raw.is_empty() {
            return Err(PubmedError::InvalidIdentifier("empty".into()));
        }
        Ok(Self(raw.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

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
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Article {
            pmid: Pmid::parse("STUB-001").unwrap(),
            title: "Stub Entity".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Article = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
