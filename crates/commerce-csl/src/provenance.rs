// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the Commerce CSL
/// port.
#[derive(Copy, Clone, Debug)]
pub struct CommerceCsl;

impl ProvenanceSource for CommerceCsl {
    fn as_str(&self) -> &'static str {
        "commerce_csl"
    }
}

pub const COMMERCE_CSL_PROVENANCE: CommerceCsl = CommerceCsl;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        assert_eq!(COMMERCE_CSL_PROVENANCE.as_str(), "commerce_csl");
    }
}
