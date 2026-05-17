// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Skatteverket port — Swedish Tax Agency.
//!
//! ## Legal scoping — IMPORTANT
//!
//! This port covers **only the publicly queryable surface** that
//! Skatteverket exposes for business-to-business diligence:
//!
//! - **F-skatt status** (whether an organisation is registered for
//!   F-skatt — i.e., tax-self-employed status that determines who
//!   withholds tax on payments)
//! - **VAT (moms) registration status** — whether the entity is
//!   registered for Swedish VAT
//! - **Employer registration status** (arbetsgivarregistrering)
//!
//! These three are publicly disclosable per Skatteverket's `e-tjänst`
//! for verifying counterparties, and per Offentlighetsprincipen
//! (Public Access to Information principle) as applied to corporate
//! tax-administrative status (not personal tax data).
//!
//! **Out of scope** (do not add without legal review):
//! - Personal income, tax assessments, deductions
//! - Withholding tax amounts, owed balances
//! - Audit history or enforcement actions
//! - Any individual taxpayer data
//!
//! ## Embassy port: evidence-only
//!
//! Results say *what Skatteverket publicly publishes* about an
//! organisation. Decisions to invoice, withhold, or refuse business
//! belong in the Commercial Rail layer.
//!
//! ## Source
//!
//! Real provider: Skatteverket's public lookup service
//! (`https://www7.skatteverket.se/portal/foretagochorganisationer/`).
//! Live integration deferred.
//!
//! ## Status
//!
//! **Skeleton only** — stub returns deterministic synthetic status.

mod error;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::SkatteverketError;
pub use provenance::{SKATTEVERKET_PROVENANCE, Skatteverket};
pub use provider::{
    SkatteverketProvider, SkatteverketRequest, SkatteverketResponse, StubSkatteverketProvider,
};
pub use suggestor::{SkatteverketLookupSuggestor, SkatteverketTaxStatusPayload};
pub use types::{SwedishOrgNumber, SwedishTaxStatus};
