// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecEdgarError {
    #[error("invalid CIK: {0}")]
    InvalidCik(String),

    #[error("invalid accession number: {0}")]
    InvalidAccession(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("network/transport error: {0}")]
    Transport(String),

    #[error("rate-limit exceeded — the SEC enforces 10 req/sec workspace-wide")]
    RateLimited,
}
