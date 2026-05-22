// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! OFAC SLS port — US Treasury Office of Foreign Assets Control
//! Sanctions List Search.
//!
//! Embassy port: evidence-only. A hit produced here is *information
//! about* a sanctions exposure. The decision to refuse a transaction,
//! file a report, or freeze an asset belongs in the Commercial Rail
//! layer above — not here.
//!
//! ## Sanctions trio
//!
//! This port is one of three sanctions-screening ports that share a
//! coherent on-the-wire shape ([`SanctionsHit`]) so Formations can
//! consume hits from any of them uniformly:
//!
//! - `embassy-ofac-sls` — US Treasury (this port)
//! - `embassy-eu-sanctions` — EU Consolidated Financial Sanctions List
//! - `embassy-commerce-csl` — US Commerce BIS Consolidated Screening List
//!
//! The three answer the same business question ("can I do business
//! with this counterparty?") from three jurisdictions. A Formation
//! that needs full coverage subscribes to all three.
//!
//! ## Status
//!
//! Both stub and live providers ship. Default-features builds get the
//! typed domain + [`StubOfacSlsProvider`]. Building with `--features
//! live` adds [`LiveOfacSlsProvider`] which downloads the canonical
//! OFAC SDN CSV from US Treasury (no auth required) and screens
//! submitted subjects by exact / substring name match.

mod error;
#[cfg(feature = "live")]
pub mod live;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::OfacSlsError;
#[cfg(feature = "live")]
pub use live::LiveOfacSlsProvider;
pub use provenance::{OFAC_SLS_PROVENANCE, OfacSls};
pub use provider::{OfacSlsProvider, OfacSlsRequest, OfacSlsResponse, StubOfacSlsProvider};
pub use suggestor::{OfacSlsHitPayload, OfacSlsLookupSuggestor};
pub use types::{MatchType, SanctionsHit, SanctionsSubject, SubjectType};
