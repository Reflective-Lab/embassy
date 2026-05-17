// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Typed SEC EDGAR domain — CIK, AccessionNumber, FormType, Filing.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::error::SecEdgarError;

/// Central Index Key — SEC's unique identifier for an entity that files.
///
/// Always stored as a 10-digit zero-padded string. Construct via
/// [`Cik::parse`]; the input may be either the bare digits (`320193`)
/// or the zero-padded form (`0000320193`). The port normalizes on
/// parse so downstream comparisons are stable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cik(String);

impl Cik {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, SecEdgarError> {
        let raw = raw.as_ref().trim();
        if raw.is_empty() {
            return Err(SecEdgarError::InvalidCik("empty".into()));
        }
        if !raw.chars().all(|c| c.is_ascii_digit()) {
            return Err(SecEdgarError::InvalidCik(format!(
                "non-digit characters in `{raw}`"
            )));
        }
        if raw.len() > 10 {
            return Err(SecEdgarError::InvalidCik(format!(
                "CIK longer than 10 digits: `{raw}`"
            )));
        }
        Ok(Self(format!("{raw:0>10}")))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// SEC accession number — the unique filing identifier.
///
/// Wire format: `XXXXXXXXXX-XX-XXXXXX` (10-2-6 digits separated by
/// hyphens). The port accepts either the hyphenated form or the
/// 18-digit-only form and normalizes to hyphenated for storage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AccessionNumber(String);

impl AccessionNumber {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, SecEdgarError> {
        let raw = raw.as_ref().trim();
        let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() != 18 {
            return Err(SecEdgarError::InvalidAccession(format!(
                "expected 18 digits, got {} from `{raw}`",
                digits.len()
            )));
        }
        // Re-emit canonical hyphenated form.
        let formatted = format!("{}-{}-{}", &digits[..10], &digits[10..12], &digits[12..]);
        Ok(Self(formatted))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// SEC form type — well-known forms enumerated, long tail via [`FormType::Other`].
///
/// Closed enum for the forms apps will actually filter on; `Other`
/// keeps the port forward-compatible without forcing schema churn for
/// every quarterly SEC vocabulary expansion.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FormType {
    /// Annual report — Form 10-K
    #[serde(rename = "10-K")]
    Form10K,
    /// Quarterly report — Form 10-Q
    #[serde(rename = "10-Q")]
    Form10Q,
    /// Current report — Form 8-K
    #[serde(rename = "8-K")]
    Form8K,
    /// Registration statement — Form S-1
    #[serde(rename = "S-1")]
    FormS1,
    /// Beneficial ownership — Form 13F
    #[serde(rename = "13F")]
    Form13F,
    /// Insider transaction — Form 4
    #[serde(rename = "4")]
    Form4,
    /// Foreign private issuer annual — Form 20-F
    #[serde(rename = "20-F")]
    Form20F,
    /// Proxy statement — Form DEF 14A
    #[serde(rename = "DEF 14A")]
    FormDef14A,
    /// Any other SEC form — vocabulary is open-ended; carry the raw label.
    Other(String),
}

impl FormType {
    #[must_use]
    pub fn as_label(&self) -> &str {
        match self {
            Self::Form10K => "10-K",
            Self::Form10Q => "10-Q",
            Self::Form8K => "8-K",
            Self::FormS1 => "S-1",
            Self::Form13F => "13F",
            Self::Form4 => "4",
            Self::Form20F => "20-F",
            Self::FormDef14A => "DEF 14A",
            Self::Other(label) => label,
        }
    }

    /// Map an SEC label to a [`FormType`]. Unknown labels are preserved as [`FormType::Other`].
    pub fn from_label(label: impl AsRef<str>) -> Self {
        match label.as_ref() {
            "10-K" => Self::Form10K,
            "10-Q" => Self::Form10Q,
            "8-K" => Self::Form8K,
            "S-1" => Self::FormS1,
            "13F" => Self::Form13F,
            "4" => Self::Form4,
            "20-F" => Self::Form20F,
            "DEF 14A" => Self::FormDef14A,
            other => Self::Other(other.to_string()),
        }
    }
}

/// A named section inside a filing (e.g., "1A" for Risk Factors).
///
/// `id` is the SEC item identifier (kept as-is from the filing index
/// — `"1A"`, `"7"`, `"8"`, etc.). `title` is the human-readable
/// heading as it appears in the filing. `body` is the raw text/HTML
/// content; downstream extractors decide how to parse it further.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilingSection {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// A single SEC filing — the typed source domain object.
///
/// Sections are stored in a [`BTreeMap`] keyed by section id so
/// serialization is stable across runs (audit-friendly).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filing {
    pub cik: Cik,
    pub accession_number: AccessionNumber,
    pub form_type: FormType,
    /// ISO-8601 date of filing (kept as a string; the port does not
    /// parse to a Date type because consumers vary on timezone
    /// handling — some want UTC, some need the filer's local).
    pub filed_at: String,
    pub sections: BTreeMap<String, FilingSection>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cik_pads_to_ten_digits() {
        // Intent: every CIK in this workspace must compare as a
        // canonical 10-digit string. If a caller passes raw digits
        // (`320193`) the port must accept and normalize — otherwise
        // every downstream lookup keyed on CIK could miss a record.
        assert_eq!(Cik::parse("320193").unwrap().as_str(), "0000320193");
        assert_eq!(Cik::parse("0000320193").unwrap().as_str(), "0000320193");
    }

