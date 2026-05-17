// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + skeleton stub.
//!
//! Live HTTP/API implementation deferred. The stub here returns one
//! canned [`Organization`] per Lookup so callers can wire Formations
//! against the surface today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::CrunchbaseError;
use crate::types::{Organization, OrganizationId};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CrunchbaseRequest {
    Lookup { identifier: OrganizationId },
}

impl FactPayload for CrunchbaseRequest {
    const FAMILY: &'static str = "embassy.crunchbase.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrunchbaseResponse {
    pub records: Vec<Observation<Organization>>,
}

#[async_trait]
pub trait CrunchbaseProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &CrunchbaseRequest,
        ctx: &CallContext,
    ) -> Result<CrunchbaseResponse, CrunchbaseError>;
}

#[derive(Debug, Clone, Default)]
pub struct StubCrunchbaseProvider;

#[async_trait]
impl CrunchbaseProvider for StubCrunchbaseProvider {
    fn name(&self) -> &'static str {
        "stub_crunchbase"
    }

    async fn fetch(
        &self,
        request: &CrunchbaseRequest,
        _ctx: &CallContext,
    ) -> Result<CrunchbaseResponse, CrunchbaseError> {
        let hash_input = serde_json::to_string(request).map_err(|e| {
            CrunchbaseError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&hash_input);

        let CrunchbaseRequest::Lookup { identifier } = request;
        let entity = Organization {
            permalink: identifier.clone(),
            name: "Stub Organization".to_string(),
        };

        let obs = Observation {
            observation_id: format!("obs:crunchbase:{request_hash}"),
            request_hash,
            vendor: "stub_crunchbase".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: None,
        };

        Ok(CrunchbaseResponse { records: vec![obs] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        // Intent: replay-from-audit contract — the recorded
        // request_hash must equal content_hash(canonical-JSON
        // request). Same load-bearing guarantee as every other port.
        let provider = StubCrunchbaseProvider;
        let req = CrunchbaseRequest::Lookup {
            identifier: OrganizationId::parse("STUB-001").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation() {
        let provider = StubCrunchbaseProvider;
        let req = CrunchbaseRequest::Lookup {
            identifier: OrganizationId::parse("STUB-001").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
