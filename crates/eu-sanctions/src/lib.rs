// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! EU Sanctions port — EU Consolidated Financial Sanctions List.
//!
//! Embassy port: evidence-only. A hit produced here is *information
//! about* an EU sanctions exposure. Decisions to freeze, refuse, or
//! report belong in the Commercial Rail layer.
//!
//! ## Sanctions trio
//!
//! Shares the on-the-wire [`SanctionsHit`] shape with `embassy-ofac-sls`
//! and `embassy-commerce-csl` so a Formation can fold hits from all
//! three jurisdictions into one screening decision.
//!
//! ## Source
//!
//! The EU publishes the consolidated list as XML at
//! `https://webgate.ec.europa.eu/fsd/fsf` (requires registration for
//! the authenticated channel; an unauthenticated XML download is also
//! published). Live integration deferred.
//!
//! ## Status
//!
//! Both stub and live providers ship. Default-features builds get the
//! typed domain + [`StubEuSanctionsProvider`]. Building with
//! `--features live` adds [`LiveEuSanctionsProvider`] which fetches
//! the OpenSanctions mirror of the EU FSF (CC-BY 4.0, no auth) —
//! consumers with EU-Login credentials can point at the canonical
//! source via `LiveFetchOptions::sanctions_url`.

mod error;
#[cfg(feature = "live")]
pub mod live;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::EuSanctionsError;
#[cfg(feature = "live")]
pub use live::LiveEuSanctionsProvider;
pub use provenance::{EU_SANCTIONS_PROVENANCE, EuSanctions};
pub use provider::{
    EuSanctionsProvider, EuSanctionsRequest, EuSanctionsResponse, StubEuSanctionsProvider,
};
pub use suggestor::{EuSanctionsHitPayload, EuSanctionsLookupSuggestor};
pub use types::{MatchType, SanctionsHit, SanctionsSubject, SubjectType};
