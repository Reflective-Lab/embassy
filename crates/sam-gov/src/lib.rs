// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! SAM.gov port — US System for Award Management.
//!
//! SAM.gov is the canonical US federal registration system: every
//! organization that wants to do business with the US federal
//! government must register, be assigned a Unique Entity Identifier
//! (UEI, 12 alphanumeric chars), and maintain an active registration.
//!
//! Embassy port: evidence-only. A contractor registration record here
//! says *what SAM.gov holds about a given UEI*. Decisions to enter
//! into a contract, file an exclusion, or refuse business belong in
//! the Commercial Rail layer.
//!
//! Pairs with `embassy-usaspending`: SAM gives you the *who is
//! registered*, USAspending gives you the *what awards have been made*.
//! Together they answer Q1 (counterparty integrity) and Q3 (federal
//! spend exposure) from one US-bundle.
//!
//! ## Source
//!
//! Real provider: `https://api.sam.gov/entity-information/v3/entities`
//! (requires API key). Live integration deferred.
//!
//! ## Status
//!
//! Both stub and live providers ship. Default-features builds get the
//! typed domain + [`StubSamGovProvider`]. Building with
//! `--features live` adds [`LiveSamGovProvider`] which calls
//! `https://api.sam.gov/entity-information/v3/entities` — requires
//! `SAM_GOV_API_KEY` (free registration at `api.data.gov/signup/`).
//! Per the REAL-by-default standard, `from_env()` fails loud if the
//! var is missing; there is no silent stub fallback.

mod error;
#[cfg(feature = "live")]
pub mod live;
mod provenance;
mod provider;
mod suggestor;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::SamGovError;
#[cfg(feature = "live")]
pub use live::LiveSamGovProvider;
pub use provenance::{SAM_GOV_PROVENANCE, SamGov};
pub use provider::{SamGovProvider, SamGovRequest, SamGovResponse, StubSamGovProvider};
pub use suggestor::{SamGovLookupSuggestor, SamGovRegistrationPayload};
pub use types::{ContractorRegistration, RegistrationStatus, Uei};
