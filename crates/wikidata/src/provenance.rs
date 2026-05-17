// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the Wikidata port port.
#[derive(Copy, Clone, Debug)]
pub struct Wikidata;

impl ProvenanceSource for Wikidata {
    fn as_str(&self) -> &'static str {
        "wikidata"
    }
}

pub const WIKIDATA_PROVENANCE: Wikidata = Wikidata;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical provenance string. If a future refactor changes
        // this constant, every log-search that scopes
        // `provenance="wikidata"` silently misses new facts.
        assert_eq!(WIKIDATA_PROVENANCE.as_str(), "wikidata");
    }
}
