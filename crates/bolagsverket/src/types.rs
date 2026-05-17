// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Typed Bolagsverket domain — OrgNumber, LegalForm, Company.

use serde::{Deserialize, Serialize};

use crate::error::BolagsverketError;

/// Swedish organisationsnummer — 10 digits with Luhn check digit.
///
/// Canonical form: `NNNNNN-NNNN`. The port accepts:
/// - bare digits (`5566778899`)
/// - hyphenated form (`556677-8899`)
/// - personnummer form with century prefix (`19556677-8899`) — the
///   prefix is stripped on parse.
///
/// On construction the Luhn-mod-10 check digit is validated; bogus IDs
/// fail at the port boundary rather than leaking into the audit trail.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrgNumber(String);

impl OrgNumber {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, BolagsverketError> {
        let raw = raw.as_ref().trim();
        if raw.is_empty() {
            return Err(BolagsverketError::InvalidOrgNumber("empty".into()));
        }
        let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
        // Strip optional century prefix (12 → 10 digits).
        let digits = match digits.len() {
            10 => digits,
            12 => digits[2..].to_string(),
            n => {
                return Err(BolagsverketError::InvalidOrgNumber(format!(
                    "expected 10 or 12 digits, got {n} from `{raw}`"
                )));
            }
        };
        if !luhn_check(&digits) {
            return Err(BolagsverketError::InvalidOrgNumber(format!(
                "Luhn check digit invalid for `{raw}`"
            )));
        }
        Ok(Self(format!("{}-{}", &digits[..6], &digits[6..])))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the 10-digit form without the hyphen — useful for API
    /// calls that need the compact representation.
    #[must_use]
    pub fn as_digits(&self) -> String {
        self.0.chars().filter(char::is_ascii_digit).collect()
    }
}

/// Luhn-mod-10 check on the full 10-digit string.
fn luhn_check(digits: &str) -> bool {
    if digits.len() != 10 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let total: u32 = digits
        .chars()
        .rev()
        .enumerate()
        .map(|(i, c)| {
            let d = c.to_digit(10).unwrap_or(0);
            if i % 2 == 1 {
                let doubled = d * 2;
                if doubled > 9 { doubled - 9 } else { doubled }
            } else {
                d
            }
        })
        .sum();
    total % 10 == 0
}

/// Swedish legal form (företagsform).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalForm {
    /// Aktiebolag — limited liability company.
    Aktiebolag,
    /// Handelsbolag — general partnership.
    Handelsbolag,
    /// Kommanditbolag — limited partnership.
    Kommanditbolag,
    /// Enskild näringsidkare — sole trader.
    EnskildFirma,
    /// Ekonomisk förening — cooperative.
    EkonomiskForening,
    /// Stiftelse — foundation.
    Stiftelse,
    /// Ideell förening — non-profit association.
    IdeellForening,
    /// Anything else — the Bolagsverket vocabulary has tail categories
    /// (filialer, trossamfund, etc.) that don't need typed variants.
    Other(String),
}

impl LegalForm {
    #[must_use]
    pub fn as_label(&self) -> &str {
        match self {
            Self::Aktiebolag => "Aktiebolag",
            Self::Handelsbolag => "Handelsbolag",
            Self::Kommanditbolag => "Kommanditbolag",
            Self::EnskildFirma => "Enskild näringsidkare",
            Self::EkonomiskForening => "Ekonomisk förening",
            Self::Stiftelse => "Stiftelse",
            Self::IdeellForening => "Ideell förening",
            Self::Other(label) => label,
        }
    }
}

/// Registration status reported by Bolagsverket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompanyStatus {
    Active,
    Dormant,
    InLiquidation,
    Bankrupt,
    Deregistered,
}

/// Swedish address — kept minimal; the port doesn't model every
/// Bolagsverket address sub-field, only what most apps actually filter on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address {
    pub street: String,
    pub postal_code: String,
    pub city: String,
    pub country: String,
}

