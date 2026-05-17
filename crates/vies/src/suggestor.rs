// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — [`ViesLookupSuggestor`] reads
//! [`ViesRequest`] facts from `ContextKey::Seeds` and proposes typed
//! [`ViesValidationPayload`] facts to `ContextKey::Hypotheses`.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::VIES_PROVENANCE;
use crate::provider::{ViesProvider, ViesRequest};
use crate::types::VatValidation;

/// Typed fact payload — one [`VatValidation`] per fact. Flattens the
/// provider-side Observation into kernel-relevant fields so the
/// `ProposedFact::new` `PartialEq` requirement is satisfied without
/// forcing embassy-pack to derive `PartialEq` on every `Observation<T>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViesValidationPayload {
    pub validation: VatValidation,
    /// Joins back to `Observation::request_hash` for audit replay.
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for ViesValidationPayload {
    const FAMILY: &'static str = "embassy.vies.validation";
    const VERSION: u16 = 1;
}

pub struct ViesLookupSuggestor<P: ViesProvider + 'static> {
    provider: Arc<P>,
}

impl<P: ViesProvider + 'static> ViesLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: ViesProvider + 'static> Suggestor for ViesLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "ViesLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> &'static str {
        VIES_PROVENANCE.as_str()
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<ViesRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<ViesRequest>() else {
                continue;
            };

            let response = match self
                .provider
                .validate(request, &embassy_pack::CallContext::default())
                .await
            {
                Ok(resp) => resp,
                Err(err) => {
                    tracing::warn!(
                        seed = %seed.id(),
                        provider = self.provider.name(),
                        error = %err,
                        "VIES validation failed; skipping seed"
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

                let payload = ViesValidationPayload {
                    validation: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("vies:{}:{idx}", seed.id()),
                        payload,
                        VIES_PROVENANCE.as_str(),
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
    use crate::provider::StubViesProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        let s = ViesLookupSuggestor::new(Arc::new(StubViesProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical_vies() {
        // Intent: audit-log scope `provenance="vies"` must hit every
        // validation observation. VAT-check audit trails are
        // regulatory artefacts; mis-tagging breaks the tax-compliance
        // story.
        let s = ViesLookupSuggestor::new(Arc::new(StubViesProvider));
        assert_eq!(s.provenance(), "vies");
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        // Intent: payload (family, version) is the cross-version
        // contract for any downstream consumer filtering VIES facts.
        // Changing either is a payload-schema break — pin the values
        // so the change shows up in code review.
        assert_eq!(ViesValidationPayload::FAMILY, "embassy.vies.validation");
        assert_eq!(ViesValidationPayload::VERSION, 1);
    }
}
