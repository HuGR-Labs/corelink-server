# Ownership wave 004 — handlers and CAS

This wave uses the four-artifact contract in
[WAVE_001_PLAN.md](WAVE_001_PLAN.md#frozen-artifact-contract). Authoring is
source/static only: no Cargo execution, network, remote/provider operation,
publication, shared registry/index/status edits, or self-review.

## Conflict-free slots

| WP | Package | Manifest | Profile | Owned paths |
|---|---|---|---|---|
| W004-HAC | `corelink-handler-ac` | `crates/corelink-handler-ac/Cargo.toml` | S | `own-corelink-handler-ac`, `crates/corelink-handler-ac/` |
| W004-HADMIN | `corelink-handler-admin` | `crates/corelink-handler-admin/Cargo.toml` | S | `own-corelink-handler-admin`, `crates/corelink-handler-admin/` |
| W004-HCAS | `corelink-handler-cas` | `crates/corelink-handler-cas/Cargo.toml` | S | `own-corelink-handler-cas`, `crates/corelink-handler-cas/` |
| W004-HERASE | `corelink-handler-cas-erase` | `crates/corelink-handler-cas-erase/Cargo.toml` | S | `own-corelink-handler-cas-erase`, `crates/corelink-handler-cas-erase/` |
| W004-HCUSTOMER | `corelink-handler-customer` | `crates/corelink-handler-customer/Cargo.toml` | S | `own-corelink-handler-customer`, `crates/corelink-handler-customer/` |
| W004-CAS | `corelink-cas` | `crates/corelink-cas/Cargo.toml` | H | `own-corelink-cas`, `crates/corelink-cas/` |

Each author creates only its ownership skill and three package documents. The
package name is authoritative; a directory name is never silently substituted.
The lead owns cross-package reconciliation, review, integration, registry,
index, status and all publication gates.

## Static anchors

- **Handler AC:** lookup/update traits, in-memory fake, SLO observer and audit
  envelope are local source surfaces. `corelink-slo` is a declared dependency;
  actual HTTP/CF Worker, telemetry delivery and audit persistence are unknown.
- **Handler admin:** read/mutate traits, ledger, audit and observer form one
  package. Verify dual-approval conditions in source; do not imply real
  principal identity, approval authority, control-plane route or provider use.
- **Handler CAS:** read/write traits, digest algorithm, request, observer and
  audit are local. The fake's digest behavior is not a claim about object
  storage, REAPI, HTTP mounting or wasm execution.
- **Handler CAS erase:** pure request/tombstone/idempotent erase logic only.
  R2 delete and D1 tombstone transport are explicitly external composition in
  the container and must not be claimed as implementation ownership or
  observed behavior.
- **Handler customer:** six dashboard surfaces, request modules, audit query,
  observer and fake comprise package territory. No customer data, control-plane
  route, identity, billing, key or audit backend is observed.
- **CAS:** hybrid aggregator with absorbed modules (chunker, dedup, edge,
  LRU, manifest, multipart schema) plus re-exports and worker surfaces. Every
  artifact must separate local implementation, public canonical path, upstream
  provider/implementation ownership, composition roots and runtime operators.
  Re-export and workspace dependency never prove migration, target selection,
  storage reachability or runtime use.

## Acceptance and review

The four declared profile checks plus scope-only diff are author prerequisites.
A fresh independent reviewer returns four separate verdicts and challenges
package identity, trait/fake versus provider distinctions, audit ordering,
target/feature claims, re-export ownership and unproved runtime boundaries.
Changed bytes require a fresh appropriate re-review. Integration is not a
standard freeze, runtime certification or issue-publication authorization.
