// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Sanctions screening typed domain — EU consolidated list flavour.
//!
//! The shape ([`SanctionsSubject`] in, [`SanctionsHit`] out) is shared
//! verbatim across the sanctions trio (ofac-sls, eu-sanctions,
//! commerce-csl). The field names are identical across the three so
//! Formations can fold hits from any source into one decision view.

use serde::{Deserialize, Serialize};

use crate::error::EuSanctionsError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SanctionsSubject {
    pub name: String,
    pub country: Option<String>,
}

impl SanctionsSubject {
    pub fn parse(name: impl AsRef<str>) -> Result<Self, EuSanctionsError> {
        let name = name.as_ref().trim().to_string();
        if name.is_empty() {
            return Err(EuSanctionsError::InvalidSubject("empty name".into()));
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchType {
    Exact,
    Fuzzy,
    Alias,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectType {
    Individual,
    Entity,
    Vessel,
    Aircraft,
    Unknown,
}

/// One hit on the EU Consolidated Financial Sanctions List.
///
/// Shape is identical to the sister sanctions ports so a Formation can
/// merge hits across sources without per-source field mapping. The EU
/// regulation reference (e.g., "(EU) 2014/145") goes in `list_program`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SanctionsHit {
    pub subject_name: String,
    pub match_score: f32,
    pub match_type: MatchType,
    pub subject_type: SubjectType,
    pub list_name: String,
    pub list_program: Option<String>,
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
    fn hit_shape_matches_sister_ports() {
        // Intent: serde key set on a SanctionsHit must be the same
        // across the sanctions trio. A Formation that folds hits from
        // multiple ports relies on the key set being identical.
        // Touching the key set here without touching ofac-sls and
        // commerce-csl in the same patch breaks that contract.
        let hit = SanctionsHit {
            subject_name: "Sanctioned Individual".into(),
            match_score: 0.98,
            match_type: MatchType::Exact,
            subject_type: SubjectType::Individual,
            list_name: "EU Consolidated".into(),
            list_program: Some("(EU) 2014/145".into()),
            listed_at: Some("2024-02-01".into()),
            aliases: vec!["SI".into()],
            jurisdictions: vec!["EU".into()],
        };
        let json = serde_json::to_value(&hit).unwrap();
        let obj = json.as_object().unwrap();
        for required in [
            "subject_name",
            "match_score",
            "match_type",
            "subject_type",
            "list_name",
            "list_program",
            "listed_at",
            "aliases",
            "jurisdictions",
        ] {
            assert!(
                obj.contains_key(required),
                "sanctions-trio key `{required}` must be present"
            );
        }
    }
}
