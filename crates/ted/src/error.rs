// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TedError {
    #[error("invalid notice id: {0}")]
    InvalidNoticeId(String),

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("notice not found: {0}")]
    NotFound(String),

    #[error("network/transport error: {0}")]
    Transport(String),

    #[error("rate-limit exceeded")]
    RateLimited,
}
