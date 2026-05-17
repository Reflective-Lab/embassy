// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use converge_pack::ProvenanceSource;

#[derive(Copy, Clone, Debug)]
pub struct Bolagsverket;

impl ProvenanceSource for Bolagsverket {
    fn as_str(&self) -> &'static str {
        "bolagsverket"
    }
}

pub const BOLAGSVERKET_PROVENANCE: Bolagsverket = Bolagsverket;
