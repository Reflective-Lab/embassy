// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the Crunchbase port port.
#[derive(Copy, Clone, Debug)]
pub struct Crunchbase;

impl ProvenanceSource for Crunchbase {
    fn as_str(&self) -> &'static str {
        "crunchbase"
    }
}

pub const CRUNCHBASE_PROVENANCE: Crunchbase = Crunchbase;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical provenance string. If a future refactor changes
        // this constant, every log-search that scopes
        // `provenance="crunchbase"` silently misses new facts.
        assert_eq!(CRUNCHBASE_PROVENANCE.as_str(), "crunchbase");
    }
}
