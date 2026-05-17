// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Typed USAspending domain — award identifier + award record.

use serde::{Deserialize, Serialize};

use crate::error::UsaspendingError;

/// USAspending award identifier. USAspending uses opaque strings
/// (e.g., `"CONT_AWD_..."` for contracts, `"ASST_NON_..."` for
/// assistance awards) — the port does not parse the internal
/// structure, only validates non-empty + ASCII printable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AwardId(String);

impl AwardId {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, UsaspendingError> {
        let raw = raw.as_ref().trim().to_string();
        if raw.is_empty() {
            return Err(UsaspendingError::InvalidAwardId("empty".into()));
        }
        if !raw.chars().all(|c| c.is_ascii_graphic()) {
            return Err(UsaspendingError::InvalidAwardId(format!(
                "non-printable characters: `{raw}`"
            )));
        }
        Ok(Self(raw))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One federal award record.
///
/// `total_obligated_micros` holds the obligated amount in *micros of
/// USD* (1 USD = 1_000_000 micros) to avoid floating-point drift on
/// audit comparisons. The live provider exposes a decimal string;
/// the port parses to micros at the boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederalAward {
    pub award_id: AwardId,
    pub recipient_name: String,
    /// Total obligated amount in micros of USD. `1_000_000` micros =
    /// 1 USD. Storing micros keeps audit equality exact under
    /// roundtrip.
    pub total_obligated_micros: i64,
    pub awarding_agency: String,
    /// ISO 8601 date the award period of performance starts.
    pub period_of_performance_start: Option<String>,
    pub period_of_performance_end: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn award_id_rejects_empty() {
        // Intent: empty IDs at the boundary would let garbage into the
        // audit trail.
        assert!(AwardId::parse("").is_err());
        assert!(AwardId::parse("   ").is_err());
    }

    #[test]
    fn award_id_rejects_non_printable() {
        // Intent: control characters in an award ID mean corrupt input.
        let bad = "ID\u{0007}WITH-BELL";
        assert!(AwardId::parse(bad).is_err());
    }

    #[test]
    fn award_serde_round_trips() {
        // Intent: micros-as-i64 must round-trip byte-for-byte. If a
        // refactor ever switches to f64 USD, two stored values for the
        // same award could diverge under reserialize — and the audit
        // log loses change-detection.
        let award = FederalAward {
            award_id: AwardId::parse("CONT_AWD_STUB_001").unwrap(),
            recipient_name: "Stub Federal Contractor LLC".into(),
            total_obligated_micros: 1_234_567_890_000,
            awarding_agency: "Department of Stub".into(),
            period_of_performance_start: Some("2026-01-01".into()),
            period_of_performance_end: Some("2027-01-01".into()),
        };
        let json = serde_json::to_string(&award).unwrap();
        let back: FederalAward = serde_json::from_str(&json).unwrap();
        assert_eq!(back, award);
    }
}
