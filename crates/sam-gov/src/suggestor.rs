// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — [`SamGovLookupSuggestor`] reads
//! [`SamGovRequest`] facts from `ContextKey::Seeds` and proposes typed
//! [`SamGovRegistrationPayload`] facts to `ContextKey::Hypotheses`.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact, Provenance,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::SAM_GOV_PROVENANCE;
use crate::provider::{SamGovProvider, SamGovRequest};
use crate::types::ContractorRegistration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamGovRegistrationPayload {
    pub registration: ContractorRegistration,
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for SamGovRegistrationPayload {
    const FAMILY: &'static str = "embassy.sam_gov.registration";
    const VERSION: u16 = 1;
}

pub struct SamGovLookupSuggestor<P: SamGovProvider + 'static> {
    provider: Arc<P>,
}

impl<P: SamGovProvider + 'static> SamGovLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: SamGovProvider + 'static> Suggestor for SamGovLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "SamGovLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> Provenance {
        SAM_GOV_PROVENANCE.provenance()
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<SamGovRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<SamGovRequest>() else {
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
                        "SAM.gov lookup failed; skipping seed"
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

                let payload = SamGovRegistrationPayload {
                    registration: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("sam_gov:{}:{idx}", seed.id()),
                        payload,
                        SAM_GOV_PROVENANCE.provenance(),
                    )
                    .with_subject_from(seed)
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
    use crate::provider::StubSamGovProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        let s = SamGovLookupSuggestor::new(Arc::new(StubSamGovProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical() {
        let s = SamGovLookupSuggestor::new(Arc::new(StubSamGovProvider));
        assert_eq!(s.provenance(), SAM_GOV_PROVENANCE.provenance());
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        assert_eq!(
            SamGovRegistrationPayload::FAMILY,
            "embassy.sam_gov.registration"
        );
        assert_eq!(SamGovRegistrationPayload::VERSION, 1);
    }
}
