# embassy

Source-specific connector ports for Converge extensions.

`embassy` is where integrations live when the external system identity is part
of the semantic contract. LinkedIn is not just "a search provider"; it carries
source-specific identity, terms, rate limits, provenance, and business meaning.
That source-shaped contract belongs here.

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

## Embassy vs Manifold

| Question | Embassy | Manifold |
|---|---|---|
| Is the source identity part of the type? | Yes | No |
| Can another vendor be swapped in behind the same contract? | Usually no | Yes |
| Example | `LinkedInProfile` | Vector search, object storage, web fetch |
| Captures | Business semantics, source constraints, provenance | Generic capability behavior |

If the contract must name the external system, use Embassy. If the caller only
needs a generic capability, use `../manifold`.

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
