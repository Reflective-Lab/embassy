// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + skeleton stub.
//!
//! Live HTTP/API implementation deferred. The stub here returns one
//! canned [`Company`] per Lookup so callers can wire Formations
//! against the surface today.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::CompaniesHouseError;
use crate::types::{Company, CompanyNumber};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompaniesHouseRequest {
    Lookup { identifier: CompanyNumber },
}

impl FactPayload for CompaniesHouseRequest {
    const FAMILY: &'static str = "embassy.companies_house.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompaniesHouseResponse {
    pub records: Vec<Observation<Company>>,
}

#[async_trait]
pub trait CompaniesHouseProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn fetch(
        &self,
        request: &CompaniesHouseRequest,
        ctx: &CallContext,
    ) -> Result<CompaniesHouseResponse, CompaniesHouseError>;
}

#[derive(Debug, Clone, Default)]
pub struct StubCompaniesHouseProvider;

#[async_trait]
impl CompaniesHouseProvider for StubCompaniesHouseProvider {
    fn name(&self) -> &'static str {
        "stub_companies_house"
    }

    async fn fetch(
        &self,
        request: &CompaniesHouseRequest,
        _ctx: &CallContext,
    ) -> Result<CompaniesHouseResponse, CompaniesHouseError> {
        let hash_input = serde_json::to_string(request).map_err(|e| {
            CompaniesHouseError::InvalidRequest(format!("non-serializable request: {e}"))
        })?;
        let request_hash = content_hash(&hash_input);

        let CompaniesHouseRequest::Lookup { identifier } = request;
        let entity = Company {
            company_number: identifier.clone(),
            company_name: "Stub Company".to_string(),
        };

        let obs = Observation {
            observation_id: format!("obs:companies-house:{request_hash}"),
            request_hash,
            vendor: "stub_companies_house".to_string(),
            model: "stub".to_string(),
            latency_ms: 5,
            cost_estimate: None,
            tokens: None,
            content: entity,
            raw_response: None,
        };

        Ok(CompaniesHouseResponse { records: vec![obs] })
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
        let provider = StubCompaniesHouseProvider;
        let req = CompaniesHouseRequest::Lookup {
            identifier: CompanyNumber::parse("12345678").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation() {
        let provider = StubCompaniesHouseProvider;
        let req = CompaniesHouseRequest::Lookup {
            identifier: CompanyNumber::parse("12345678").unwrap(),
        };
        let resp = provider.fetch(&req, &CallContext::default()).await.unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
