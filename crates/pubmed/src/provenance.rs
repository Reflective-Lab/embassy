// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the PubMed / NCBI E-utilities port.
#[derive(Copy, Clone, Debug)]
pub struct Pubmed;

impl ProvenanceSource for Pubmed {
    fn as_str(&self) -> &'static str {
        "pubmed"
    }
}

pub const PUBMED_PROVENANCE: Pubmed = Pubmed;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical provenance string. If a future refactor changes
        // this constant, every log-search that scopes
        // `provenance="pubmed"` silently misses new facts.
        assert_eq!(PUBMED_PROVENANCE.as_str(), "pubmed");
    }
}
