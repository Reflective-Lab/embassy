// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — [`TedLookupSuggestor`] reads
//! [`TedRequest`] facts from `ContextKey::Seeds` and proposes typed
//! [`TedNoticePayload`] facts to `ContextKey::Hypotheses`.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact, Provenance,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::TED_PROVENANCE;
use crate::provider::{TedProvider, TedRequest};
use crate::types::ProcurementNotice;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TedNoticePayload {
    pub notice: ProcurementNotice,
    pub request_hash: String,
    pub vendor: String,
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for TedNoticePayload {
    const FAMILY: &'static str = "embassy.ted.notice";
    const VERSION: u16 = 1;
}

pub struct TedLookupSuggestor<P: TedProvider + 'static> {
    provider: Arc<P>,
}

impl<P: TedProvider + 'static> TedLookupSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: TedProvider + 'static> Suggestor for TedLookupSuggestor<P> {
    fn name(&self) -> &'static str {
        "TedLookupSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> Provenance {
        Provenance::from(TED_PROVENANCE.as_str())
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<TedRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<TedRequest>() else {
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
                        "TED lookup failed; skipping seed"
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

                let payload = TedNoticePayload {
                    notice: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("ted:{}:{idx}", seed.id()),
                        payload,
                        Provenance::from(TED_PROVENANCE.as_str()),
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
    use crate::provider::StubTedProvider;

    #[test]
    fn suggestor_declares_seeds_dependency() {
        let s = TedLookupSuggestor::new(Arc::new(StubTedProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical() {
        let s = TedLookupSuggestor::new(Arc::new(StubTedProvider));
        assert_eq!(s.provenance().as_str(), "ted");
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        assert_eq!(TedNoticePayload::FAMILY, "embassy.ted.notice");
        assert_eq!(TedNoticePayload::VERSION, 1);
    }
}
