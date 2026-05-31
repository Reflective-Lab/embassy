// Copyright 2024-2026 Reflective Labs
// SPDX-License-Identifier: MIT

//! Formation-callable surface — `SecFilingSuggestor` reads
//! [`SecEdgarRequest`] facts from `ContextKey::Seeds` and proposes typed
//! [`SecFilingPayload`] facts to `ContextKey::Hypotheses`.
//!
//! The Suggestor is the entry point for any convergence loop that needs
//! SEC filings. It owns no transport logic — it delegates to a
//! [`SecEdgarProvider`], so test loops can use [`crate::StubSecEdgarProvider`]
//! while production loops swap in the live HTTP provider when it ships.

use std::sync::Arc;

use async_trait::async_trait;
use converge_pack::{
    AgentEffect, Context, ContextKey, ExecutionIdentity, FactPayload, ProposedFact, Provenance,
    ProvenanceSource, Suggestor,
};
use serde::{Deserialize, Serialize};

use crate::provenance::SEC_EDGAR_PROVENANCE;
use crate::provider::{SecEdgarProvider, SecEdgarRequest};
use crate::types::Filing;

/// Typed fact payload — one filing per fact, ready to ride the
/// convergence kernel's promotion machinery.
///
/// Flattens the provider-side [`embassy_pack::Observation`] into the
/// audit-relevant fields directly. The kernel's `ProposedFact` already
/// owns provenance and the `FactPayload` contract requires `PartialEq`
/// for deduplication, so re-wrapping `Observation<T>` would force
/// embassy-pack to derive `PartialEq` on every typed content. Keep the
/// observation shape at the provider boundary; flatten at the kernel
/// boundary.
///
/// Carries an [`ExecutionIdentity`] recording producer, provider name,
/// and runtime config so audit and replay can answer *which provider,
/// with which request, produced this filing*.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecFilingPayload {
    pub filing: Filing,
    /// `embassy_pack::content_hash` of the originating request — joins
    /// this fact back to its `Observation::request_hash` for audit replay.
    pub request_hash: String,
    /// Provider that produced the filing (e.g., `stub_sec_edgar`,
    /// `live_sec_edgar`). Mirrors `Observation::vendor`.
    pub vendor: String,
    /// Wallclock latency the provider reported. Mirrors
    /// `Observation::latency_ms`. Operational, not load-bearing.
    pub latency_ms: u64,
    pub execution_identity: ExecutionIdentity,
}

impl FactPayload for SecFilingPayload {
    const FAMILY: &'static str = "embassy.sec_edgar.filing";
    const VERSION: u16 = 1;
}

/// Reads [`SecEdgarRequest`] facts from `Seeds`, dispatches them through
/// a [`SecEdgarProvider`], and proposes one [`SecFilingPayload`] per
/// returned observation to `Hypotheses`.
pub struct SecFilingSuggestor<P: SecEdgarProvider + 'static> {
    provider: Arc<P>,
}

impl<P: SecEdgarProvider + 'static> SecFilingSuggestor<P> {
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl<P: SecEdgarProvider + 'static> Suggestor for SecFilingSuggestor<P> {
    fn name(&self) -> &'static str {
        "SecFilingSuggestor"
    }

    fn dependencies(&self) -> &[ContextKey] {
        &[ContextKey::Seeds]
    }

    fn provenance(&self) -> Provenance {
        Provenance::from(SEC_EDGAR_PROVENANCE.as_str())
    }

    fn accepts(&self, ctx: &dyn Context) -> bool {
        ctx.get(ContextKey::Seeds)
            .iter()
            .any(|fact| fact.payload::<SecEdgarRequest>().is_some())
    }

    async fn execute(&self, ctx: &dyn Context) -> AgentEffect {
        let mut proposals = Vec::new();

        for seed in ctx.get(ContextKey::Seeds) {
            let Some(request) = seed.payload::<SecEdgarRequest>() else {
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
                        "SEC EDGAR fetch failed; skipping seed"
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

                let payload = SecFilingPayload {
                    filing: observation.content,
                    request_hash: observation.request_hash,
                    vendor: observation.vendor,
                    latency_ms: observation.latency_ms,
                    execution_identity,
                };

                proposals.push(
                    ProposedFact::new(
                        ContextKey::Hypotheses,
                        format!("sec-edgar:{}:{idx}", seed.id()),
                        payload,
                        Provenance::from(SEC_EDGAR_PROVENANCE.as_str()),
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
    use crate::provider::StubSecEdgarProvider;
    use crate::types::{AccessionNumber, Cik, FormType};

    fn apple_cik() -> Cik {
        Cik::parse("0000320193").unwrap()
    }

    #[test]
    fn suggestor_declares_seeds_dependency() {
        // Intent: the engine schedules Suggestors by reading
        // dependencies(). If this list ever drops Seeds, the
        // Suggestor stops being woken up when a request lands — and
        // would silently miss every request. Pin it.
        let s = SecFilingSuggestor::new(Arc::new(StubSecEdgarProvider));
        assert_eq!(s.dependencies(), &[ContextKey::Seeds]);
    }

    #[test]
    fn suggestor_provenance_is_canonical_sec_edgar() {
        // Intent: every fact this Suggestor emits must be tagged
        // `sec-edgar` for log-search joins. If a refactor returns a
        // different string, audit queries scoped to SEC will silently
        // miss new filings.
        let s = SecFilingSuggestor::new(Arc::new(StubSecEdgarProvider));
        assert_eq!(s.provenance().as_str(), "sec-edgar");
    }

    #[test]
    fn payload_family_and_version_are_stable() {
        // Intent: family + version together are the cross-version
        // contract for any downstream consumer that filters typed
        // payloads. Changing either is a payload-schema break — pin
        // the current values so the change is visible in code review.
        assert_eq!(SecFilingPayload::FAMILY, "embassy.sec_edgar.filing");
        assert_eq!(SecFilingPayload::VERSION, 1);
    }

    #[test]
    fn request_kinds_serde_round_trip() {
        // Intent: both request variants must round-trip through
        // serde because they ride the ProposedFact payload boundary.
        // A serde regression here would block Formation composition.
        let filing_req = SecEdgarRequest::Filing {
            cik: apple_cik(),
            accession_number: AccessionNumber::parse("0000320193-23-000106").unwrap(),
        };
        let latest_req = SecEdgarRequest::LatestByForm {
            cik: apple_cik(),
            form_type: FormType::Form10K,
        };
        for req in [filing_req, latest_req] {
            let json = serde_json::to_string(&req).unwrap();
            let back: SecEdgarRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_string(&back).unwrap(),
                json,
                "request variant must round-trip without re-ordering or losing fields"
            );
        }
    }
}
