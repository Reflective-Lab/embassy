// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Statistics Sweden (SCB) port — national statistical tables via PxWeb.
//!
//! Sovereign integration in the embassy sense: this port's identity
//! and contract (auth, ToS, rate-limit) are part of the API surface
//! and cannot be abstracted behind a generic transport.
//!
//! ## Status
//!
//! **Skeleton only.** The typed domain has a minimal placeholder
//! shape — newtype identifier + a bare `TableMetadata` carrier. The
//! Provider trait + `StubScbProvider` are defined so callers can
//! depend on the surface, but the live HTTP/API implementation is
//! deferred until an app pulls hard enough to justify the work.
//!
//! Grow the domain types at that time, not before.

mod error;
mod provenance;
mod provider;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::ScbError;
pub use provenance::{SCB_PROVENANCE, Scb};
pub use provider::{ScbProvider, ScbRequest, ScbResponse, StubScbProvider};
pub use types::{TableId, TableMetadata};
