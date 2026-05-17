// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Typed GLEIF domain — LEI (ISO 17442), LegalEntity, status enums.

use serde::{Deserialize, Serialize};

use crate::error::GleifError;

/// Legal Entity Identifier — 20 characters per ISO 17442.
///
/// Structure:
/// - chars 1-4: LOU prefix (Local Operating Unit / issuer code)
/// - chars 5-6: reserved `"00"`
/// - chars 7-18: entity-specific code
/// - chars 19-20: mod-97-10 check digits
///
/// The port validates the check digit on construction. A typo in any
/// position fails parse rather than silently passing through to the
/// audit log.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Lei(String);

impl Lei {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, GleifError> {
        let raw = raw.as_ref().trim().to_ascii_uppercase();
        if raw.len() != 20 {
            return Err(GleifError::InvalidLei(format!(
                "expected 20 characters, got {} from `{raw}`",
                raw.len()
            )));
        }
        if !raw.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(GleifError::InvalidLei(format!(
                "LEI may only contain A-Z and 0-9: `{raw}`"
            )));
        }
        // Chars 5-6 are reserved "00" by the standard.
        if &raw[4..6] != "00" {
            return Err(GleifError::InvalidLei(format!(
                "chars 5-6 must be \"00\" per ISO 17442: `{raw}`"
            )));
        }
        if !verify_mod_97(&raw) {
            return Err(GleifError::InvalidLei(format!(
                "ISO 17442 mod-97 check digits invalid for `{raw}`"
            )));
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The issuer / LOU prefix (chars 1-4) — e.g., `"529900"` for some
    /// Bloomberg-issued LEIs.
    #[must_use]
    pub fn lou_prefix(&self) -> &str {
        &self.0[..4]
    }
}

/// ISO 17442 mod-97 check: the full 20-char LEI converted to numeric
/// (A=10, B=11, ..., Z=35) must satisfy `numeric % 97 == 1`.
fn verify_mod_97(lei: &str) -> bool {
    // Stream-fold the digits through mod-97 to stay in u64 without
    // ever materializing the (potentially 38-digit) full number.
    let mut residue: u64 = 0;
    for ch in lei.chars() {
        let value: u64 = if ch.is_ascii_digit() {
            u64::from(ch as u8 - b'0')
        } else if ch.is_ascii_uppercase() {
            u64::from(ch as u8 - b'A') + 10
        } else {
            return false;
        };
        // Each letter contributes two decimal digits (10..=35), each
        // digit contributes one. Treat both uniformly by shifting the
        // running residue left by the right number of decimal places.
        let shift = if value >= 10 { 100 } else { 10 };
        residue = (residue * shift + value) % 97;
    }
    residue == 1
}

/// GLEIF registration status — life-cycle of the LEI itself, distinct
/// from the legal status of the entity it identifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationStatus {
    /// Active, current registration.
    Issued,
    /// Issued but later voided (registration error, fraudulent issue).
    Annulled,
    /// Duplicate of another LEI; this one is withdrawn.
    Duplicate,
    /// Registration lapsed (renewal not received).
    Lapsed,
    /// Entity merged into another; this LEI no longer current.
    Merged,
    /// Entity retired (dissolved, wound up).
    Retired,
    /// Pending review / not yet issued.
    Pending,
}

/// Legal entity life-cycle — distinct from registration status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalEntityStatus {
    Active,
    Inactive,
}

/// GLEIF entity category — closed enum for the main categories with
/// `Other` for the long tail (per-jurisdiction sub-categories).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityCategory {
    /// Standard legal entity.
    General,
    /// Branch of a foreign entity.
    Branch,
    /// Investment fund.
    Fund,
    /// Sole proprietorship (single-person business with separate LEI).
    SoleProprietor,
    /// Resident government entity (ministry, agency, public body).
    ResidentGovernmental,
    /// International organization (UN, OECD, multilateral institutions).
    InternationalOrganization,
    /// Anything else — GLEIF keeps the long tail open.
    Other(String),
}

