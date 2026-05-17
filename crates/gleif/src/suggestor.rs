// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — [`GleifLookupSuggestor`] reads
//! [`GleifRequest`] facts from `ContextKey::Seeds` and proposes typed
//! [`GleifLegalEntityPayload`] facts to `ContextKey::Hypotheses`.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::GLEIF_PROVENANCE;
use crate::provider::{GleifProvider, GleifRequest};
use crate::types::LegalEntity;

/// Typed fact payload — one [`LegalEntity`] per fact. Flattens the
/// provider-side Observation into kernel-relevant fields so the
/// `ProposedFact::new` `PartialEq` requirement is satisfied without
/// forcing embassy-pack to derive `PartialEq` on every `Observation<T>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GleifLegalEntityPayload {
    pub entity: LegalEntity,
    /// Joins back to `Observation::request_hash` for audit replay.
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for GleifLegalEntityPayload {
    const FAMILY: &'static str = "embassy.gleif.legal_entity";
    const VERSION: u16 = 1;
}

pub struct GleifLookupSuggestor<P: GleifProvider + 'static> {
    provider: Arc<P>,
}

impl<P: GleifProvider + 'static> GleifLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: GleifProvider + 'static> Suggestor for GleifLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "GleifLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> &'static str {
        GLEIF_PROVENANCE.as_str()
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<GleifRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<GleifRequest>() else {
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
                        "GLEIF fetch failed; skipping seed"
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

                let payload = GleifLegalEntityPayload {
                    entity: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("gleif:{}:{idx}", seed.id()),
                        payload,
                        GLEIF_PROVENANCE.as_str(),
                    )
                    .with_confidence(0.98),
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
    use crate::provider::StubGleifProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        // Intent: the engine wakes Suggestors by dirty-dependency
        // intersection. Losing Seeds here mutes the Suggestor.
        let s = GleifLookupSuggestor::new(Arc::new(StubGleifProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical_gleif() {
        // Intent: every fact this Suggestor emits is tagged "gleif"
        // so audit-log scope filters hit every record. Critical for
        // GLEIF specifically because every counterparty-identity
        // query in the workspace will eventually join on this
        // provenance string.
        let s = GleifLookupSuggestor::new(Arc::new(StubGleifProvider));
        assert_eq!(s.provenance(), "gleif");
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        // Intent: payload (family, version) is the cross-version
        // contract for any downstream consumer filtering typed
        // payloads. Changing either is a payload-schema break — pin
        // current values so the change is visible in code review.
        assert_eq!(
            GleifLegalEntityPayload::FAMILY,
            "embassy.gleif.legal_entity"
        );
        assert_eq!(GleifLegalEntityPayload::VERSION, 1);
    }
}
