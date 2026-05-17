// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the OFAC SLS port.
///
/// Sanctions audit trails are regulatory artefacts — mis-tagging a hit
/// here (or losing the tag in a refactor) breaks the legal-defensibility
/// story for any downstream decision.
#[derive(Copy, Clone, Debug)]
pub struct OfacSls;

impl ProvenanceSource for OfacSls {
    fn as_str(&self) -> &'static str {
        "ofac_sls"
    }
}

pub const OFAC_SLS_PROVENANCE: OfacSls = OfacSls;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: audit-log queries scoped to provenance="ofac_sls"
        // must continue to hit every sanctions observation produced
        // here. A refactor that changes this string silently breaks
        // the compliance reporting chain.
        assert_eq!(OFAC_SLS_PROVENANCE.as_str(), "ofac_sls");
    }
}
