// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! LinkedIn port — professional network research.
//!
//! Extracted from `organism/crates/intelligence/src/linkedin.rs` on
//! 2026-05-05 as the first port to land in embassy. LinkedIn is a
//! sovereign integration: its identity is part of the contract (ToS,
//! rate limits, vendor laws), so it cannot be abstracted behind a
//! generic interface.
//!
//! The provider trait went **async** in 1.2.0 — every other embassy
//! port uses `async fn fetch`, and keeping LinkedIn sync forced the
//! Suggestor wrapper into `block_in_place` workarounds. The break is
//! intentional; single-developer workspace, no `#[deprecated]` shim.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact,
    ProvenanceSource, Suggestor,
};
use embassy_pack::{CallContext, Observation, content_hash};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Typed domain ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedInGetRequest {
    pub endpoint: String,
    pub query: HashMap<String, String>,
}

impl LinkedInGetRequest {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            query: HashMap::new(),
        }
    }
}

impl FactPayload for LinkedInGetRequest {
    const FAMILY: &'static str = "embassy.linkedin.request";
    const VERSION: u16 = 1;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkedInProfile {
    pub profile_id: String,
    pub name: String,
    pub title: Option<String>,
    pub company: Option<String>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedInGetResponse {
    pub records: Vec<Observation<LinkedInProfile>>,
}

// ── Provenance ─────────────────────────────────────────────────────────────

/// Canonical provenance marker for facts emitted by the LinkedIn port.
#[derive(Copy, Clone, Debug)]
pub struct LinkedIn;

impl ProvenanceSource for LinkedIn {
    fn as_str(&self) -> &'static str {
        "linkedin"
    }
}

pub const LINKEDIN_PROVENANCE: LinkedIn = LinkedIn;

// ── Provider ───────────────────────────────────────────────────────────────

#[async_trait]
pub trait LinkedInProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn get(
        &self,
        request: &LinkedInGetRequest,
        ctx: &CallContext,
    ) -> Result<LinkedInGetResponse, String>;
}

#[derive(Debug, Clone, Default)]
pub struct StubLinkedInProvider;

#[async_trait]
impl LinkedInProvider for StubLinkedInProvider {
    fn name(&self) -> &'static str {
        "stub_linkedin"
    }

    async fn get(
        &self,
        request: &LinkedInGetRequest,
        _ctx: &CallContext,
    ) -> Result<LinkedInGetResponse, String> {
        if request.endpoint.trim().is_empty() {
            return Err("Empty endpoint".to_string());
        }
        let hash_input = format!("{}:{:?}", request.endpoint, request.query);
        let request_hash = content_hash(&hash_input);
        let obs = Observation {
            observation_id: format!("obs:linkedin:{request_hash}"),
            request_hash,
            vendor: "stub_linkedin".to_string(),
            model: "stub".to_string(),
            latency_ms: 10,
            cost_estimate: None,
            tokens: None,
            content: LinkedInProfile {
                profile_id: "LI-STUB-001".to_string(),
                name: "Jane Doe".to_string(),
                title: Some("VP Engineering".to_string()),
                company: Some("Acme Corp".to_string()),
                payload: serde_json::json!({"name": "Jane Doe"}),
            },
            raw_response: None,
        };
        Ok(LinkedInGetResponse { records: vec![obs] })
    }
}

// ── Suggestor surface ──────────────────────────────────────────────────────

/// Typed fact payload — one [`LinkedInProfile`] per fact. Flattens the
/// provider-side Observation so `ProposedFact`'s `PartialEq` requirement
/// is satisfied without forcing `Observation<T>` to derive `PartialEq`
/// in embassy-pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinkedInProfilePayload {
    pub profile: LinkedInProfile,
    /// Joins back to `Observation::request_hash` for audit replay.
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for LinkedInProfilePayload {
    const FAMILY: &'static str = "embassy.linkedin.profile";
    const VERSION: u16 = 1;
}

/// Reads [`LinkedInGetRequest`] facts from `Seeds`, dispatches them
/// through a [`LinkedInProvider`], and proposes one
/// [`LinkedInProfilePayload`] per returned observation to `Hypotheses`.
pub struct LinkedInLookupSuggestor<P: LinkedInProvider + 'static> {
    provider: Arc<P>,
}

