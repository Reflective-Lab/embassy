// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — [`UsaspendingLookupSuggestor`] reads
//! [`UsaspendingRequest`] facts from `ContextKey::Seeds` and proposes
//! typed [`UsaspendingAwardPayload`] facts to `ContextKey::Hypotheses`.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact, Provenance,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::USASPENDING_PROVENANCE;
use crate::provider::{UsaspendingProvider, UsaspendingRequest};
use crate::types::FederalAward;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsaspendingAwardPayload {
    pub award: FederalAward,
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for UsaspendingAwardPayload {
    const FAMILY: &'static str = "embassy.usaspending.award";
    const VERSION: u16 = 1;
}

pub struct UsaspendingLookupSuggestor<P: UsaspendingProvider + 'static> {
    provider: Arc<P>,
}

impl<P: UsaspendingProvider + 'static> UsaspendingLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: UsaspendingProvider + 'static> Suggestor for UsaspendingLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "UsaspendingLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> Provenance {
        Provenance::from(USASPENDING_PROVENANCE.as_str())
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<UsaspendingRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<UsaspendingRequest>() else {
                continue;
            };

            let response = match self
                .provider
                .lookup(request, &embassy_pack::CallContext::default())
                .await
            {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(
                        seed = %seed.id(),
                        provider = self.provider.name(),
                        error = %err,
                        "USAspending lookup failed; skipping seed"
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

                let payload = UsaspendingAwardPayload {
                    award: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("usaspending:{}:{idx}", seed.id()),
                        payload,
                        Provenance::from(USASPENDING_PROVENANCE.as_str()),
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
    use crate::provider::StubUsaspendingProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        let s = UsaspendingLookupSuggestor::new(Arc::new(StubUsaspendingProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical() {
        let s = UsaspendingLookupSuggestor::new(Arc::new(StubUsaspendingProvider));
        assert_eq!(s.provenance().as_str(), "usaspending");
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        assert_eq!(UsaspendingAwardPayload::FAMILY, "embassy.usaspending.award");
        assert_eq!(UsaspendingAwardPayload::VERSION, 1);
    }
}
