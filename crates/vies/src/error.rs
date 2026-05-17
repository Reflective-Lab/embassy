// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ViesError {
    #[error("invalid VAT number: {0}")]
    InvalidVatNumber(String),

    #[error("country `{0}` is not an EU member; VIES cannot validate it")]
    NonEuCountry(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("network/transport error: {0}")]
    Transport(String),

    /// VIES is up but the requested member state's database is
    /// temporarily unavailable. Surfaced as a typed status, distinct
    /// from a transport-level failure — callers may want to retry on a
    /// different cadence.
    #[error("VIES member state {0} is currently unavailable")]
    MemberStateUnavailable(String),

    #[error("rate-limit exceeded")]
    RateLimited,
}
