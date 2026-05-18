// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::ScbError;

embassy_pack::simple_id!(
    /// The canonical identifier for this port's domain.
    ///
    /// SCB (Statistics Sweden) table ID format observed in the API:
    /// 2 uppercase letters + 4 digits + 1-3 uppercase letters + digits.
    /// Pattern: `[A-Z]{2}[0-9]{4}[A-Z]{1,3}[0-9]+`. Minimum length: 8.
    /// Examples: `BE0101A1`, `NV0119K1`, `AM0401A`.
    ///
    /// Note: the digit suffix length varies (1-2 observed in the wild). The
    /// validation below checks structural requirements — starts with 2 uppercase
    /// letters, followed by 4 digits, followed by 1+ uppercase letters, followed
    /// by 1+ digits — without constraining exact suffix length.
    TableId, ScbError,
    |s: &str| {
        if s.len() < 8 {
            return Err(ScbError::InvalidIdentifier(
                "invalid TableId: too short; expected at least 8 characters (e.g. 'BE0101A1')".into(),
            ));
        }
        let chars: Vec<char> = s.chars().collect();
        // First 2 chars: uppercase letters (subject area code)
        if !chars[0].is_ascii_uppercase() || !chars[1].is_ascii_uppercase() {
            return Err(ScbError::InvalidIdentifier(
                "invalid TableId: must start with 2 uppercase letters (subject area code)".into(),
            ));
        }
        // Next 4 chars: digits (statistical product code)
        if !chars[2..6].iter().all(|c| c.is_ascii_digit()) {
            return Err(ScbError::InvalidIdentifier(
                "invalid TableId: positions 3-6 must be digits (statistical product code)".into(),
            ));
        }
        // Remaining chars: at least one uppercase letter followed by at least one digit
        let rest = &chars[6..];
        let letter_run = rest.iter().take_while(|c| c.is_ascii_uppercase()).count();
        if letter_run == 0 {
            return Err(ScbError::InvalidIdentifier(
                "invalid TableId: expected at least one uppercase letter after the 4-digit product code".into(),
            ));
        }
        let digit_run = rest[letter_run..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digit_run == 0 {
            return Err(ScbError::InvalidIdentifier(
                "invalid TableId: expected digits after the subtable letter(s)".into(),
            ));
        }
        Ok(s.to_string())
    }
);

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `table_id` + `title` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableMetadata {
    pub table_id: TableId,
    pub title: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(TableId::parse("").is_err());
        assert!(TableId::parse("   ").is_err());
    }

    #[test]
    fn valid_table_ids_parse() {
        // Intent: real SCB table IDs observed in the PxWeb API must be
        // accepted so the port can fetch population and economic statistics
        // without rejecting valid production identifiers.
        assert_eq!(TableId::parse("BE0101A1").unwrap().as_str(), "BE0101A1");
        assert_eq!(TableId::parse("NV0119K1").unwrap().as_str(), "NV0119K1");
        assert_eq!(TableId::parse("AM0401A1").unwrap().as_str(), "AM0401A1");
    }

    #[test]
    fn invalid_table_ids_rejected() {
        // Intent: IDs that don't match the 2-letter + 4-digit + letter +
        // digit structure must be caught before they reach the SCB API,
        // where a bad table ID yields an opaque error.
        assert!(TableId::parse("STUB-001").is_err());     // old stub value
        assert!(TableId::parse("BE01").is_err());         // too short
        assert!(TableId::parse("be0101A1").is_err());     // lowercase subject code
        assert!(TableId::parse("BE010XA1").is_err());     // non-digit in product code
        assert!(TableId::parse("BE01011A").is_err());     // no trailing digit(s)
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = TableMetadata {
            table_id: TableId::parse("BE0101A1").unwrap(),
            title: "Population by region".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: TableMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
