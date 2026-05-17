// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommerceCslError {
    #[error("invalid subject: {0}")]
    InvalidSubject(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("network/transport error: {0}")]
    Transport(String),

    #[error("rate-limit exceeded")]
    RateLimited,
}
