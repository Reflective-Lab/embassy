// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GleifError {
    #[error("invalid LEI: {0}")]
    InvalidLei(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("LEI {0} not found in registry")]
    NotFound(String),

    #[error("network/transport error: {0}")]
    Transport(String),

    #[error("rate-limit exceeded")]
    RateLimited,
}
