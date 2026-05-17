// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! GLEIF port — Global Legal Entity Identifier Foundation.
//!
//! GLEIF is the spine of counterparty identity. Every regulated entity
//! that participates in international transactions has (or should have)
//! a 20-character **LEI** issued under ISO 17442. Embassy's GLEIF port
//! exposes that namespace + the ownership relationships GLEIF publishes
//! (direct parent, ultimate parent, child entities) as typed evidence
//! suitable for any Formation that asks *"who is this counterparty?"*
//!
//! ## Sovereign contract
//!
//! - **LEI structure** is ISO 17442: 20 chars, chars 1-4 are the LOU
//!   (issuer) prefix, chars 5-6 are reserved `"00"`, chars 7-18 are the
//!   entity-specific portion, chars 19-20 are the mod-97-10 check
//!   digits. The port validates the check digit on construction so bogus
//!   IDs never reach the audit trail.
//! - **Licensing** is **CC0** — GLEIF publishes its data into the public
//!   domain. Apps can store, redistribute, and combine LEI records
//!   freely. The port records this in observation metadata so
//!   downstream consumers can rely on it.
//! - **Authority** is consultative: GLEIF *publishes* a registry; the
//!   actual legal force of an LEI is jurisdiction-dependent (mostly EU
//!   MiFID II + US Dodd-Frank). Embassy carries the GLEIF record
//!   verbatim; downstream policy decides what to do with it.
//!
//! ## What this port does NOT own
//!
//! Per `Operating Authority Boundary`: this port observes; it does not
//! decide if a counterparty is *allowed*. Sanctions screening (OFAC,
//! EU sanctions, Commerce CSL) is a separate set of ports. GLEIF
//! provides the identity spine those screens join on (most sanctions
//! lists publish target LEIs alongside name/address).
//!
//! ## Status
//!
//! Stub provider only. Live HTTP against
//! `https://api.gleif.org/api/v1/lei-records/{lei}` deferred to a
//! `live` feature in a follow-on when a real app pulls.

mod error;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::GleifError;
pub use provenance::{GLEIF_PROVENANCE, Gleif};
pub use provider::{GleifProvider, GleifRequest, GleifResponse, StubGleifProvider};
pub use suggestor::{GleifLegalEntityPayload, GleifLookupSuggestor};
pub use types::{EntityCategory, LegalEntity, LegalEntityStatus, Lei, RegistrationStatus};
