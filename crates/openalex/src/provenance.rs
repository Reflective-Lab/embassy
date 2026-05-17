// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the OpenAlex port port.
#[derive(Copy, Clone, Debug)]
pub struct OpenAlex;

impl ProvenanceSource for OpenAlex {
    fn as_str(&self) -> &'static str {
        "openalex"
    }
}

pub const OPENALEX_PROVENANCE: OpenAlex = OpenAlex;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical provenance string. If a future refactor changes
        // this constant, every log-search that scopes
        // `provenance="openalex"` silently misses new facts.
        assert_eq!(OPENALEX_PROVENANCE.as_str(), "openalex");
    }
}
