// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the European Patent Office (OPS) port port.
#[derive(Copy, Clone, Debug)]
pub struct Epo;

impl ProvenanceSource for Epo {
    fn as_str(&self) -> &'static str {
        "epo"
    }
}

pub const EPO_PROVENANCE: Epo = Epo;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical provenance string. If a future refactor changes
        // this constant, every log-search that scopes
        // `provenance="epo"` silently misses new facts.
        assert_eq!(EPO_PROVENANCE.as_str(), "epo");
    }
}
