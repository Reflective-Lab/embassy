// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the USAspending
/// port.
#[derive(Copy, Clone, Debug)]
pub struct Usaspending;

impl ProvenanceSource for Usaspending {
    fn as_str(&self) -> &'static str {
        "usaspending"
    }
}

pub const USASPENDING_PROVENANCE: Usaspending = Usaspending;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        assert_eq!(USASPENDING_PROVENANCE.as_str(), "usaspending");
    }
}
