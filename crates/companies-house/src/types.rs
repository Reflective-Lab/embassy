// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::CompaniesHouseError;

embassy_pack::simple_id!(
    /// The canonical identifier for this port's domain.
    ///
    /// Companies House company number format: either 8 ASCII digits (e.g.
    /// `12345678`) or a 2-letter uppercase prefix followed by 6 digits (e.g.
    /// `SC012345`, `NI000001`). Total length is always exactly 8 characters.
    CompanyNumber, CompaniesHouseError,
    |s: &str| {
        let is_all_digits = s.len() == 8 && s.chars().all(|c| c.is_ascii_digit());
        let is_prefixed = s.len() == 8
            && s[..2].chars().all(|c| c.is_ascii_uppercase())
            && s[2..].chars().all(|c| c.is_ascii_digit());
        if !is_all_digits && !is_prefixed {
            return Err(CompaniesHouseError::InvalidIdentifier(
                "invalid CompanyNumber: must be 8 digits or 2 uppercase letters followed by 6 digits".into(),
            ));
        }
        Ok(s.to_string())
    }
);

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `company_number` + `company_name` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Company {
    pub company_number: CompanyNumber,
    pub company_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(CompanyNumber::parse("").is_err());
        assert!(CompanyNumber::parse("   ").is_err());
    }

    #[test]
    fn valid_company_numbers_parse() {
        // Intent: both the all-digit and the 2-letter-prefix variants must
        // be accepted so the port works for England/Wales, Scotland, and
        // Northern Ireland registrations alike.
        assert_eq!(CompanyNumber::parse("12345678").unwrap().as_str(), "12345678");
        assert_eq!(CompanyNumber::parse("SC012345").unwrap().as_str(), "SC012345");
        assert_eq!(CompanyNumber::parse("NI000001").unwrap().as_str(), "NI000001");
        assert_eq!(CompanyNumber::parse("OC123456").unwrap().as_str(), "OC123456");
    }

    #[test]
    fn invalid_company_numbers_rejected() {
        // Intent: numbers with wrong length or disallowed char patterns must
        // be rejected before reaching the Companies House API to avoid
        // misleading 404 responses.
        assert!(CompanyNumber::parse("STUB-001").is_err());  // old stub value
        assert!(CompanyNumber::parse("1234567").is_err());   // 7 digits, too short
        assert!(CompanyNumber::parse("123456789").is_err()); // 9 digits, too long
        assert!(CompanyNumber::parse("sc012345").is_err());  // lowercase prefix
        assert!(CompanyNumber::parse("S1012345").is_err());  // 1-char prefix
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Company {
            company_number: CompanyNumber::parse("12345678").unwrap(),
            company_name: "Stub Ltd".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Company = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
