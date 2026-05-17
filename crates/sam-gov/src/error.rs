// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SamGovError {
    #[error("invalid UEI: {0}")]
    InvalidUei(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("UEI not found: {0}")]
    NotFound(String),

    #[error("network/transport error: {0}")]
    Transport(String),

    #[error("rate-limit exceeded")]
    RateLimited,
}
