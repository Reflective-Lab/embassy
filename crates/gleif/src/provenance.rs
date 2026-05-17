// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

/// Canonical provenance marker for facts emitted by the GLEIF port.
///
/// Audit-log queries scoped to `provenance="gleif"` should hit every
/// LEI observation, ownership relation, and search hit produced here.
#[derive(Copy, Clone, Debug)]
pub struct Gleif;

impl ProvenanceSource for Gleif {
    fn as_str(&self) -> &'static str {
        "gleif"
    }
}

pub const GLEIF_PROVENANCE: Gleif = Gleif;
