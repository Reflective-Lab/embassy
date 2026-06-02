// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — [`SkatteverketLookupSuggestor`] reads
//! [`SkatteverketRequest`] facts from `ContextKey::Seeds` and proposes
//! typed [`SkatteverketTaxStatusPayload`] facts to
//! `ContextKey::Hypotheses`.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact, Provenance,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::SKATTEVERKET_PROVENANCE;
use crate::provider::{SkatteverketProvider, SkatteverketRequest};
use crate::types::SwedishTaxStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkatteverketTaxStatusPayload {
    pub status: SwedishTaxStatus,
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for SkatteverketTaxStatusPayload {
    const FAMILY: &'static str = "embassy.skatteverket.tax_status";
    const VERSION: u16 = 1;
}

pub struct SkatteverketLookupSuggestor<P: SkatteverketProvider + 'static> {
    provider: Arc<P>,
}

impl<P: SkatteverketProvider + 'static> SkatteverketLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: SkatteverketProvider + 'static> Suggestor for SkatteverketLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "SkatteverketLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> Provenance {
        SKATTEVERKET_PROVENANCE.provenance()
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<SkatteverketRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<SkatteverketRequest>() else {
                continue;
            };

            let response = match self
                .provider
                .status(request, &embassy_pack::CallContext::default())
                .await
            {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(
                        seed = %seed.id(),
                        provider = self.provider.name(),
                        error = %err,
                        "Skatteverket status lookup failed; skipping seed"
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

                let payload = SkatteverketTaxStatusPayload {
                    status: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("skatteverket:{}:{idx}", seed.id()),
                        payload,
                        SKATTEVERKET_PROVENANCE.provenance(),
                    )
                    .with_confidence(0.97),
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
    use crate::provider::StubSkatteverketProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        let s = SkatteverketLookupSuggestor::new(Arc::new(StubSkatteverketProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical() {
        let s = SkatteverketLookupSuggestor::new(Arc::new(StubSkatteverketProvider));
        assert_eq!(s.provenance(), SKATTEVERKET_PROVENANCE.provenance());
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        assert_eq!(
            SkatteverketTaxStatusPayload::FAMILY,
            "embassy.skatteverket.tax_status"
        );
        assert_eq!(SkatteverketTaxStatusPayload::VERSION, 1);
    }
}
