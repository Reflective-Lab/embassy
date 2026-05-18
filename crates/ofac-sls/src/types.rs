// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Sanctions screening typed domain.
//!
//! The shape ([`SanctionsSubject`] in, [`SanctionsHit`] out) and the
//! supporting enums ([`MatchType`], [`SubjectType`]) are defined once in
//! `embassy-pack` and re-exported here so callers importing from this
//! crate continue to work without change.

pub use embassy_pack::{MatchType, SanctionsHit, SanctionsSubject, SubjectType};

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
