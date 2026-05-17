// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! European Patent Office (OPS) port — EP patents, families, citations.
//!
//! Sovereign integration in the embassy sense: this port's identity
//! and contract (auth, ToS, rate-limit) are part of the API surface
//! and cannot be abstracted behind a generic transport.
//!
//! ## Status
//!
//! **Skeleton only.** The typed domain has a minimal placeholder
//! shape — newtype identifier + a bare `Patent` carrier. The
//! Provider trait + `StubEpoProvider` are defined so callers can
//! depend on the surface, but the live HTTP/API implementation is
//! deferred until an app pulls hard enough to justify the work.
//!
//! Grow the domain types at that time, not before.

mod error;
mod provenance;
mod provider;
mod types;

pub use embassy_pack::{CallContext, Observation, content_hash};

pub use error::EpoError;
pub use provenance::{EPO_PROVENANCE, Epo};
pub use provider::{EpoProvider, EpoRequest, EpoResponse, StubEpoProvider};
pub use types::{EpoNumber, Patent};
