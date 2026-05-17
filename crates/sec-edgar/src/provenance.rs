// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the SEC EDGAR port.
///
/// Use [`SEC_EDGAR_PROVENANCE`] as the marker on every fact produced
/// here so audit log queries can filter "everything that came from
/// EDGAR" with a single `provenance="sec-edgar"` clause.
#[derive(Copy, Clone, Debug)]
pub struct SecEdgar;

impl ProvenanceSource for SecEdgar {
    fn as_str(&self) -> &'static str {
        "sec-edgar"
    }
}

/// Canonical provenance const — pass to `Suggestor::provenance()` and
/// `ProposedFact::new(..., SEC_EDGAR_PROVENANCE.as_str())`.
pub const SEC_EDGAR_PROVENANCE: SecEdgar = SecEdgar;
