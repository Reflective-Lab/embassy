// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::EpoError;

embassy_pack::simple_id!(
    /// The canonical identifier for this port's domain.
    ///
    /// EPO publication number format: 2-letter country code (uppercase) +
    /// 6-12 digits + 1-2 alphanumeric kind code chars. Minimum total length: 9.
    /// Examples: `EP1234567A1`, `WO2023012345A1`.
    EpoNumber, EpoError,
    |s: &str| {
        let chars: Vec<char> = s.chars().collect();
        // Minimum: 2 country + 6 digits + 1 kind = 9
        if chars.len() < 9 {
            return Err(EpoError::InvalidIdentifier(
                "invalid EpoNumber: too short; expected 2-letter country code + digits + kind code (min 9 chars)".into(),
            ));
        }
        if !chars[0].is_ascii_uppercase() || !chars[1].is_ascii_uppercase() {
            return Err(EpoError::InvalidIdentifier(
                "invalid EpoNumber: must start with 2 uppercase country code letters (e.g. 'EP', 'WO', 'US')".into(),
            ));
        }
        let middle_and_kind = &chars[2..];
        // Require at least one digit in middle section
        if !middle_and_kind.iter().any(|c| c.is_ascii_digit()) {
            return Err(EpoError::InvalidIdentifier(
                "invalid EpoNumber: no digit sequence found after country code".into(),
            ));
        }
        // Last char must be alphanumeric (kind code)
        if !chars.last().unwrap().is_ascii_alphanumeric() {
            return Err(EpoError::InvalidIdentifier(
                "invalid EpoNumber: kind code (last 1-2 chars) must be alphanumeric".into(),
            ));
        }
        Ok(s.to_string())
    }
);

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `publication_number` + `title` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patent {
    pub publication_number: EpoNumber,
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(EpoNumber::parse("").is_err());
        assert!(EpoNumber::parse("   ").is_err());
    }

    #[test]
    fn valid_epo_numbers_parse() {
        // Intent: well-formed EPO publication numbers with 2-letter country
        // code must be accepted so patent records can be forwarded to OPS.
        assert_eq!(EpoNumber::parse("EP1234567A1").unwrap().as_str(), "EP1234567A1");
        assert_eq!(EpoNumber::parse("WO2023012345A1").unwrap().as_str(), "WO2023012345A1");
        assert_eq!(EpoNumber::parse("US20230012345A1").unwrap().as_str(), "US20230012345A1");
    }

    #[test]
    fn invalid_epo_numbers_rejected() {
        // Intent: numbers without the 2-letter country prefix, or that are
        // too short to hold all required fields, must be rejected before
        // they reach the EPO OPS API.
        assert!(EpoNumber::parse("STUB-001").is_err());  // old stub value
        assert!(EpoNumber::parse("1234567A1").is_err()); // missing country code
        assert!(EpoNumber::parse("ep1234567A1").is_err()); // lowercase country code
        assert!(EpoNumber::parse("EP123").is_err());     // too short
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Patent {
            publication_number: EpoNumber::parse("EP1234567A1").unwrap(),
            title: "Stub Patent".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Patent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
