// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the UK Companies House port port.
#[derive(Copy, Clone, Debug)]
pub struct CompaniesHouse;

impl ProvenanceSource for CompaniesHouse {
    fn as_str(&self) -> &'static str {
        "companies-house"
    }
}

pub const COMPANIES_HOUSE_PROVENANCE: CompaniesHouse = CompaniesHouse;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical provenance string. If a future refactor changes
        // this constant, every log-search that scopes
        // `provenance="companies-house"` silently misses new facts.
        assert_eq!(COMPANIES_HOUSE_PROVENANCE.as_str(), "companies-house");
    }
}
