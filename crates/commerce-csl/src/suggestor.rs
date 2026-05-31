// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — [`CommerceCslLookupSuggestor`] reads
//! [`CommerceCslRequest`] facts from `ContextKey::Seeds` and proposes
//! typed [`CommerceCslHitPayload`] facts to `ContextKey::Hypotheses`.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact, Provenance,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::COMMERCE_CSL_PROVENANCE;
use crate::provider::{CommerceCslProvider, CommerceCslRequest};
use crate::types::SanctionsHit;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommerceCslHitPayload {
    pub hit: SanctionsHit,
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for CommerceCslHitPayload {
    const FAMILY: &'static str = "embassy.commerce_csl.hit";
    const VERSION: u16 = 1;
}

pub struct CommerceCslLookupSuggestor<P: CommerceCslProvider + 'static> {
    provider: Arc<P>,
}

impl<P: CommerceCslProvider + 'static> CommerceCslLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: CommerceCslProvider + 'static> Suggestor for CommerceCslLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "CommerceCslLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> Provenance {
        Provenance::from(COMMERCE_CSL_PROVENANCE.as_str())
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<CommerceCslRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<CommerceCslRequest>() else {
                continue;
            };

            let response = match self
                .provider
                .screen(request, &embassy_pack::CallContext::default())
                .await
            {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(
                        seed = %seed.id(),
                        provider = self.provider.name(),
                        error = %err,
                        "Commerce CSL screen failed; skipping seed"
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

                let payload = CommerceCslHitPayload {
                    hit: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("commerce_csl:{}:{idx}", seed.id()),
                        payload,
                        Provenance::from(COMMERCE_CSL_PROVENANCE.as_str()),
                    )
                    .with_confidence(0.99),
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
    use crate::provider::StubCommerceCslProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        let s = CommerceCslLookupSuggestor::new(Arc::new(StubCommerceCslProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical() {
        let s = CommerceCslLookupSuggestor::new(Arc::new(StubCommerceCslProvider));
        assert_eq!(s.provenance().as_str(), "commerce_csl");
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        assert_eq!(CommerceCslHitPayload::FAMILY, "embassy.commerce_csl.hit");
        assert_eq!(CommerceCslHitPayload::VERSION, 1);
    }
}
