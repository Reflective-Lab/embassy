// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the SAM.gov port.
#[derive(Copy, Clone, Debug)]
pub struct SamGov;

impl ProvenanceSource for SamGov {
    fn as_str(&self) -> &'static str {
        "sam_gov"
    }
}

pub const SAM_GOV_PROVENANCE: SamGov = SamGov;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        assert_eq!(SAM_GOV_PROVENANCE.as_str(), "sam_gov");
    }
}
