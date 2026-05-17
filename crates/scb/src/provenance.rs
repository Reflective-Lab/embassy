// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the Statistics Sweden (SCB) port port.
#[derive(Copy, Clone, Debug)]
pub struct Scb;

impl ProvenanceSource for Scb {
    fn as_str(&self) -> &'static str {
        "scb"
    }
}

pub const SCB_PROVENANCE: Scb = Scb;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical provenance string. If a future refactor changes
        // this constant, every log-search that scopes
        // `provenance="scb"` silently misses new facts.
        assert_eq!(SCB_PROVENANCE.as_str(), "scb");
    }
}