impl<P: LinkedInProvider + 'static> LinkedInLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: LinkedInProvider + 'static> Suggestor for LinkedInLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "LinkedInLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> &'static str {
        LINKEDIN_PROVENANCE.as_str()
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<LinkedInGetRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<LinkedInGetRequest>() else {
                continue;
            };

            let response = match self.provider.get(request, &CallContext::default()).await {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(
                        seed = %seed.id(),
                        provider = self.provider.name(),
                        error = %err,
                        "LinkedIn fetch failed; skipping seed"
                    );
                    continue;
                }
            };

            for (idx, observation) in response.records.into_iter().enumerate() {
                let runtime_config = ExecutionIdentity::runtime_config_from_typed(request);
                let execution_identity = ExecutionIdentity::non_native(
                    env!("CARGO_PKG_NAME"),
                    env!("CARGO_PKG_VERSION"),
                    self.provider.name().to_string(),
                    runtime_config,
                );

                let payload = LinkedInProfilePayload {
                    profile: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("linkedin:{}:{idx}", seed.id()),
                        payload,
                        LINKEDIN_PROVENANCE.as_str(),
                    )
                    .with_confidence(0.95),
                );
            }
        }

        if proposals.is_empty() {
            AgentEffect::empty()
        } else {
            AgentEffect::with_proposals(proposals)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_new_starts_with_empty_query() {
        let req = LinkedInGetRequest::new("/v2/people");
        assert_eq!(req.endpoint, "/v2/people");
        assert!(req.query.is_empty());
    }

    #[test]
    fn stub_provider_name_is_stable() {
        let provider = StubLinkedInProvider;
        assert_eq!(provider.name(), "stub_linkedin");
    }

    #[tokio::test]
    async fn stub_provider_returns_one_observation() {
        let provider = StubLinkedInProvider;
        let mut req = LinkedInGetRequest::new("/v2/people");
        req.query.insert("q".into(), "engineer".into());
        let ctx = CallContext::default();

        let response = provider.get(&req, &ctx).await.expect("ok");
        assert_eq!(response.records.len(), 1);

        let obs = &response.records[0];
        assert_eq!(obs.vendor, "stub_linkedin");
        assert_eq!(obs.model, "stub");
        assert_eq!(obs.content.profile_id, "LI-STUB-001");
        assert_eq!(obs.content.name, "Jane Doe");
        assert_eq!(obs.content.title.as_deref(), Some("VP Engineering"));
        assert_eq!(obs.content.company.as_deref(), Some("Acme Corp"));
        assert!(obs.observation_id.starts_with("obs:linkedin:"));
        assert_eq!(obs.observation_id.len(), "obs:linkedin:".len() + 16);
    }

    #[tokio::test]
    async fn stub_provider_rejects_empty_endpoint() {
        let provider = StubLinkedInProvider;
        let req = LinkedInGetRequest::new("   ");
        let ctx = CallContext::default();
        let err = provider.get(&req, &ctx).await.expect_err("must reject");
        assert_eq!(err, "Empty endpoint");
    }

    #[tokio::test]
    async fn stub_provider_is_deterministic_per_request() {
        let provider = StubLinkedInProvider;
        let req = LinkedInGetRequest::new("/v2/people");
        let ctx = CallContext::default();
        let a = provider.get(&req, &ctx).await.expect("ok");
        let b = provider.get(&req, &ctx).await.expect("ok");
        assert_eq!(a.records[0].request_hash, b.records[0].request_hash);
        assert_eq!(a.records[0].observation_id, b.records[0].observation_id);
    }

    #[tokio::test]
    async fn profile_payload_round_trips() {
        let req = LinkedInGetRequest::new("/v2/people");
        let provider = StubLinkedInProvider;
        let ctx = CallContext::default();
        let response = provider.get(&req, &ctx).await.expect("ok");
        let json = serde_json::to_string(&response).expect("serialize");
        let back: LinkedInGetResponse = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.records.len(), 1);
        assert_eq!(back.records[0].content.name, "Jane Doe");
    }

    #[test]
    fn provenance_string_is_canonical() {
        // Intent: every fact emitted by this port carries the
        // canonical `linkedin` provenance string. Log-search scoped to
        // `provenance="linkedin"` must hit every record.
        assert_eq!(LINKEDIN_PROVENANCE.as_str(), "linkedin");
    }

    #[test]
    fn suggestor_declares_seeds_dependency() {
        let s = LinkedInLookupSuggestor::new(Arc::new(StubLinkedInProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_linkedin() {
        let s = LinkedInLookupSuggestor::new(Arc::new(StubLinkedInProvider));
        assert_eq!(s.provenance(), "linkedin");
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        // Intent: family + version are the cross-version payload
        // contract for any downstream typed-payload consumer.
        // Changing either is a schema break; pin the values so the
        // change shows up in code review.
        assert_eq!(LinkedInProfilePayload::FAMILY, "embassy.linkedin.profile");
        assert_eq!(LinkedInProfilePayload::VERSION, 1);
        assert_eq!(LinkedInGetRequest::FAMILY, "embassy.linkedin.request");
        assert_eq!(LinkedInGetRequest::VERSION, 1);
    }
}
