# Changelog

All notable changes to embassy will be documented in this file.

## [Unreleased]

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
