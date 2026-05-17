// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UsaspendingError {
    #[error("invalid award id: {0}")]
    InvalidAwardId(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("award not found: {0}")]
    NotFound(String),

    #[error("network/transport error: {0}")]
    Transport(String),

    #[error("rate-limit exceeded")]
    RateLimited,
}
