---
tags: [architecture, surface]
source: mixed
---
# Surface

`embassy` exposes one canonical published crate (`embassy`)
plus optional adapter crates with adapter-qualified names.

The public surface is evidence-oriented. Embassy names external systems when
their identity is part of the observation contract, but it does not expose
business command surfaces for billing, entitlement, partner payout, signing,
CRM mutation, payroll, escrow release, or customer writeback.

## Public surface

- `embassy-pack` - shared call context, provenanced observations, and content
  hash helpers.
- source crates such as `embassy-linkedin`, `embassy-sec-edgar`,
  `embassy-bolagsverket`, and skeleton source ports for source-specific
  evidence.

## Contract dependencies

- `converge-pack` — `Pack`, `ProposedPlan`, `ProblemSpec`
- `converge-model` — semantic types
- `converge-provider` — capability identity (when applicable)

## Forbidden imports

Per [Extension Release Checklist §1](https://github.com/Reflective-Lab/converge/blob/main/kb/Standards/Extension%20Release%20Checklist.md):

- No imports of `converge-core` internals.
- No imports of foundation `runtime`, `provider`, or transport crates.
- No re-exports of foundation types except those promised stable.

## Forbidden ownership

- No Reflective Labs commercial rail contracts.
- No customer workflow, entitlement, subscription, payout, refund, or ledger
  semantics.
- No credential storage, tenant routing, deployment topology, or host policy.
- No irreversible command policy or approval placement.
