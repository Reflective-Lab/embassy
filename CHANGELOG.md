# Changelog

All notable changes to embassy will be documented in this file.

## [Unreleased]

## [1.3.0] - 2026-05-17

### Added

- Nine P0 ports answering the executive business questions: `gleif`,
  `vies`, `ofac-sls`, `eu-sanctions`, `commerce-csl`, `sam-gov`,
  `usaspending`, `ted`, `skatteverket`. Sanctions trio uses a
  coherent `SanctionsHit` shape across all three sources.
- `sec-edgar` `live` feature: real transport lifted from fathom-sparc.
- Ten skeleton ports (uspto, crunchbase, github, pubmed, arxiv,
  openalex, wikidata, companies-house, scb, epo).
- Suggestor wrappers across all eleven ports; LinkedIn async path.

### Changed

- Workspace floor: Converge 3.9.1.
- First release where all five `just release-check` gates pass cleanly
  before tagging.

## [1.1.1] - 2026-05-15

### Changed

- Aligned workspace and internal path dependency versions for the coordinated
  extension release. No public API changes.

## [1.1.0] - 2026-05-07

### Added

- Unit tests for `embassy-pack` (`content_hash` properties, `CallContext`
  defaults, `Observation` JSON round-trip) and `embassy-linkedin`
  (`LinkedInGetRequest::new`, `StubLinkedInProvider`).
- `serde_json` as a dev-dependency for `embassy-pack` round-trip tests.

### Changed

- Cargo packages renamed to `converge-embassy-pack` and
  `converge-embassy-linkedin`; Rust library names remain `embassy_pack` and
  `embassy_linkedin`.
- Adopted Converge 3.8.1 baseline.

## [1.0.0] - 2026-05-05

### Added

- Workspace scaffolded 2026-05-05. Home for Converge ports — business-oriented
  integrations with specific external services (LinkedIn, OCR, etc.).
- `embassy-pack` (0.1.0) — shared port contract surface (`CallContext`,
  `Observation<T>`, `content_hash`).
- `embassy-linkedin` (0.1.0) — first port landed. Extracted from
  `organism/crates/intelligence/src/linkedin.rs`. Stub provider only;
  real backing implementation TBD.
