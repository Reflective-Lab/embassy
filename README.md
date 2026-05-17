# embassy

[![CI](https://github.com/Reflective-Lab/embassy-ports/actions/workflows/ci.yml/badge.svg)](https://github.com/Reflective-Lab/embassy-ports/actions/workflows/ci.yml)
[![Coverage](https://github.com/Reflective-Lab/embassy-ports/actions/workflows/coverage.yml/badge.svg)](https://github.com/Reflective-Lab/embassy-ports/actions/workflows/coverage.yml)
[![Security](https://github.com/Reflective-Lab/embassy-ports/actions/workflows/security.yml/badge.svg)](https://github.com/Reflective-Lab/embassy-ports/actions/workflows/security.yml)
[![Stability](https://github.com/Reflective-Lab/embassy-ports/actions/workflows/stability.yml/badge.svg)](https://github.com/Reflective-Lab/embassy-ports/actions/workflows/stability.yml)
[![Crates.io](https://img.shields.io/crates/v/converge-embassy-pack.svg)](https://crates.io/crates/converge-embassy-pack)
[![docs.rs](https://docs.rs/converge-embassy-pack/badge.svg)](https://docs.rs/converge-embassy-pack)
[![dependency status](https://deps.rs/repo/github/Reflective-Lab/embassy-ports/status.svg)](https://deps.rs/repo/github/Reflective-Lab/embassy-ports)
![MSRV](https://img.shields.io/badge/MSRV-1.94.0-blue)
<img alt="gitleaks badge" src="https://img.shields.io/badge/protected%20by-gitleaks-blue">
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Source-specific connector ports for Converge extensions.

`embassy` is where integrations live when the external system identity is part
of the semantic contract. LinkedIn is not just "a search provider"; it carries
source-specific identity, terms, rate limits, provenance, and business meaning.
That source-shaped contract belongs here.

Embassy is not the home for operating business authority. A source-specific
system can be named in an Embassy contract when the system is being observed as
evidence. When the integration changes business state, moves money, grants
entitlements, signs agreements, mutates CRM records, updates payroll, or
creates customer or partner obligations, the owning business layer must define
the command surface and policy. Embassy may still provide source-faithful
observations, but it must not own the business action.

Cargo packages: `converge-embassy-pack` and `converge-embassy-linkedin`. Rust
library names remain `embassy_pack` and `embassy_linkedin`.

## Why It Exists

Converge should not own every external business connector. Embassy gives those
connectors a stable home without hiding important source semantics behind a
generic provider interface.

## What Embassy Owns

- Shared connector call context.
- Provenanced observations.
- Source-specific request and response types.
- Source-specific provider traits.
- Stub providers for deterministic tests.

## What Embassy Does Not Own

- Reflective Labs billing, subscriptions, partner payouts, marketplace terms,
  entitlements, or revenue sharing.
- Customer business workflows, domain Truth catalogs, projections, or writeback
  policy.
- Irreversible command policy, approval placement, ledger posting, webhook
  replay protection, tenant secret storage, or runtime deployment topology.
- Generic provider capabilities such as fetch, search, storage, vector recall,
  LLM calls, or interchangeable tool execution.

## Embassy vs Manifold

| Question | Embassy | Manifold |
|---|---|---|
| Is the source identity part of the type? | Yes | No |
| Can another vendor be swapped in behind the same contract? | Usually no | Yes |
| Example | `LinkedInProfile` | Vector search, object storage, web fetch |
| Captures | Business semantics, source constraints, provenance | Generic capability behavior |

If the contract must name the external system, use Embassy. If the caller only
needs a generic capability, use `../manifold`.

If the integration acts with business authority, use the owning product,
customer, or Reflective operating layer. For example, an SEC filing observation
may belong in Embassy, but `PartnerPayout`, `Subscription`,
`EntitlementGrant`, and `RevenueShareAgreement` belong in Reflective Commerce
Rails because Reflective bears the commercial consequence.

## Repository Layout

```text
crates/pack/
  src/lib.rs       CallContext, Observation<T>, content_hash

crates/linkedin/
  src/lib.rs       LinkedInProvider, request/response types, stub provider
```

Future ports should follow the same shape: source-specific contract first,
provider implementations behind it.

## Usage

```rust
use embassy_linkedin::{LinkedInGetRequest, LinkedInProvider, StubLinkedInProvider};
use embassy_pack::CallContext;

let provider = StubLinkedInProvider;
let request = LinkedInGetRequest::new("/people/search");
let response = provider.get(&request, &CallContext::default())?;
```

## Development

```sh
just check
just test
just lint
just doc
```

Embassy currently has no Converge dependency. Add Converge contracts only when
a port needs to emit a suggestor, proposal, or other Converge-shaped artifact.

## Project Files

- [AGENTS.md](AGENTS.md) - agent entrypoint and boundary rules.
- [CHANGELOG.md](CHANGELOG.md) - release notes.
- [CONTRIBUTING.md](CONTRIBUTING.md) - contribution guide.
- [SECURITY.md](SECURITY.md) - vulnerability reporting and operator notes.
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) - community expectations.

## Status

Scaffolded on 2026-05-05. `embassy-linkedin` is the first extracted port.

## License

MIT - see [LICENSE](LICENSE).
