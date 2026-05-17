// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! SEC EDGAR port — US Securities and Exchange Commission filings.
//!
//! Sovereign integration in the embassy sense: EDGAR's contract is
//! *part of the API surface*, not implementation detail. Specifically:
//!
//! - **User-Agent required** — anonymous requests are blocked; the SEC
//!   requires a recognizable Reflective Labs research UA with an
//!   email contact.
//! - **Rate limit 10 req/sec** workspace-wide. Live providers must
//!   throttle; bursty traffic from one app affects every other.
//! - **CIK normalization** — Central Index Keys are 10-digit
//!   zero-padded; `0000320193` is Apple, not `320193`. The port
//!   normalizes on input.
//! - **Form types are a closed vocabulary** — 10-K, 10-Q, 8-K, S-1,
//!   etc. — kept open via `FormType::Other(String)` for the long tail.
//!
//! This crate ships the typed source domain (CIK, AccessionNumber,
//! FormType, Filing). App-specific synthesis on top (drift signals,
//! language analysis, multi-tenant aggregation) stays in the
//! consuming app.
//!
//! Today the port ships:
//! - `StubSecEdgarProvider` — deterministic, no network, for unit tests
//!   and CI
//!
//! Live provider deferred behind a feature in a follow-on release; the
//! contract here is the stable surface every implementation will
//! satisfy.

mod error;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::SecEdgarError;
pub use provenance::{SEC_EDGAR_PROVENANCE, SecEdgar};
pub use provider::{SecEdgarProvider, SecEdgarRequest, SecEdgarResponse, StubSecEdgarProvider};
pub use suggestor::{SecFilingPayload, SecFilingSuggestor};
pub use types::{AccessionNumber, Cik, Filing, FilingSection, FormType};
