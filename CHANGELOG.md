# Changelog

All notable changes to embassy will be documented in this file.

## [Unreleased]

### Added

- Workspace scaffolded 2026-05-05. Home for Converge ports — business-oriented
  integrations with specific external services (LinkedIn, OCR, etc.).
- `embassy-pack` (0.1.0) — shared port contract surface (`CallContext`,
  `Observation<T>`, `content_hash`).
- `embassy-linkedin` (0.1.0) — first port landed. Extracted from
  `organism/crates/intelligence/src/linkedin.rs`. Stub provider only;
  real backing implementation TBD.

### Changed

- Cargo packages renamed to `converge-embassy-pack` and
  `converge-embassy-linkedin`; Rust library names remain `embassy_pack` and
  `embassy_linkedin`.