    #[test]
    fn cik_rejects_non_digits() {
        // Intent: silent normalization of garbage input would leak
        // bogus CIKs into the audit trail. Reject loudly at the port
        // boundary.
        assert!(Cik::parse("abc").is_err());
        assert!(Cik::parse("320-193").is_err());
        assert!(Cik::parse("").is_err());
    }

    #[test]
    fn cik_rejects_overlong_input() {
        // Intent: SEC's CIK space tops out at 10 digits. Anything
        // longer is corrupt; refuse rather than truncate.
        assert!(Cik::parse("12345678901").is_err());
    }

    #[test]
    fn accession_number_normalizes_to_hyphenated() {
        // Intent: hyphenated and unhyphenated forms must both parse to
        // the same canonical string. Same audit-trail argument as
        // CIK normalization.
        let canonical = "0000320193-23-000106";
        assert_eq!(
            AccessionNumber::parse("0000320193-23-000106")
                .unwrap()
                .as_str(),
            canonical
        );
        assert_eq!(
            AccessionNumber::parse("000032019323000106")
                .unwrap()
                .as_str(),
            canonical
        );
    }

    #[test]
    fn accession_number_rejects_wrong_digit_count() {
        assert!(AccessionNumber::parse("000032019323").is_err());
        assert!(AccessionNumber::parse("").is_err());
    }

    #[test]
    fn form_type_round_trips_known_forms_through_label() {
        // Intent: known forms must round-trip via from_label/as_label
        // so filtering ("show me only 10-Ks") works whether the caller
        // built the FormType from an enum variant or from a string.
        for label in ["10-K", "10-Q", "8-K", "S-1", "13F", "4", "20-F", "DEF 14A"] {
            let parsed = FormType::from_label(label);
            assert_eq!(parsed.as_label(), label);
            // Must NOT degrade to Other for known forms.
            assert!(
                !matches!(parsed, FormType::Other(_)),
                "known form `{label}` parsed as Other — vocabulary regression"
            );
        }
    }

    #[test]
    fn form_type_preserves_unknown_labels_via_other() {
        // Intent: the SEC vocabulary grows quarterly. Unknown forms
        // must NOT panic and must NOT lose information — the raw
        // label has to round-trip.
        let raw = "144-Z-NEW-FORM";
        let parsed = FormType::from_label(raw);
        match &parsed {
            FormType::Other(label) => assert_eq!(label, raw),
            other => panic!("expected Other, got {other:?}"),
        }
        assert_eq!(parsed.as_label(), raw);
    }

    #[test]
    fn filing_serde_round_trip_preserves_section_order() {
        // Intent: serialization must be stable across runs so audit
        // diffs are meaningful. BTreeMap ordering is the load-bearing
        // contract here — if it ever degrades to HashMap, two
        // identical filings could serialize differently and trigger
        // spurious "changed" signals in downstream drift detection.
        let mut sections = BTreeMap::new();
        sections.insert(
            "1".to_string(),
            FilingSection {
                id: "1".into(),
                title: "Business".into(),
                body: "...".into(),
            },
        );
        sections.insert(
            "1A".to_string(),
            FilingSection {
                id: "1A".into(),
                title: "Risk Factors".into(),
                body: "...".into(),
            },
        );

        let filing = Filing {
            cik: Cik::parse("320193").unwrap(),
            accession_number: AccessionNumber::parse("0000320193-23-000106").unwrap(),
            form_type: FormType::Form10K,
            filed_at: "2023-11-03".into(),
            sections,
        };

        let json_a = serde_json::to_string(&filing).unwrap();
        let json_b = serde_json::to_string(&filing).unwrap();
        assert_eq!(
            json_a, json_b,
            "two serializations of the same filing must produce identical JSON"
        );

        let back: Filing = serde_json::from_str(&json_a).unwrap();
        assert_eq!(back, filing);
    }
}
