// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — [`OfacSlsLookupSuggestor`] reads
//! [`OfacSlsRequest`] facts from `ContextKey::Seeds` and proposes typed
//! [`OfacSlsHitPayload`] facts to `ContextKey::Hypotheses`.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact, Provenance,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::OFAC_SLS_PROVENANCE;
use crate::provider::{OfacSlsProvider, OfacSlsRequest};
use crate::types::SanctionsHit;

/// Typed fact payload — one [`SanctionsHit`] per fact. Flattens the
/// provider-side Observation into kernel-relevant fields so the
/// `ProposedFact` `PartialEq` requirement holds without forcing
/// embassy-pack to derive `PartialEq` on every `Observation<T>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OfacSlsHitPayload {
    pub hit: SanctionsHit,
    /// Joins back to `Observation::request_hash` for audit replay.
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for OfacSlsHitPayload {
    const FAMILY: &'static str = "embassy.ofac_sls.hit";
    const VERSION: u16 = 1;
}

pub struct OfacSlsLookupSuggestor<P: OfacSlsProvider + 'static> {
    provider: Arc<P>,
}

impl<P: OfacSlsProvider + 'static> OfacSlsLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: OfacSlsProvider + 'static> Suggestor for OfacSlsLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "OfacSlsLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> Provenance {
        Provenance::from(OFAC_SLS_PROVENANCE.as_str())
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<OfacSlsRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<OfacSlsRequest>() else {
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
                        "OFAC SLS screen failed; skipping seed"
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

                let payload = OfacSlsHitPayload {
                    hit: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("ofac_sls:{}:{idx}", seed.id()),
                        payload,
                        Provenance::from(OFAC_SLS_PROVENANCE.as_str()),
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
    use crate::provider::StubOfacSlsProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        let s = OfacSlsLookupSuggestor::new(Arc::new(StubOfacSlsProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical() {
        // Intent: every sanctions-hit fact must carry the canonical
        // ofac_sls provenance so compliance log searches scoped to
        // that string hit every record.
        let s = OfacSlsLookupSuggestor::new(Arc::new(StubOfacSlsProvider));
        assert_eq!(s.provenance().as_str(), "ofac_sls");
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        // Intent: payload (family, version) is the cross-version
        // contract — changing either is a sanctions-schema break.
        assert_eq!(OfacSlsHitPayload::FAMILY, "embassy.ofac_sls.hit");
        assert_eq!(OfacSlsHitPayload::VERSION, 1);
    }
}
