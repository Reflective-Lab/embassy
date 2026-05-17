// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Bolagsverket port — the Swedish Companies Registration Office.
//!
//! Sovereign integration: Bolagsverket runs Näringslivsregistret (the
//! authoritative business register) and the API contract is part of the
//! port surface. Specifically:
//!
//! - **OrgNumber format** — Swedish organisation numbers are exactly 10
//!   digits in `NNNNNN-NNNN` form with a Luhn-mod-10 check digit. The
//!   port validates the check digit on construction so bogus IDs
//!   never reach the audit trail. (12-digit personnummer form is
//!   accepted on input but normalized to 10 by stripping the century
//!   prefix.)
//! - **Legal form vocabulary** — Aktiebolag (AB), Handelsbolag (HB),
//!   Kommanditbolag (KB), Enskild näringsidkare (EF), Ekonomisk
//!   förening (EkF), Stiftelse (Foundation), Ideell förening
//!   (NonProfit), etc. — closed enum with `Other(String)` for the long
//!   tail.
//! - **Authoritative vs enriched** — this port targets Bolagsverket's
//!   own APIs (Näringslivsregistret, Foretagsapi). The richer
//!   commercial aggregators (Allabolag, BvD/Orbis) are separate ports
//!   that downstream apps compose on top when they need enrichment
//!   beyond the public registry.
//!
//! Today the port ships:
//! - `StubBolagsverketProvider` — deterministic, no network, for unit
//!   tests and CI loops.
//!
//! Live provider deferred behind a future feature flag.

mod error;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::BolagsverketError;
pub use provenance::{BOLAGSVERKET_PROVENANCE, Bolagsverket};
pub use provider::{
    BolagsverketProvider, BolagsverketRequest, BolagsverketResponse, StubBolagsverketProvider,
};
pub use suggestor::{BolagsverketCompanyPayload, BolagsverketLookupSuggestor};
pub use types::{Address, Company, CompanyStatus, LegalForm, OrgNumber};
