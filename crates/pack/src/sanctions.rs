// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Shared sanctions screening types used across the sanctions trio
//! (ofac-sls, eu-sanctions, commerce-csl).
//!
//! The shape ([`SanctionsSubject`] in, [`SanctionsHit`] out) is shared
//! across the three ports. Keep the field names identical — Formations
//! and downstream consumers rely on structural uniformity to fold hits
//! from multiple sources into one decision.

use serde::{Deserialize, Serialize};

/// What we ask the screening list about — a name, optionally narrowed
/// by country code. Country is ISO 3166-1 alpha-2 when present.
///
/// The port is intentionally permissive on the input: sanctions
/// screening accepts free-form names because match logic on the live
/// side is fuzzy. Strict validation belongs at the Commercial Rail
/// layer that decides what to screen and why.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SanctionsSubject {
    pub name: String,
    pub country: Option<String>,
}

impl SanctionsSubject {
    /// Parse a subject name, trimming whitespace. Returns `Err` with a
    /// description string if the name is empty after trimming.
    ///
    /// Returns `Result<Self, String>` so the shared type is not bound to
    /// any one port's error enum; each port wraps the error if needed.
    pub fn parse(name: impl AsRef<str>) -> Result<Self, String> {
        let name = name.as_ref().trim().to_string();
        if name.is_empty() {
            return Err("empty name".into());
        }
        Ok(Self {
            name,
            country: None,
        })
    }

    #[must_use]
    pub fn with_country(mut self, country: impl Into<String>) -> Self {
        self.country = Some(country.into());
        self
    }
}

/// How a hit matched the queried subject. Distinct from match score
/// because the *kind* of match matters for compliance review
/// independent of confidence — an exact name match at 0.99 is a
/// different review-burden than an alias match at 0.99.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    Exact,
    Fuzzy,
    Alias,
}

/// What kind of entity is on the list. Drives downstream review:
/// vessel/aircraft listings are operationally different from individual
/// or corporate entity listings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectType {
    Individual,
    Entity,
    Vessel,
    Aircraft,
    Unknown,
}

/// One hit on a sanctions screening list.
///
/// Field shape is shared across the sanctions trio so a Formation can
/// fold hits from all three sources into one decision view. Per-source
/// quirks (OFAC program codes, EU regulation references, BIS list
/// segments) live in the optional `list_program` slot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SanctionsHit {
    /// The name as it appears on the source list (not the queried
    /// name). May differ from the query when matched by alias or
    /// fuzzy logic.
    pub subject_name: String,
    /// Match confidence in `[0.0, 1.0]`. The live provider sets this;
    /// the stub returns deterministic values per input.
    pub match_score: f32,
    pub match_type: MatchType,
    pub subject_type: SubjectType,
    /// Canonical list identifier (e.g., "OFAC SDN", "OFAC NS-PLC").
    pub list_name: String,
    /// Program / regulation code within the list, when applicable
    /// (e.g., "SDGT", "CUBA", "UKRAINE-EO13662").
    pub list_program: Option<String>,
    /// ISO 8601 date the listing was added or last updated.
    pub listed_at: Option<String>,
    pub aliases: Vec<String>,
    pub jurisdictions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_rejects_empty_name() {
        // Intent: empty-name screening would match every record on the
        // live side and flood downstream review. Refuse at the boundary.
        assert!(SanctionsSubject::parse("").is_err());
        assert!(SanctionsSubject::parse("   ").is_err());
    }

    #[test]
    fn subject_trims_whitespace() {
        // Intent: the same name with stray whitespace should canonicalize
        // to the same subject, so two sightings converge.
        let a = SanctionsSubject::parse("  Volvo  ").unwrap();
        let b = SanctionsSubject::parse("Volvo").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn hit_serde_round_trips() {
        // Intent: a hit must round-trip through serde exactly so a
        // stored record can be byte-compared against a re-fetch for
        // change detection. Sanctions list deltas trigger compliance
        // re-review; missing a delta because of a serde quirk is a
        // material defect.
        let hit = SanctionsHit {
            subject_name: "Specially Designated Individual".into(),
            match_score: 0.97,
            match_type: MatchType::Fuzzy,
            subject_type: SubjectType::Individual,
            list_name: "OFAC SDN".into(),
            list_program: Some("SDGT".into()),
            listed_at: Some("2024-03-01".into()),
            aliases: vec!["SDI".into()],
            jurisdictions: vec!["US".into()],
        };
        let json = serde_json::to_string(&hit).unwrap();
        let back: SanctionsHit = serde_json::from_str(&json).unwrap();
        assert_eq!(back, hit);
    }
}
