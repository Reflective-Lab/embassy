// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::UsptoError;

/// The canonical identifier for this port's domain.
///
/// USPTO patent number format: `US` + 7-8 digits + 1-2 char kind code.
/// More broadly: 2-letter uppercase country code + digits + 1-2 alphanumeric
/// kind chars. Minimum total length: 9.
/// Examples: `US10000001B2`, `US20230012345A1`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PatentNumber(String);

impl PatentNumber {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, UsptoError> {
        let s = raw.as_ref().trim();
        if s.is_empty() {
            return Err(UsptoError::InvalidIdentifier("empty".into()));
        }
        let chars: Vec<char> = s.chars().collect();
        // Minimum: 2 country + 6 digits + 1 kind = 9
        if chars.len() < 9 {
            return Err(UsptoError::InvalidIdentifier(
                "invalid PatentNumber: too short; expected 2-letter country code + digits + kind code (min 9 chars)".into(),
            ));
        }
        if !chars[0].is_ascii_uppercase() || !chars[1].is_ascii_uppercase() {
            return Err(UsptoError::InvalidIdentifier(
                "invalid PatentNumber: must start with 2 uppercase country code letters (e.g. 'US')".into(),
            ));
        }
        // Require at least one digit in middle section
        if !chars[2..].iter().any(|c| c.is_ascii_digit()) {
            return Err(UsptoError::InvalidIdentifier(
                "invalid PatentNumber: no digit sequence found after country code".into(),
            ));
        }
        // Last char must be alphanumeric (kind code)
        if !chars.last().unwrap().is_ascii_alphanumeric() {
            return Err(UsptoError::InvalidIdentifier(
                "invalid PatentNumber: kind code (last 1-2 chars) must be alphanumeric".into(),
            ));
        }
        Ok(Self(s.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `patent_number` + `title` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patent {
    pub patent_number: PatentNumber,
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(PatentNumber::parse("").is_err());
        assert!(PatentNumber::parse("   ").is_err());
    }

    #[test]
    fn valid_patent_numbers_parse() {
        // Intent: well-formed USPTO patent numbers must be accepted so
        // records can be forwarded to the USPTO PatentsView API.
        assert_eq!(PatentNumber::parse("US10000001B2").unwrap().as_str(), "US10000001B2");
        assert_eq!(PatentNumber::parse("US20230012345A1").unwrap().as_str(), "US20230012345A1");
    }

    #[test]
    fn invalid_patent_numbers_rejected() {
        // Intent: numbers missing the 2-letter prefix or too short must be
        // caught at the boundary to avoid silent mismatches in patent lookups.
        assert!(PatentNumber::parse("STUB-001").is_err());   // old stub value
        assert!(PatentNumber::parse("10000001B2").is_err()); // missing country code
        assert!(PatentNumber::parse("us10000001B2").is_err()); // lowercase prefix
        assert!(PatentNumber::parse("US123").is_err());      // too short
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Patent {
            patent_number: PatentNumber::parse("US10000001B2").unwrap(),
            title: "Stub Patent".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Patent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
