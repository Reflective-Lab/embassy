> **Archived 2026-07-02** — active milestone tracking moved to Linear (Reflective team).
> This file is kept for historical context only. Do not add new items here.

---
source: mixed
---
# Milestones

> See `~/dev/reflective/stack/bedrock-platform/EPIC.md` for the coarse-grained outcomes these milestones advance.

## Shipped: v1.3.0 — Converge 3.9.1 alignment + P0 evidence cluster

**Date:** 2026-05-17 | **Tracks:** Converge 3.9.1

This is the first embassy release tagged after a clean `just release-check`
run across all five gates (security-audit, coverage, performance-profile,
soak, lint). Earlier 1.x tags shipped without that gate ever passing
cleanly — see "Historical" below for the honest reconciliation.

- Raised Converge floor to `3.9.1` (re-exports + polars_bridge surface).
- Landed nine P0 port skeletons answering the five executive business
  questions in `kb/Planning/EXECUTIVE_ROADMAP` terms:
  - Counterparty identity: `gleif` (LEI, ISO 17442 check digits),
    `vies` (EU VAT validation with `consultation_number` audit hook).
  - Sanctions trio with a coherent `SanctionsHit` shape across three
    sources: `ofac-sls` (US Treasury OFAC), `eu-sanctions` (EU
    Consolidated Financial Sanctions List), `commerce-csl` (US Commerce
    BIS Consolidated Screening List).
  - US federal counterparty + spend: `sam-gov` (UEI +
    `ContractorRegistration` Active/Expired/Inactive paths),
    `usaspending` (`FederalAward`, obligated amount as i64 micros).
  - Procurement + tax: `ted` (EU Tenders Electronic Daily,
    sequence-year notice id form), `skatteverket` (F-skatt / VAT /
    employer booleans only — explicit legal scoping on the publicly
    queryable surface).
- Encoded the Embassy-vs-Commercial-Rail boundary in KB: Embassy ports
  *observe* named sources; anything that moves money, signs, or
  mutates business state belongs in the Reflective Commerce Rails
  layer above.
- Relaxed the gleif LEI validator to match ISO 17442-1:2020
  (the 2012 "chars 5-6 reserved as `00`" rule was dropped; real LEIs
  like Apple's `HWUPKR0MPOU8FGXBT394` use non-zero chars there). Six
  gleif tests that asserted real-world LEIs now pass; the obsolete
  `lei_rejects_missing_reserved_zeros` test was deleted because the
  rule it asserted no longer exists.
- Added the standard converge-extension RUSTSEC ignore list to
  `security-audit` (incl. RUSTSEC-2025-0057 for fxhash, reached
  transitively via scraper → manifold-adapters). Brings embassy in
  lockstep with manifold's ignore policy.

## Historical reconciliation

Earlier milestone entries claimed v1.0.0 was "in progress" with
unchecked items, but the repo had already moved past v1.0.0, v1.1.0,
and v1.1.1 tags. The honest record:

- **v1.0.0 (2026-05-05)** — initial scaffold tagged; Extension Release
  Checklist adopted as the engineering bar but never run cleanly end
  to end at that point.
- **v1.1.0 (2026-05-07)** — converge-prefixed crate names; Converge
  3.8.1 baseline; first port (`linkedin`) and shared pack landed.
  Coverage + stability badges surfaced on README.
- **v1.1.1 (2026-05-15)** — coordinated-extension version alignment;
  no public API changes. Tagged but never published to crates.io.
- **v1.2.0 (intermediate, untagged)** — sec-edgar + bolagsverket as
  the first two Suggestor-emitting embassies; ten additional port
  skeletons (uspto, crunchbase, github, pubmed, arxiv, openalex,
  wikidata, companies-house, scb, epo); Suggestor wrappers across
  all eleven ports; sec-edgar `live` feature lifting transport from
  fathom-sparc.

The v1.3.0 release above is the first to satisfy every gate in the
Extension Release Checklist before tagging.
