---
type: "ADR"
title: "ADR-0035 — Action Cache handler invariants (REAPI v2)"
description: "The eight canonical invariants every REAPI v2 GetActionResult/UpdateActionResult handler must preserve regardless of caching, batching, or async optimization."
source_files:
  - "specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "action-cache", "reapi", "handlers", "invariants", "s04"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0035 — Action Cache handler invariants (REAPI v2)

The Action Cache read/write handlers (`GetActionResult` / `UpdateActionResult`) are a security and correctness surface that future optimization passes — caching, batching, driver swaps — must never erode. This ADR locks down the eight invariants that define correct handler behaviour so each one maps to a property test and a chaos scenario, and a refactor that preserves them preserves behaviour.

# Context

S-04 introduces the REAPI v2 ActionCache handlers, and the spec must lock down the invariants handlers MUST preserve regardless of optimization, validated by property tests (10k iter PR / 100k nightly), a chaos suite, and a minimum-10-test REAPI v2 conformance set (ADR-0035:23-29).

# Decision

Eight canonical invariants are declared: H-1/H-2 require signature verification on every read and pre-INSERT on every write (never serve or persist unverified bytes); H-3 reads `tenant_prefix` from the persisted `ac_meta` column rather than recomputing it from the tenant key at GET time; H-4 extracts `tenant_id` only from the Tower-middleware `TenantCtx`, never from body/query/headers; H-5 makes the `ac_meta`+`audit_outbox` D1 batch atomic so an audit miss is impossible under success; H-6 caps the negative-cache window at 60 s KV TTL; H-7 runs a 1%-sampled output-reachability check that warns but never fails the GET; and H-8 makes `UpdateActionResult` idempotent on its identity tuple (ADR-0035:31-61).

# Consequences

Each invariant becomes individually testable and a mass refactor (e.g. swapping the sqlx driver) preserves behaviour if the invariants still pass, with the REAPI v2 conformance gate verifying a subset — the only noted cost being the arbitrary 1% H-7 sampling rate flagged for revisit at the S-06 reconcile lessons (ADR-0035:63-73). It governs the [Action Cache surface](/surfaces/action-cache.md) and the [Bazel REAPI v2 surface](/surfaces/bazel-reapi.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md:23-29` — the AC handlers and the property/chaos/conformance validation regime (Context).
2. `specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md:31-61` — the eight canonical handler invariants H-1 through H-8 (Decision).
3. `specs/03_architecture/adrs/ADR-0035-ac-handler-invariants.md:63-73` — per-invariant testability and refactor safety vs the arbitrary 1% sampling rate (Consequences).
