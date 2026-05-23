// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! TED port — Tenders Electronic Daily.
//!
//! TED is the official journal of EU public procurement: every
//! procurement above a regulated threshold from any EU member state
//! (including national-level Swedish, German, French notices) is
//! published here. One source, EU-wide coverage.
//!
//! Embassy port: evidence-only. Notices here say *what was published*;
//! decisions to bid, partner, or build pursuit pipelines belong in the
//! Commercial Rail layer.
//!
//! ## Source
//!
//! Real provider: `https://ted.europa.eu/api/v3.0/notices/search`
//! (no auth, but rate-limited; data also available as bulk XML).
//! Live integration deferred.
//!
//! ## Status
//!
//! Both stub and live providers ship. The `live` feature adds
//! [`LiveTedProvider`] which calls the TED v3 search endpoint and
//! follows the cursor-paginated `iterationNextToken` chain via
//! [`manifold::pagination::paginate`] until the caller's `limit` is
//! met. No auth required; rate limit is per-IP. The search endpoint
//! URL is configurable via `live::LiveFetchOptions::search_url` since
//! TED has moved the path across releases.

mod error;
#[cfg(feature = "live")]
pub mod live;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::TedError;
#[cfg(feature = "live")]
pub use live::LiveTedProvider;
pub use provenance::{TED_PROVENANCE, Ted};
pub use provider::{StubTedProvider, TedProvider, TedRequest, TedResponse};
pub use suggestor::{TedLookupSuggestor, TedNoticePayload};
pub use types::{ProcurementNotice, ProcurementType, TedNoticeId};
