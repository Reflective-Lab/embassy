---
tags: [architecture, authority, connectors]
source: mixed
---
# Operating Authority Boundary

Embassy owns source-specific evidence ports. It does not own operating business
authority.

## Decision

Use Embassy when a named external source is being observed and the source
identity is part of the evidence.

Do not use Embassy when the integration acts on behalf of Reflective Labs, a
customer, a partner, or a user in a way that changes business state.

## Embassy Evidence

Embassy can model:

- public filings and registries,
- sanctions and screening lists,
- patent and trademark sources,
- professional profile sources,
- public procurement and statistics sources,
- other external reference sources when the business only needs
  source-faithful evidence.

These contracts should produce observations with provenance, source terms,
freshness, content hashes, and source-specific identifiers.

## Operating Authority

Operating authority belongs above Embassy when an interaction can:

- move money,
- grant or revoke an entitlement,
- create a subscription or partner obligation,
- sign or countersign a document,
- release escrow,
- mutate CRM, ERP, accounting, HR, support, or identity state,
- create a legally or commercially relied-on record,
- trigger customer-visible writeback.

Those surfaces need command types, idempotency keys, audit events, replay
protection, policy gates, tenant scoping, failure semantics, and usually ACID
persistence. They are business systems, not evidence ports.

## Placement

| Concern | Owner |
|---|---|
| Source-specific external evidence | Embassy |
| Interchangeable fetch/search/storage/tool/provider capability | Manifold or Converge ToolRegistry |
| Customer-owned business action | Customer app, engagement, or deployment boundary |
| Reflective-owned billing, entitlements, partner payouts, marketplace terms | Reflective Commerce Rails |
| Product domain workflow and writeback policy | Marquee app or customer app |
| Approval, policy, authority gates | Helms plus Arbiter, depending on scope |

## Examples

`SecCompanyFiling` can be an Embassy observation because the source is being
used as external evidence.

`PartnerPayout`, `Subscription`, `EntitlementGrant`, `RefundDecision`, and
`RevenueShareAgreement` are not Embassy contracts. They are Reflective Commerce
Rails contracts because Reflective owns the commercial consequence.

`SalesforceAccountSnapshot` can be source evidence. Updating the opportunity,
assigning an owner, or advancing a sales stage is customer operating authority
and belongs in the customer application boundary.

## Rule

If the system only observes a named source, Embassy may own it.

If the system acts with business authority, the business owner must own the
contract. Embassy can inform the action, but it must not become the place where
business decisions, commercial terms, or irreversible writeback policy hide.
