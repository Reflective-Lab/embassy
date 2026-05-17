// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkatteverketError {
    #[error("invalid Swedish organisation number: {0}")]
    InvalidOrgNumber(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("org number not found: {0}")]
    NotFound(String),

    #[error("network/transport error: {0}")]
    Transport(String),

    #[error("rate-limit exceeded")]
    RateLimited,
}
