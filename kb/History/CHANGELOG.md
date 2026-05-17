---
source: mixed
---
# Changelog

All notable changes to `embassy-ports` are recorded here.

## [1.3.0] — 2026-05-17

### Added

- Nine P0 ports answering the executive business questions:
  - `gleif` (LEI registry, ISO 17442 mod-97-10 check digits).
  - `vies` (EU VAT validation, `consultation_number` audit hook).
  - `ofac-sls`, `eu-sanctions`, `commerce-csl` — sanctions trio with
    a coherent `SanctionsHit` shape across all three sources.
  - `sam-gov` (UEI + `ContractorRegistration` Active/Expired/Inactive).
  - `usaspending` (`FederalAward`, obligated amount as i64 micros).
  - `ted` (EU Tenders Electronic Daily, sequence-year notice id form).
  - `skatteverket` (F-skatt / VAT / employer booleans, publicly
    queryable surface only).
- `sec-edgar` `live` feature: real transport lifted from fathom-sparc.
- Ten port skeletons (uspto, crunchbase, github, pubmed, arxiv,
  openalex, wikidata, companies-house, scb, epo) with typed
  identifier + entity placeholder + Provider trait + stub.
- Suggestor wrappers across all eleven ports; LinkedIn async path.

### Changed

- Workspace floor: Converge 3.9.1 (kernel re-exports + polars_bridge).
- KB now encodes the Embassy-vs-Commercial-Rail boundary explicitly.
- First release where all five `just release-check` gates pass cleanly
  (security-audit, coverage, performance-profile, soak, lint) before
  tagging.

## [1.1.1] — 2026-05-15

- Coordinated-extension version alignment. No public API changes.
  Tagged but not published to crates.io.

## [1.1.0] — 2026-05-07

### Added

- Unit tests for `embassy-pack` (`content_hash` properties,
  `CallContext` defaults, `Observation` JSON round-trip) and
  `embassy-linkedin` (`LinkedInGetRequest::new`,
  `StubLinkedInProvider`).
- `serde_json` as a dev-dependency for `embassy-pack` round-trip tests.

### Changed

- Cargo packages renamed to `converge-embassy-pack` and
  `converge-embassy-linkedin`; Rust library names remain `embassy_pack`
  and `embassy_linkedin`.
- Adopted Converge 3.8.1 baseline.

## [1.0.0] — 2026-05-05

### Added

- Workspace scaffolded 2026-05-05. Home for Converge ports —
  business-oriented integrations with specific external services
  (LinkedIn, OCR, etc.).
- `embassy-pack` (0.1.0) — shared port contract surface
  (`CallContext`, `Observation<T>`, `content_hash`).
- `embassy-linkedin` (0.1.0) — first port landed. Extracted from
  `organism/crates/intelligence/src/linkedin.rs`. Stub provider only;
  real backing implementation TBD.
