// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Minimal typed domain — placeholder shapes for the skeleton.
//!
//! Expand when an app pulls. The headline identifier is locked in so
//! downstream consumers can reference the canonical type without
//! committing to a richer entity shape they don't need yet.

use serde::{Deserialize, Serialize};

use crate::error::GithubError;

embassy_pack::simple_id!(
    /// The canonical identifier for this port's domain.
    ///
    /// GitHub organization slug format: `[a-zA-Z0-9-]+` with no leading hyphen,
    /// no trailing hyphen, and no consecutive hyphens (`--`).
    /// Examples: `rust-lang`, `microsoft`, `anthropics`.
    OrgSlug, GithubError,
    |s: &str| {
        if s.starts_with('-') || s.ends_with('-') {
            return Err(GithubError::InvalidIdentifier(
                "invalid OrgSlug: must not start or end with a hyphen".into(),
            ));
        }
        if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(GithubError::InvalidIdentifier(
                "invalid OrgSlug: only ASCII letters, digits, and hyphens are allowed".into(),
            ));
        }
        if s.contains("--") {
            return Err(GithubError::InvalidIdentifier(
                "invalid OrgSlug: consecutive hyphens ('--') are not allowed".into(),
            ));
        }
        Ok(s.to_string())
    }
);

/// Placeholder typed entity. Replace with real per-service fields when
/// an app needs them; the `login` + `html_url` pair is
/// intentionally minimal so the surface compiles without committing
/// to a fuller schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Organization {
    pub login: OrgSlug,
    pub html_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rejects_empty() {
        // Intent: every port rejects empty IDs at the boundary —
        // catching this here keeps garbage out of the audit log.
        assert!(OrgSlug::parse("").is_err());
        assert!(OrgSlug::parse("   ").is_err());
    }

    #[test]
    fn valid_org_slugs_parse() {
        // Intent: slugs that match GitHub's naming rules must be accepted
        // so the port can look up real organizations without false rejections.
        assert_eq!(OrgSlug::parse("rust-lang").unwrap().as_str(), "rust-lang");
        assert_eq!(OrgSlug::parse("microsoft").unwrap().as_str(), "microsoft");
        assert_eq!(OrgSlug::parse("org123").unwrap().as_str(), "org123");
    }

    #[test]
    fn invalid_org_slugs_rejected() {
        // Intent: slugs that violate GitHub's rules (leading/trailing hyphen,
        // consecutive hyphens, disallowed chars) must be caught at the port
        // boundary before they produce a confusing API error.
        assert!(OrgSlug::parse("-leading").is_err());   // leading hyphen
        assert!(OrgSlug::parse("trailing-").is_err());  // trailing hyphen
        assert!(OrgSlug::parse("double--hyphen").is_err()); // consecutive hyphens
        assert!(OrgSlug::parse("has space").is_err());  // space not allowed
        assert!(OrgSlug::parse("has_underscore").is_err()); // underscore not allowed
    }

    #[test]
    fn entity_serde_round_trips() {
        // Intent: payloads ride serde across the kernel boundary;
        // a regression would block Formation composition.
        let e = Organization {
            login: OrgSlug::parse("rust-lang").unwrap(),
            html_url: "https://github.com/rust-lang".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: Organization = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
