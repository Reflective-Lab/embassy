# embassy Agent Guide

This is the canonical agent entrypoint for `embassy`.

`embassy` is a Converge extension for source-specific connector ports where
the external service identity is part of the semantic contract.

## Start Here

1. Read `README.md`.
2. Read `/Users/kpernyer/dev/extensions/kb/Modules/Embassy.md`.
3. Read `/Users/kpernyer/dev/extensions/kb/Architecture/Port Provider Boundary.md`.
4. Use `just --list` for local commands.

## Commands

```bash
just check
just test
just lint
just doc
```

## Boundaries

- Use Embassy for source-shaped contracts such as LinkedIn.
- Use Manifold for interchangeable generic provider capabilities.
- Keep product credentials, runtime wiring, and deployment topology out of this
  repository.

## Rules

- Preserve `unsafe_code = "forbid"`.
- Connector observations must carry provenance.
- Do not hide source-specific legal, identity, or provenance semantics behind a
  generic capability.
- Update `README.md`, `CHANGELOG.md`, and the extensions KB when a new port
  lands.
