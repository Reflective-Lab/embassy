// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Typed VIES domain — country-coded VAT number, validation result.

use serde::{Deserialize, Serialize};

use crate::error::ViesError;

/// EU member state ISO 3166-1 alpha-2 code. Validates membership at
/// parse time so a non-EU country code (`US`, `CH`, `NO`) is rejected
/// loudly before any VIES call would have happened.
///
/// The "GR" → "EL" exception: Greece's ISO code is `GR` but its EU VAT
/// prefix is the legacy `EL`. The port accepts both on input but
/// canonicalizes to the VAT-prefix form (`EL`) for storage, which is
/// the form VIES itself uses on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CountryCode(String);

impl CountryCode {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, ViesError> {
        let raw = raw.as_ref().trim().to_ascii_uppercase();
        if raw.len() != 2 {
            return Err(ViesError::InvalidVatNumber(format!(
                "country code must be 2 characters, got `{raw}`"
            )));
        }
        // Greece special case — accept GR, canonicalize to EL.
        let canonical = if raw == "GR" { "EL".to_string() } else { raw };
        if !EU_VAT_PREFIXES.contains(&canonical.as_str()) {
            return Err(ViesError::NonEuCountry(canonical));
        }
        Ok(Self(canonical))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The 27 EU member states + the Northern Ireland VAT prefix (XI) +
/// Greece's VAT prefix legacy form (EL instead of GR).
const EU_VAT_PREFIXES: &[&str] = &[
    "AT", "BE", "BG", "CY", "CZ", "DE", "DK", "EE", "EL", "ES", "FI", "FR", "HR", "HU", "IE", "IT",
    "LT", "LU", "LV", "MT", "NL", "PL", "PT", "RO", "SE", "SI", "SK", "XI",
];

/// A complete EU VAT number = `CountryCode` + national portion.
///
/// On parse the port validates the country code is in the EU member
/// set and the national portion is non-empty. Per-country format
/// (digit count, letter positions, checksums) is deferred to the live
/// feature — the format rules differ across all 27 member states and
/// evolve with national tax administration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VatNumber {
    pub country: CountryCode,
    pub national: String,
}

impl VatNumber {
    /// Parse a wire-format VAT number like `"SE556074308901"` or
    /// `"GR090145420"`. The two-letter prefix is split off and parsed
    /// as a [`CountryCode`]; the remainder becomes [`Self::national`].
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, ViesError> {
        let raw = raw.as_ref().trim().replace(' ', "");
        if raw.len() < 3 {
            return Err(ViesError::InvalidVatNumber(format!(
                "too short to contain a country prefix + national number: `{raw}`"
            )));
        }
        let (cc, national) = raw.split_at(2);
        let country = CountryCode::parse(cc)?;
        if national.is_empty() {
            return Err(ViesError::InvalidVatNumber(format!(
                "national portion empty after stripping `{cc}` prefix"
            )));
        }
        if !national.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ViesError::InvalidVatNumber(format!(
                "national portion must be alphanumeric: `{national}`"
            )));
        }
        Ok(Self {
            country,
            national: national.to_ascii_uppercase(),
        })
    }

    /// Canonical wire form: country prefix + national portion, no
    /// separator. Reverses [`Self::parse`].
    #[must_use]
    pub fn to_wire(&self) -> String {
        format!("{}{}", self.country.as_str(), self.national)
    }
}

/// Whether VIES could reach the member state's database at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberStateStatus {
    /// Member state DB responded.
    Available,
    /// Member state DB was offline; the validity answer is unknown.
    /// Distinct from a `false` validity — re-trying may succeed.
    Unavailable,
}

