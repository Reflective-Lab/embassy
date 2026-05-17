// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the EU Sanctions
/// port.
#[derive(Copy, Clone, Debug)]
pub struct EuSanctions;

impl ProvenanceSource for EuSanctions {
    fn as_str(&self) -> &'static str {
        "eu_sanctions"
    }
}

pub const EU_SANCTIONS_PROVENANCE: EuSanctions = EuSanctions;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: audit-log queries scoped to provenance="eu_sanctions"
        // must continue to hit every observation produced here.
        assert_eq!(EU_SANCTIONS_PROVENANCE.as_str(), "eu_sanctions");
    }
}
