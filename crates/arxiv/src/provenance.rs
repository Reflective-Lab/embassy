// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the arXiv port port.
#[derive(Copy, Clone, Debug)]
pub struct Arxiv;

impl ProvenanceSource for Arxiv {
    fn as_str(&self) -> &'static str {
        "arxiv"
    }
}

pub const ARXIV_PROVENANCE: Arxiv = Arxiv;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical provenance string. If a future refactor changes
        // this constant, every log-search that scopes
        // `provenance="arxiv"` silently misses new facts.
        assert_eq!(ARXIV_PROVENANCE.as_str(), "arxiv");
    }
}
