// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the VIES port.
///
/// Audit-log queries scoped to `provenance="vies"` should hit every
/// VAT validation observation produced here — including the negative
/// results, which carry the same audit weight as the positives.
#[derive(Copy, Clone, Debug)]
pub struct Vies;

impl ProvenanceSource for Vies {
    fn as_str(&self) -> &'static str {
        "vies"
    }
}

pub const VIES_PROVENANCE: Vies = Vies;
