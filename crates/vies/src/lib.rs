// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! VIES port — EU VAT Information Exchange System.
//!
//! VIES is the EU's free, public service for verifying that a VAT
//! registration number is currently active. Every cross-border B2B
//! invoice in the EU is supposed to be backed by a VIES check at the
//! time of issue; the *consultation number* VIES returns is the
//! contemporaneous proof tax authorities accept later.
//!
//! ## Sovereign contract
//!
//! - **VAT number structure** = 2-letter ISO 3166-1 country code +
//!   national VAT number whose format differs per member state. SE is
//!   12 digits; DE is 9 digits; NL is 12 chars including a literal `B`;
//!   etc. The port validates the country code is in the EU member set
//!   and that the national portion is non-empty; **per-country format
//!   regexes are deferred to the live feature** because they evolve
//!   with each member state's tax administration.
//! - **Free, no auth, but per-country availability** — VIES is a
//!   federation of national tax authority databases that the EU SOAP
//!   service multiplexes. Any member state's database may be offline at
//!   any moment, and VIES returns `MS_UNAVAILABLE` rather than a stale
//!   cached answer. The port surfaces that distinction as a typed
//!   status, not as a generic error.
//! - **Consultation number is load-bearing** — when valid, VIES returns
//!   a unique consultation number (`requestIdentifier`) that the
//!   requester records as proof of the check. Without it the validity
//!   answer has no audit weight. The typed observation always carries
//!   the consultation number when present.
//!
//! ## Status
//!
//! Both stub and live providers ship. Default-features builds get the
//! typed domain + [`StubViesProvider`]. Building with `--features
//! live` adds [`LiveViesProvider`] which POSTs the SOAP envelope to
//! `https://ec.europa.eu/taxation_customs/vies/services/checkVatService`
//! — no auth required. Uses manifold's POST + XML helpers (added in
//! manifold v1.1.2) to keep the SOAP wiring tight and dependency-light.

mod error;
#[cfg(feature = "live")]
pub mod live;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::ViesError;
#[cfg(feature = "live")]
pub use live::LiveViesProvider;
pub use provenance::{VIES_PROVENANCE, Vies};
pub use provider::{StubViesProvider, ViesProvider, ViesRequest, ViesResponse};
pub use suggestor::{ViesLookupSuggestor, ViesValidationPayload};
pub use types::{CountryCode, MemberStateStatus, VatNumber, VatValidation};
