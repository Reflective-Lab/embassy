// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — `BolagsverketLookupSuggestor` reads
//! [`BolagsverketRequest`] facts from `ContextKey::Seeds` and proposes
//! typed [`BolagsverketCompanyPayload`] facts to `ContextKey::Hypotheses`.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::BOLAGSVERKET_PROVENANCE;
use crate::provider::{BolagsverketProvider, BolagsverketRequest};
use crate::types::Company;

/// Typed fact payload — one company per fact. Flattens the provider-
/// side Observation into kernel-relevant fields (same rationale as the
/// SEC port: kernel needs `PartialEq`, embassy-pack `Observation<T>`
/// does not derive it; flatten at the kernel boundary).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BolagsverketCompanyPayload {
    pub company: Company,
    /// Joins back to `Observation::request_hash` for audit replay.
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for BolagsverketCompanyPayload {
    const FAMILY: &'static str = "embassy.bolagsverket.company";
    const VERSION: u16 = 1;
}

pub struct BolagsverketLookupSuggestor<P: BolagsverketProvider + 'static> {
    provider: Arc<P>,
}

impl<P: BolagsverketProvider + 'static> BolagsverketLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: BolagsverketProvider + 'static> Suggestor for BolagsverketLookupSuggestor<P> {
    fn name(&self) -> &str {
        "BolagsverketLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> &'static str {
        BOLAGSVERKET_PROVENANCE.as_str()
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<BolagsverketRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<BolagsverketRequest>() else {
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
                        "Bolagsverket fetch failed; skipping seed"
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

                let payload = BolagsverketCompanyPayload {
                    company: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("bolagsverket:{}:{idx}", seed.id()),
                        payload,
                        BOLAGSVERKET_PROVENANCE.as_str(),
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
    use crate::provider::StubBolagsverketProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        // Intent: same as SEC port — engine wakes by dirty
        // dependencies; losing Seeds here would mute the Suggestor
        // entirely.
        let s = BolagsverketLookupSuggestor::new(Arc::new(StubBolagsverketProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical_bolagsverket() {
        // Intent: audit log scope filter `provenance="bolagsverket"`
        // must always hit every fact this Suggestor emits.
        let s = BolagsverketLookupSuggestor::new(Arc::new(StubBolagsverketProvider));
        assert_eq!(s.provenance(), "bolagsverket");
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        // Intent: same cross-version payload contract as the SEC port.
        assert_eq!(
            BolagsverketCompanyPayload::FAMILY,
            "embassy.bolagsverket.company"
        );
        assert_eq!(BolagsverketCompanyPayload::VERSION, 1);
    }
}