/// A single Swedish company registration — the typed source domain object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Company {
    pub org_number: OrgNumber,
    pub name: String,
    pub legal_form: LegalForm,
    pub status: CompanyStatus,
    /// ISO-8601 date of registration. Kept as a string for the same
    /// timezone-agnostic reasons as SEC filing dates.
    pub registered_at: String,
    pub registered_address: Option<Address>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A well-known valid Swedish org number (Volvo Cars AB).
    const VOLVO_CARS: &str = "556074-3089";

    #[test]
    fn org_number_accepts_canonical_form() {
        // Intent: the canonical hyphenated form must parse without
        // mutation. If a future "cleanup" inadvertently changes the
        // canonical hyphen position, every downstream key lookup breaks.
        let n = OrgNumber::parse(VOLVO_CARS).unwrap();
        assert_eq!(n.as_str(), VOLVO_CARS);
    }

    #[test]
    fn org_number_normalizes_compact_and_personnummer_forms() {
        // Intent: callers come from many sources (CSV imports, manual
        // entry, foreign systems). All valid representations must
        // converge to the same canonical string — otherwise two
        // sightings of the same company would create two distinct
        // facts in the kernel.
        assert_eq!(OrgNumber::parse("5560743089").unwrap().as_str(), VOLVO_CARS);
        assert_eq!(
            OrgNumber::parse("19556074-3089").unwrap().as_str(),
            VOLVO_CARS
        );
    }

    #[test]
    fn org_number_rejects_invalid_luhn() {
        // Intent: typos in org numbers are common (one transposed
        // digit). The Luhn check is the cheap defence; rejecting at
        // the port keeps garbage out of the audit log.
        // Take a valid number and flip the last digit.
        let bad = "556074-3088";
        let err = OrgNumber::parse(bad).expect_err("Luhn must catch this");
        assert!(matches!(err, BolagsverketError::InvalidOrgNumber(_)));
    }

    #[test]
    fn org_number_rejects_wrong_digit_count() {
        assert!(OrgNumber::parse("12345").is_err());
        assert!(OrgNumber::parse("123456789012345").is_err());
        assert!(OrgNumber::parse("").is_err());
    }

    #[test]
    fn org_number_as_digits_strips_hyphen() {
        // Intent: some downstream APIs want the compact form, some
        // want hyphenated. Both must be derivable from one stored
        // value so we never have to round-trip through a parser.
        let n = OrgNumber::parse(VOLVO_CARS).unwrap();
        assert_eq!(n.as_digits(), "5560743089");
    }

    #[test]
    fn legal_form_serde_round_trips_known_variants() {
        // Intent: serialization stability for the typed variants. If
        // serde rename ever drifts, every persisted Company would need
        // a migration.
        for form in [
            LegalForm::Aktiebolag,
            LegalForm::Handelsbolag,
            LegalForm::Kommanditbolag,
            LegalForm::EnskildFirma,
            LegalForm::EkonomiskForening,
            LegalForm::Stiftelse,
            LegalForm::IdeellForening,
        ] {
            let json = serde_json::to_string(&form).unwrap();
            let back: LegalForm = serde_json::from_str(&json).unwrap();
            assert_eq!(back, form);
        }
    }

    #[test]
    fn legal_form_preserves_unknown_via_other() {
        // Intent: same forward-compat argument as SEC FormType — the
        // Bolagsverket vocabulary has tail entries (filialer,
        // trossamfund) and the port must not lose them.
        let raw = "filial";
        let form = LegalForm::Other(raw.to_string());
        let json = serde_json::to_string(&form).unwrap();
        let back: LegalForm = serde_json::from_str(&json).unwrap();
        assert_eq!(back, form);
    }

    #[test]
    fn company_serde_round_trip_preserves_all_fields() {
        // Intent: every audit record must round-trip exactly so a
        // re-fetched company can be compared against the stored
        // version for change detection.
        let company = Company {
            org_number: OrgNumber::parse(VOLVO_CARS).unwrap(),
            name: "Volvo Car Corporation".to_string(),
            legal_form: LegalForm::Aktiebolag,
            status: CompanyStatus::Active,
            registered_at: "1915-04-14".to_string(),
            registered_address: Some(Address {
                street: "Assar Gabrielssons väg".into(),
                postal_code: "405 31".into(),
                city: "Göteborg".into(),
                country: "Sverige".into(),
            }),
        };
        let json = serde_json::to_string(&company).unwrap();
        let back: Company = serde_json::from_str(&json).unwrap();
        assert_eq!(back, company);
    }
}
