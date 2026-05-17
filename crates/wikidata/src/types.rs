// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::WikidataError;

/// The canonical identifier for this port's domain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct QId(String);

impl QId {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, WikidataError> {
        let raw = raw.as_ref().trim();
        if raw.is_empty() {
            return Err(WikidataError::InvalidIdentifier("empty".into()));
        }
        Ok(Self(raw.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

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
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Entity {
            qid: QId::parse("STUB-001").unwrap(),
            label: "Stub Entity".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Entity = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
