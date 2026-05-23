// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Provider trait + deterministic stub.

use async_trait::async_trait;
use converge_pack::FactPayload;
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};

use crate::error::TedError;
use crate::types::{ProcurementNotice, ProcurementType, TedNoticeId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TedRequest {
    Lookup {
        notice_id: TedNoticeId,
    },
    /// Search for recent procurement notices in a given country.
    /// Cursor-paginated upstream — the live provider follows the
    /// `iterationNextToken` chain via `manifold::pagination::paginate`
    /// until `limit` notices are gathered or pages run out.
    SearchByCountry {
        /// ISO 3166-1 alpha-2 country code, e.g., `"SE"`, `"DE"`.
        country: String,
        /// Cap on the total number of typed notices returned.
        limit: usize,
    },
    /// Search for procurement notices where a named entity is the
    /// buyer. Used in KYC / counterparty enrichment: "has this
    /// entity issued public-sector contracts on TED?". Match is
    /// exact (TED v3 doesn't accept wildcards on `buyer-name=`);
    /// callers provide the canonical legal name.
    SearchByBuyerName {
        /// Exact buyer name as it appears in TED publications.
        buyer_name: String,
        /// Cap on the total number of typed notices returned.
        limit: usize,
    },
}

impl FactPayload for TedRequest {
    const FAMILY: &'static str = "embassy.ted.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TedResponse {
    pub records: Vec<Observation<ProcurementNotice>>,
}

#[async_trait]
pub trait TedProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn lookup(
        &self,
        request: &TedRequest,
        ctx: &CallContext,
    ) -> Result<TedResponse, TedError>;
}

/// Deterministic stub. Returns one canned [`ProcurementNotice`] per
/// Lookup.
#[derive(Debug, Clone, Default)]
pub struct StubTedProvider;

#[async_trait]
impl TedProvider for StubTedProvider {
    fn name(&self) -> &'static str {
        "stub_ted"
    }

    async fn lookup(
        &self,
        request: &TedRequest,
        _ctx: &CallContext,
    ) -> Result<TedResponse, TedError> {
        let hash_input = serde_json::to_string(request)
            .map_err(|e| TedError::InvalidRequest(format!("non-serializable request: {e}")))?;
        let request_hash = content_hash(&hash_input);

        match request {
            TedRequest::Lookup { notice_id } => {
                let notice = ProcurementNotice {
                    notice_id: notice_id.clone(),
                    contracting_authority: "Stockholm Stad (stub)".to_string(),
                    title: format!("Stub procurement {}", notice_id.as_str()),
                    country: "SE".to_string(),
                    procurement_type: ProcurementType::ContractNotice,
                    deadline: Some("2026-12-31T23:59:59Z".to_string()),
                };

                let obs = Observation {
                    observation_id: format!("obs:ted:{request_hash}"),
                    request_hash,
                    vendor: "stub_ted".to_string(),
                    model: "stub".to_string(),
                    latency_ms: 5,
                    cost_estimate: None,
                    tokens: None,
                    content: notice,
                    raw_response: None,
                };

                Ok(TedResponse { records: vec![obs] })
            }
            TedRequest::SearchByCountry { country, limit } => {
                let mut records = Vec::with_capacity(*limit);
                for i in 0..*limit {
                    let notice_id_str = format!("{:06}-2026", 100_000 + i);
                    let notice_id = TedNoticeId::parse(&notice_id_str)?;
                    let notice = ProcurementNotice {
                        notice_id,
                        contracting_authority: format!("Stub Authority #{i}"),
                        title: format!("Stub procurement notice #{i} for {country}"),
                        country: country.clone(),
                        procurement_type: ProcurementType::ContractNotice,
                        deadline: Some("2026-12-31T23:59:59Z".to_string()),
                    };
                    records.push(Observation {
                        observation_id: format!("obs:ted:{request_hash}:{i}"),
                        request_hash: request_hash.clone(),
                        vendor: "stub_ted".to_string(),
                        model: "stub".to_string(),
                        latency_ms: 5,
                        cost_estimate: None,
                        tokens: None,
                        content: notice,
                        raw_response: None,
                    });
                }
                Ok(TedResponse { records })
            }
            TedRequest::SearchByBuyerName { buyer_name, limit } => {
                let mut records = Vec::with_capacity(*limit);
                for i in 0..*limit {
                    let notice_id_str = format!("{:06}-2026", 200_000 + i);
                    let notice_id = TedNoticeId::parse(&notice_id_str)?;
                    let notice = ProcurementNotice {
                        notice_id,
                        contracting_authority: buyer_name.clone(),
                        title: format!("Stub procurement notice #{i} by {buyer_name}"),
                        country: "SE".to_string(),
                        procurement_type: ProcurementType::ContractNotice,
                        deadline: Some("2026-12-31T23:59:59Z".to_string()),
                    };
                    records.push(Observation {
                        observation_id: format!("obs:ted:{request_hash}:{i}"),
                        request_hash: request_hash.clone(),
                        vendor: "stub_ted".to_string(),
                        model: "stub".to_string(),
                        latency_ms: 5,
                        cost_estimate: None,
                        tokens: None,
                        content: notice,
                        raw_response: None,
                    });
                }
                Ok(TedResponse { records })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stub_request_hash_matches_content_hash() {
        let provider = StubTedProvider;
        let req = TedRequest::Lookup {
            notice_id: TedNoticeId::parse("123456-2026").unwrap(),
        };
        let resp = provider
            .lookup(&req, &CallContext::default())
            .await
            .unwrap();
        let expected = content_hash(&serde_json::to_string(&req).unwrap());
        assert_eq!(resp.records[0].request_hash, expected);
    }

    #[tokio::test]
    async fn stub_returns_one_observation_per_lookup() {
        let provider = StubTedProvider;
        let req = TedRequest::Lookup {
            notice_id: TedNoticeId::parse("123456-2026").unwrap(),
        };
        let resp = provider
            .lookup(&req, &CallContext::default())
            .await
            .unwrap();
        assert_eq!(resp.records.len(), 1);
    }
}
