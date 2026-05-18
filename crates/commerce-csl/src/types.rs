// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Sanctions screening typed domain — Commerce CSL flavour.
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
    fn hit_shape_matches_sister_ports() {
        // Intent: all three sanctions ports (ofac-sls, eu-sanctions,
        // commerce-csl) share the same SanctionsHit and SanctionsSubject
        // types from embassy-pack. This test confirms the shared type is
        // importable here and that a SanctionsHit can be constructed and
        // serialized — the structural uniformity guarantee that lets
        // Formations fold hits from multiple sources into one decision.
        let hit = SanctionsHit {
            subject_name: "BIS Listed Entity".into(),
            match_score: 0.95,
            match_type: MatchType::Exact,
            subject_type: SubjectType::Entity,
            list_name: "Commerce CSL".into(),
            list_program: Some("BIS Entity List".into()),
            listed_at: Some("2024-04-01".into()),
            aliases: vec![],
            jurisdictions: vec!["US".into()],
        };
        // Confirm round-trip serialization works — the serde key set is
        // guaranteed identical across the trio because it is the same type.
        let json = serde_json::to_string(&hit).unwrap();
        let back: SanctionsHit = serde_json::from_str(&json).unwrap();
        assert_eq!(back, hit);
    }
}
