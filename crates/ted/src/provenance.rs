// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the TED port.
#[derive(Copy, Clone, Debug)]
pub struct Ted;

impl ProvenanceSource for Ted {
    fn as_str(&self) -> &'static str {
        "ted"
    }
}

pub const TED_PROVENANCE: Ted = Ted;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        assert_eq!(TED_PROVENANCE.as_str(), "ted");
    }
}
