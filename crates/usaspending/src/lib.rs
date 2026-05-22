// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! USAspending port — US federal award + obligation data.
//!
//! USAspending.gov is the federal spend transparency endpoint: every
//! federal contract, grant, loan, and direct-payment award gets a
//! record there. The port surfaces individual award records and
//! aggregate spend queries.
//!
//! Embassy port: evidence-only. The records here say *what was
//! awarded*. Decisions to bid, partner, or model competitor exposure
//! belong in the Commercial Rail layer.
//!
//! Pairs with `embassy-sam-gov`: SAM = who is registered to receive
//! awards; USAspending = what awards have actually been made.
//!
//! ## Source
//!
//! Real provider: `https://api.usaspending.gov/api/v2/` (no auth
//! required, but rate-limited). Live integration deferred.
//!
//! ## Status
//!
//! Both stub and live providers ship. Default-features builds get the
//! typed domain + [`StubUsaspendingProvider`]. Building with
//! `--features live` adds [`LiveUsaspendingProvider`] which calls
//! `https://api.usaspending.gov/api/v2/awards/{award_id}/` — no auth.

mod error;
#[cfg(feature = "live")]
pub mod live;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::UsaspendingError;
#[cfg(feature = "live")]
pub use live::LiveUsaspendingProvider;
pub use provenance::{USASPENDING_PROVENANCE, Usaspending};
pub use provider::{
    StubUsaspendingProvider, UsaspendingProvider, UsaspendingRequest, UsaspendingResponse,
};
pub use suggestor::{UsaspendingAwardPayload, UsaspendingLookupSuggestor};
pub use types::{AwardId, FederalAward};
