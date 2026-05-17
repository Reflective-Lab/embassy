// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Typed SAM.gov domain — UEI newtype, contractor registration record.

use serde::{Deserialize, Serialize};

use crate::error::SamGovError;

/// US federal Unique Entity Identifier — 12 alphanumeric characters,
/// uppercased, no separators. SAM.gov assigns one per registered
/// entity; once assigned it never changes (a successor company keeps
/// the original UEI through M&A).
///
/// The port validates length + alphanumeric at parse time. SAM.gov's
/// check-digit scheme is not publicly documented in a stable form, so
/// no checksum validation is attempted; the live provider's 404 is the
/// authoritative answer to "is this UEI real?".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Uei(String);

impl Uei {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, SamGovError> {
        let raw = raw.as_ref().trim().to_ascii_uppercase();
        if raw.len() != 12 {
            return Err(SamGovError::InvalidUei(format!(
                "UEI must be 12 characters, got {}: `{raw}`",
                raw.len()
            )));
        }
        if !raw.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(SamGovError::InvalidUei(format!(
                "UEI must be alphanumeric: `{raw}`"
            )));
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Registration status at a given moment. SAM.gov distinguishes
/// Active (eligible to win federal awards) from Expired (still in the
/// system but lapsed) from Inactive (administratively removed). The
/// three answers drive operationally different decisions downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationStatus {
    Active,
    Expired,
    Inactive,
    Submitted,
}

/// One contractor registration record. The fields here are the
/// minimum set that drives downstream decisions ("is this entity
/// eligible to receive a federal award today?"). Richer fields
/// (NAICS codes, certifications, points of contact) live behind the
/// `live` feature when an app pulls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractorRegistration {
    pub uei: Uei,
    pub legal_name: String,
    pub registration_status: RegistrationStatus,
    /// ISO 8601 date the current registration expires. SAM
    /// registrations must be renewed annually; an Active record with
    /// an expiration in the past is a SAM bug, not a valid state.
    pub expiration_date: Option<String>,
    /// CAGE code — the legacy DoD Commercial and Government Entity
    /// code. Not all SAM entries carry one (only entities that have
    /// done DoD business).
    pub cage_code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uei_accepts_canonical_form() {
        // Intent: a valid 12-char alphanumeric UEI parses cleanly. The
        // happy path is by far the most common — if this regresses,
        // every SAM lookup fails at the boundary.
        let uei = Uei::parse("ABC123XYZ789").unwrap();
        assert_eq!(uei.as_str(), "ABC123XYZ789");
    }

    #[test]
    fn uei_uppercases_input() {
        // Intent: SAM.gov canonicalizes to uppercase; two sightings
        // ("abc123xyz789" and "ABC123XYZ789") must converge to one
        // record.
        let a = Uei::parse("abc123xyz789").unwrap();
        let b = Uei::parse("ABC123XYZ789").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn uei_rejects_wrong_length() {
        // Intent: SAM UEIs are exactly 12 chars. An 11- or 13-char
        // value is corrupt input — refuse loudly before any API call.
        assert!(Uei::parse("ABC123XYZ78").is_err());
        assert!(Uei::parse("ABC123XYZ7890").is_err());
    }

    #[test]
    fn uei_rejects_non_alphanumeric() {
        // Intent: punctuation in a UEI means corrupt input.
        assert!(Uei::parse("ABC-123-XYZ7").is_err());
    }

    #[test]
    fn registration_serde_round_trips() {
        let reg = ContractorRegistration {
            uei: Uei::parse("ABC123XYZ789").unwrap(),
            legal_name: "Stub Federal Contractor LLC".into(),
            registration_status: RegistrationStatus::Active,
            expiration_date: Some("2027-01-01".into()),
            cage_code: Some("1ABC2".into()),
        };
        let json = serde_json::to_string(&reg).unwrap();
        let back: ContractorRegistration = serde_json::from_str(&json).unwrap();
        assert_eq!(back, reg);
    }
}