/// A single GLEIF legal entity record.
///
/// Minimal placeholder shape — apps that need more (full address,
/// successor LEI, complete relationship graph, registration history)
/// grow this when they pull. Headline identifier + name + status are
/// the load-bearing fields most consumers actually filter on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalEntity {
    pub lei: Lei,
    pub legal_name: String,
    /// ISO 3166-1 alpha-2 country code of the legal jurisdiction.
    pub jurisdiction: String,
    /// Free-form legal form code as published by GLEIF (e.g., `"XTIQ"`
    /// for Swedish Aktiebolag). Local interpretation belongs in a
    /// per-jurisdiction translation table, not in this port.
    pub legal_form_code: String,
    pub registration_status: RegistrationStatus,
    pub entity_status: LegalEntityStatus,
    pub entity_category: EntityCategory,
    /// LEI of the immediate parent, if any. GLEIF publishes both
    /// "direct" and "ultimate" parent relationships; this field is
    /// the direct parent. Ultimate-parent / child-graph traversal is
    /// a separate request shape (`GleifRequest::Ownership`).
    pub direct_parent_lei: Option<Lei>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A known valid LEI: Apple Inc. (HWUPKR0MPOU8FGXBT394).
    /// Sourced from gleif.org public registry.
    const APPLE_LEI: &str = "HWUPKR0MPOU8FGXBT394";

    #[test]
    fn lei_parses_canonical_form() {
        // Intent: the canonical 20-char form must parse without
        // mutation. If a future refactor changes the canonical
        // representation (e.g., adds hyphens), every downstream
        // key lookup breaks.
        let lei = Lei::parse(APPLE_LEI).unwrap();
        assert_eq!(lei.as_str(), APPLE_LEI);
    }

    #[test]
    fn lei_normalizes_case_and_whitespace() {
        // Intent: GLEIF API responses come in mixed case from
        // different sources. Normalize at the boundary so two
        // sightings of the same LEI converge to the same string.
        let lei = Lei::parse("  hwupkr0mpou8fgxbt394  ").unwrap();
        assert_eq!(lei.as_str(), APPLE_LEI);
    }

    #[test]
    fn lei_rejects_wrong_length() {
        // Intent: anything that's not exactly 20 characters is not
        // an LEI — refuse rather than truncate or pad.
        assert!(Lei::parse("HWUPKR0MPOU8FGXBT39").is_err()); // 19
        assert!(Lei::parse("HWUPKR0MPOU8FGXBT3941").is_err()); // 21
        assert!(Lei::parse("").is_err());
    }

    #[test]
    fn lei_rejects_non_alphanumeric() {
        // Intent: LEI is a strict alphanumeric namespace. Punctuation,
        // whitespace mid-token, or unicode means corrupt input.
        assert!(Lei::parse("HWUPKR-0MPOU8FGXBT394").is_err());
        assert!(Lei::parse("HWUPKR_0MPOU8FGXBT394").is_err());
    }

    #[test]
    fn lei_rejects_missing_reserved_zeros() {
        // Intent: the standard reserves chars 5-6 as `"00"`. Anything
        // else is a malformed LEI; even if the check digit somehow
        // passed it would still violate the spec.
        // Build a string that's mod-97-valid in shape but has wrong
        // reserved chars — should fail.
        assert!(Lei::parse("HWUPKR99POU8FGXBT394").is_err());
    }

    #[test]
    fn lei_rejects_invalid_check_digit() {
        // Intent: typos in any position should fail the mod-97 check.
        // Flip the last digit of a valid LEI.
        let mut bad = APPLE_LEI.to_string();
        let last = bad.pop().unwrap();
        // Pick a different digit; if last was '4', use '5'.
        let replacement = if last == '4' { '5' } else { '4' };
        bad.push(replacement);
        let result = Lei::parse(&bad);
        assert!(result.is_err(), "modified LEI must fail Luhn-style check");
    }

    #[test]
    fn lei_lou_prefix_is_first_four_chars() {
        // Intent: the LOU prefix is how downstream systems route to
        // the issuer's records. If this method ever changed which
        // chars it returns, every per-LOU filter would break.
        let lei = Lei::parse(APPLE_LEI).unwrap();
        assert_eq!(lei.lou_prefix(), "HWUP");
    }

    #[test]
    fn entity_category_other_round_trips_through_serde() {
        // Intent: GLEIF's category vocabulary grows; unknown values
        // must round-trip without information loss.
        let category = EntityCategory::Other("FUND-CLASS-X".to_string());
        let json = serde_json::to_string(&category).unwrap();
        let back: EntityCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(back, category);
    }

    #[test]
    fn legal_entity_serde_round_trip() {
        // Intent: every audit record must round-trip exactly so a
        // re-fetched entity can be compared against the stored
        // version for change detection.
        let entity = LegalEntity {
            lei: Lei::parse(APPLE_LEI).unwrap(),
            legal_name: "Apple Inc.".to_string(),
            jurisdiction: "US".to_string(),
            legal_form_code: "8888".to_string(),
            registration_status: RegistrationStatus::Issued,
            entity_status: LegalEntityStatus::Active,
            entity_category: EntityCategory::General,
            direct_parent_lei: None,
        };
        let json = serde_json::to_string(&entity).unwrap();
        let back: LegalEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entity);
    }
}
