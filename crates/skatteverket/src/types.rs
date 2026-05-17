// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Typed Skatteverket domain — organisation number + tax-administrative
//! status (publicly queryable surface only).

use serde::{Deserialize, Serialize};

use crate::error::SkatteverketError;

/// Swedish organisation number (organisationsnummer) — 10 digits, no
/// separator on the wire, occasionally written with a hyphen for
/// readability (`556074-3089`). The port accepts both, strips the
/// hyphen, and stores the 10-digit canonical form.
///
/// The first digit is fixed by entity type (1 = state, 2 = county/
/// municipality, 3 = foreign-owned, 5 = AB, 6 = enskild firma in some
/// contexts, 7-9 = various). The port does not enforce the type digit
/// since legitimate edge cases exist; it validates length and digit
/// shape only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SwedishOrgNumber(String);

impl SwedishOrgNumber {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, SkatteverketError> {
        let cleaned: String = raw.as_ref().chars().filter(|c| *c != '-').collect();
        let cleaned = cleaned.trim();
        if cleaned.len() != 10 {
            return Err(SkatteverketError::InvalidOrgNumber(format!(
                "must be 10 digits after stripping hyphen, got {}: `{cleaned}`",
                cleaned.len()
            )));
        }
        if !cleaned.chars().all(|c| c.is_ascii_digit()) {
            return Err(SkatteverketError::InvalidOrgNumber(format!(
                "must be all digits: `{cleaned}`"
            )));
        }
        Ok(Self(cleaned.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Human-friendly form with a hyphen between the 6th and 7th
    /// digits — `556074-3089`. The port stores the canonical no-hyphen
    /// form; this is for display.
    #[must_use]
    pub fn to_display(&self) -> String {
        format!("{}-{}", &self.0[..6], &self.0[6..])
    }
}

/// Snapshot of an organisation's tax-administrative status at one
/// moment in time, as Skatteverket publicly discloses.
///
/// All three booleans are publicly queryable. None of them imply
/// access to amounts, history, or any personal data — they are the
/// minimum set Skatteverket exposes for legitimate business-to-business
/// diligence ("can I pay this counterparty without becoming liable
/// for their tax?").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwedishTaxStatus {
    pub org_number: SwedishOrgNumber,
    /// `true` if the organisation is registered for F-skatt — the
    /// payer is *not* responsible for withholding tax on payments
    /// (the recipient handles it).
    pub has_f_skatt: bool,
    /// `true` if registered for Swedish VAT (moms).
    pub vat_registered: bool,
    /// `true` if registered as an employer (arbetsgivarregistrering).
    pub employer_registered: bool,
    /// ISO 8601 timestamp the status was observed. Skatteverket does
    /// not stamp an `as_of` on the wire — the port records caller-side
    /// wallclock.
    pub as_of: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn org_number_accepts_hyphenated_form() {
        // Intent: humans write org numbers with a hyphen
        // (`556074-3089`). The port must accept the readable form so
        // CSV imports and form fields don't fail at the boundary.
        let a = SwedishOrgNumber::parse("556074-3089").unwrap();
        let b = SwedishOrgNumber::parse("5560743089").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn org_number_rejects_wrong_length() {
        // Intent: 9- or 11-digit values are corrupt input. Refuse
        // loudly before any Skatteverket query.
        assert!(SwedishOrgNumber::parse("556074308").is_err());
        assert!(SwedishOrgNumber::parse("55607430899").is_err());
    }

    #[test]
    fn org_number_rejects_non_digit() {
        // Intent: letters in an org number mean corrupt input.
        assert!(SwedishOrgNumber::parse("55607X3089").is_err());
    }

    #[test]
    fn org_number_display_form_inserts_hyphen() {
        // Intent: the display form is the readable representation. If
        // a refactor breaks it, UI/CSV exports degrade visibly.
        let orgnr = SwedishOrgNumber::parse("5560743089").unwrap();
        assert_eq!(orgnr.to_display(), "556074-3089");
    }

    #[test]
    fn tax_status_serde_round_trips() {
        // Intent: status records must round-trip byte-for-byte so a
        // stored snapshot can be compared against a re-fetch for
        // change detection (lapsed F-skatt is operationally
        // significant for any payer).
        let status = SwedishTaxStatus {
            org_number: SwedishOrgNumber::parse("5560743089").unwrap(),
            has_f_skatt: true,
            vat_registered: true,
            employer_registered: true,
            as_of: "2026-05-17T12:00:00Z".into(),
        };
        let json = serde_json::to_string(&status).unwrap();
        let back: SwedishTaxStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, status);
    }
}
