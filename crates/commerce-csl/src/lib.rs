// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Commerce CSL port — US Department of Commerce Consolidated Screening
//! List.
//!
//! Aggregates the BIS Entity List, Denied Persons List, Unverified
//! List, and other Commerce / State / Treasury sub-lists that the
//! Commerce Department surfaces through a single API.
//!
//! Embassy port: evidence-only. Decisions to license, refuse, or
//! report belong in the Commercial Rail layer.
//!
//! ## Sanctions trio
//!
//! Shares the on-the-wire [`SanctionsHit`] shape with `embassy-ofac-sls`
//! and `embassy-eu-sanctions` for cross-source folding.
//!
//! ## Source
//!
//! Real provider hits `https://api.trade.gov/consolidated_screening_list/search`
//! (requires API key). Live integration deferred.
//!
//! ## Status
//!
//! Both stub and live providers ship. The `live` feature pulls
//! [`LiveCommerceCslProvider`] which fetches the OpenSanctions mirror
//! of the CSL (CC-BY 4.0, no auth); consumers with a trade.gov
//! api_key can point at the canonical endpoint via
//! `LiveFetchOptions::sanctions_url`.

mod error;
#[cfg(feature = "live")]
pub mod live;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::CommerceCslError;
#[cfg(feature = "live")]
pub use live::LiveCommerceCslProvider;
pub use provenance::{COMMERCE_CSL_PROVENANCE, CommerceCsl};
pub use provider::{
    CommerceCslProvider, CommerceCslRequest, CommerceCslResponse, StubCommerceCslProvider,
};
pub use suggestor::{CommerceCslHitPayload, CommerceCslLookupSuggestor};
pub use types::{MatchType, SanctionsHit, SanctionsSubject, SubjectType};
