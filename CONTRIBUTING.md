# Contributing to embassy

`embassy` is a Converge extension for source-specific connector ports.

## Development

```sh
just check
just test
just lint
```

## Boundary

Use Embassy when the foreign system identity is part of the API. LinkedIn is
the first example: a `LinkedInProfile` is source-shaped data, not a generic web
search result.

Use `../manifold` for generic provider capabilities where vendors are
interchangeable.

## Adding a Port

When adding a connector:

1. Create a crate under `crates/<source>`.
2. Define request, response, and source-shaped semantic types.
3. Accept `embassy_pack::CallContext`.
4. Return `embassy_pack::Observation<T>` when returning external observations.
5. Include a stub provider for deterministic tests.
6. Document rate limits, provenance, and compliance constraints in the README.

## No `unsafe`

The workspace forbids `unsafe`.

## License

By contributing, you agree your contributions are licensed under MIT.
