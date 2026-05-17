// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the GitHub port port.
#[derive(Copy, Clone, Debug)]
pub struct Github;

impl ProvenanceSource for Github {
    fn as_str(&self) -> &'static str {
        "github"
    }
}

pub const GITHUB_PROVENANCE: Github = Github;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical provenance string. If a future refactor changes
        // this constant, every log-search that scopes
        // `provenance="github"` silently misses new facts.
        assert_eq!(GITHUB_PROVENANCE.as_str(), "github");
    }
}
