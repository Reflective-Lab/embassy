// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — [`EpoLookupSuggestor`] reads
//! [`EpoRequest`] facts from `ContextKey::Seeds` and proposes
//! typed [`EpoPatentPayload`] facts to `ContextKey::Hypotheses`.
//!
//! Same shape as every other embassy Suggestor: the kernel payload
//! flattens the provider-side [`embassy_pack::Observation`] into
//! audit-relevant fields, so `ProposedFact`'s `PartialEq` requirement
//! is satisfied without forcing every `Observation<T>` content to derive
//! it.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact, Provenance,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::EPO_PROVENANCE;
use crate::provider::{EpoProvider, EpoRequest};
use crate::types::Patent;

/// Typed fact payload — one [`Patent`] per fact. Flattens the
/// provider-side Observation into kernel-relevant fields so the
/// `ProposedFact` `PartialEq` requirement holds without committing
/// embassy-pack to deriving `PartialEq` on every `Observation<T>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpoPatentPayload {
    pub patent: Patent,
    /// Joins back to `Observation::request_hash` for audit replay.
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for EpoPatentPayload {
    const FAMILY: &'static str = "embassy.epo.patent";
    const VERSION: u16 = 1;
}

pub struct EpoLookupSuggestor<P: EpoProvider + 'static> {
    provider: Arc<P>,
}

impl<P: EpoProvider + 'static> EpoLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: EpoProvider + 'static> Suggestor for EpoLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "EpoLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> Provenance {
        EPO_PROVENANCE.provenance()
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<EpoRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<EpoRequest>() else {
                continue;
            };

            let response = match self
                .provider
                .fetch(request, &embassy_pack::CallContext::default())
                .await
            {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(
                        seed = %seed.id(),
                        provider = self.provider.name(),
                        error = %err,
                        "epo fetch failed; skipping seed"
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

                let payload_value = EpoPatentPayload {
                    patent: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("epo:{}:{idx}", seed.id()),
                        payload_value,
                        EPO_PROVENANCE.provenance(),
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
    use crate::provider::StubEpoProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        // Intent: the engine wakes Suggestors by dirty-dependency
        // intersection. If this list ever drops Seeds, the Suggestor
        // stops firing on incoming requests — and would silently miss
        // every one. Pin the contract.
        let s = EpoLookupSuggestor::new(Arc::new(StubEpoProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical() {
        // Intent: every fact this Suggestor emits must be tagged
        // with the canonical port provenance string so audit log
        // searches scoped to that string hit every record.
        let s = EpoLookupSuggestor::new(Arc::new(StubEpoProvider));
        assert_eq!(s.provenance(), EPO_PROVENANCE.provenance());
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        // Intent: payload (family, version) is the cross-version
        // contract for any consumer filtering typed payloads.
        // Changing either is a payload-schema break — pin the values
        // so the change shows up in code review.
        assert_eq!(EpoPatentPayload::FAMILY, "embassy.epo.patent");
        assert_eq!(EpoPatentPayload::VERSION, 1);
    }
}
