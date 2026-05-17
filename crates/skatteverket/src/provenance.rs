// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the Skatteverket
/// port.
///
/// Tax-administrative status is regulatory-adjacent — mis-tagging here
/// breaks the audit story for any downstream counterparty-diligence
/// decision.
#[derive(Copy, Clone, Debug)]
pub struct Skatteverket;

impl ProvenanceSource for Skatteverket {
    fn as_str(&self) -> &'static str {
        "skatteverket"
    }
}

pub const SKATTEVERKET_PROVENANCE: Skatteverket = Skatteverket;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        assert_eq!(SKATTEVERKET_PROVENANCE.as_str(), "skatteverket");
    }
}