/// The validity of one VAT number at one moment in time as reported by
/// the requesting member state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VatValidation {
    pub vat_number: VatNumber,
    pub valid: bool,
    pub member_state_status: MemberStateStatus,
    /// VIES `requestIdentifier` — the contemporaneous proof-of-check
    /// number tax authorities accept later. Always present when the
    /// member state was Available, even for a `valid = false` answer.
    /// Absent when `member_state_status = Unavailable`.
    pub consultation_number: Option<String>,
    /// Registered name returned by the member state — only populated
    /// for `valid = true` answers, and only when the member state
    /// chooses to disclose (some redact for privacy reasons).
    pub registered_name: Option<String>,
    /// Registered address — same disclosure rules as name.
    pub registered_address: Option<String>,
    /// ISO-8601 timestamp of the validation. The port records the
    /// caller-side wallclock; VIES does not return a server-side
    /// timestamp on the wire.
    pub validated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn country_code_accepts_eu_members() {
        // Intent: the 27 EU codes + XI (Northern Ireland) + EL
        // (Greek VAT legacy) all parse. If a future refactor drops
        // any of them, perfectly valid VAT numbers from that member
        // state stop validating.
        for cc in ["SE", "DE", "FR", "NL", "ES", "EL", "XI"] {
            assert!(
                CountryCode::parse(cc).is_ok(),
                "EU country `{cc}` must parse"
            );
        }
    }

    #[test]
    fn country_code_canonicalizes_gr_to_el() {
        // Intent: Greece's ISO code is GR but its VAT prefix is EL.
        // Apps that send "GR" (the ISO form) get normalized to "EL"
        // (the wire form) before any VIES call would happen. If this
        // ever stops, a real Greek VAT number would round-trip to a
        // wire form VIES rejects.
        let cc = CountryCode::parse("GR").unwrap();
        assert_eq!(cc.as_str(), "EL");
    }

    #[test]
    fn country_code_rejects_non_eu() {
        // Intent: non-EU country codes should fail loudly at the port
        // boundary. VIES has no record of US/CH/NO VAT numbers; the
        // failure mode without this check is a long, opaque transport
        // error rather than a clear contract violation.
        assert!(matches!(
            CountryCode::parse("US"),
            Err(ViesError::NonEuCountry(_))
        ));
        assert!(matches!(
            CountryCode::parse("CH"),
            Err(ViesError::NonEuCountry(_))
        ));
    }

    #[test]
    fn vat_number_round_trips_through_wire_form() {
        // Intent: parse(raw).to_wire() must equal the canonical raw
        // input. If the wire form ever changes shape (e.g., adds a
        // separator), every downstream system that compares two
        // sightings of the same VAT number byte-for-byte breaks.
        let raw = "SE556074308901";
        let vat = VatNumber::parse(raw).unwrap();
        assert_eq!(vat.to_wire(), raw);
    }

    #[test]
    fn vat_number_strips_whitespace() {
        // Intent: VAT numbers in the wild are formatted with spaces
        // for legibility (`"SE 5560 7430 8901"`). Normalize at the
        // port so two sightings of the same number — one pretty,
        // one terse — converge to one canonical record.
        let pretty = "SE 5560 7430 8901";
        let terse = "SE556074308901";
        assert_eq!(
            VatNumber::parse(pretty).unwrap().to_wire(),
            VatNumber::parse(terse).unwrap().to_wire()
        );
    }

    #[test]
    fn vat_number_canonicalizes_greek_prefix() {
        // Intent: a VAT number sent with the ISO Greece prefix (GR)
        // must round-trip out with the VIES wire prefix (EL).
        let vat = VatNumber::parse("GR090145420").unwrap();
        assert_eq!(vat.country.as_str(), "EL");
        assert_eq!(vat.to_wire(), "EL090145420");
    }

    #[test]
    fn vat_number_rejects_missing_national_portion() {
        // Intent: a country code with nothing after it isn't a VAT
        // number — refuse rather than treat as an empty-VAT lookup.
        assert!(VatNumber::parse("SE").is_err());
        // After whitespace strip the national portion is empty.
        assert!(VatNumber::parse("SE  ").is_err());
    }

    #[test]
    fn vat_number_rejects_non_alphanumeric_national() {
        // Intent: punctuation in the national portion (other than
        // stripped whitespace) means corrupt input. The NL format
        // includes a literal `B` letter which is alphanumeric, so we
        // accept letters generally.
        assert!(VatNumber::parse("SE5560/74308901").is_err());
    }

    #[test]
    fn vat_validation_serde_round_trip() {
        // Intent: every validation observation must round-trip
        // exactly through serde so a stored record can be byte-
        // compared against a re-fetch for change detection.
        let validation = VatValidation {
            vat_number: VatNumber::parse("SE556074308901").unwrap(),
            valid: true,
            member_state_status: MemberStateStatus::Available,
            consultation_number: Some("WAPIAAAAVskdj92".to_string()),
            registered_name: Some("Volvo Car Corporation".to_string()),
            registered_address: Some("Göteborg, Sweden".to_string()),
            validated_at: "2026-05-17T12:34:56Z".to_string(),
        };
        let json = serde_json::to_string(&validation).unwrap();
        let back: VatValidation = serde_json::from_str(&json).unwrap();
        assert_eq!(back, validation);
    }
}
