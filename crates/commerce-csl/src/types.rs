// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Sanctions screening typed domain — Commerce CSL flavour.
//!
//! Shape ([`SanctionsSubject`] in, [`SanctionsHit`] out) is shared
//! verbatim across the sanctions trio. Per-source quirks (the
//! sub-list identifier — Entity List / DPL / UVL) ride in
//! `list_program`.

use serde::{Deserialize, Serialize};

use crate::error::CommerceCslError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SanctionsSubject {
    pub name: String,
    pub country: Option<String>,
}

impl SanctionsSubject {
    pub fn parse(name: impl AsRef<str>) -> Result<Self, CommerceCslError> {
        let name = name.as_ref().trim().to_string();
        if name.is_empty() {
            return Err(CommerceCslError::InvalidSubject("empty name".into()));
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

/// One hit on a Commerce-aggregated screening list. `list_program`
/// carries the originating sub-list (e.g., "BIS Entity List",
/// "Denied Persons List", "Unverified List").
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
        // Intent: serde key set on a SanctionsHit must be identical to
        // ofac-sls and eu-sanctions so a Formation can fold hits
        // across sources.
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
