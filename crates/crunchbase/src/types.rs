// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::CrunchbaseError;

/// The canonical identifier for this port's domain.
///
/// Crunchbase organization permalink format: lowercase slug `[a-z0-9-]+` with
/// no leading hyphen, no trailing hyphen, and no consecutive hyphens.
/// Input is normalized to lowercase before validation.
/// Examples: `anthropic`, `openai`, `stripe`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrganizationId(String);

impl OrganizationId {
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, CrunchbaseError> {
        let trimmed = raw.as_ref().trim();
        if trimmed.is_empty() {
            return Err(CrunchbaseError::InvalidIdentifier("empty".into()));
        }
        let s = trimmed.to_lowercase();
        if s.starts_with('-') || s.ends_with('-') {
            return Err(CrunchbaseError::InvalidIdentifier(
                "invalid OrganizationId: must not start or end with a hyphen".into(),
            ));
        }
        if !s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(CrunchbaseError::InvalidIdentifier(
                "invalid OrganizationId: only lowercase letters, digits, and hyphens are allowed".into(),
            ));
        }
        if s.contains("--") {
            return Err(CrunchbaseError::InvalidIdentifier(
                "invalid OrganizationId: consecutive hyphens ('--') are not allowed".into(),
            ));
        }
        Ok(Self(s))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `permalink` + `name` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Organization {
    pub permalink: OrganizationId,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(OrganizationId::parse("").is_err());
        assert!(OrganizationId::parse("   ").is_err());
    }

    #[test]
    fn valid_organization_ids_parse_and_normalize() {
        // Intent: Crunchbase permalinks are canonical lowercase slugs;
        // callers may pass mixed-case input from URLs and it must be
        // normalized rather than rejected, matching Crunchbase's own behavior.
        assert_eq!(OrganizationId::parse("anthropic").unwrap().as_str(), "anthropic");
        assert_eq!(OrganizationId::parse("Anthropic").unwrap().as_str(), "anthropic");
        assert_eq!(OrganizationId::parse("open-ai").unwrap().as_str(), "open-ai");
        assert_eq!(OrganizationId::parse("org123").unwrap().as_str(), "org123");
    }

    #[test]
    fn invalid_organization_ids_rejected() {
        // Intent: slugs violating Crunchbase's URL-slug rules must fail at
        // the boundary; passing them would produce silent 404s from the API.
        assert!(OrganizationId::parse("-leading").is_err());       // leading hyphen
        assert!(OrganizationId::parse("trailing-").is_err());      // trailing hyphen
        assert!(OrganizationId::parse("double--hyphen").is_err()); // consecutive hyphens
        assert!(OrganizationId::parse("has space").is_err());      // space not allowed
        assert!(OrganizationId::parse("has_underscore").is_err()); // underscore not allowed
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Organization {
            permalink: OrganizationId::parse("anthropic").unwrap(),
            name: "Anthropic".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Organization = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
