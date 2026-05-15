---
type: followup_backlog
title: TLA+ coverage — remaining gap tickets
parent_audit: 2026-05-15-tla-coverage-audit.md
created: 2026-05-15
owner: TBD (post-R-PREP)
---

# TLA+ followup tickets

Backlog of TLA+ coverage gaps NOT closed by the R-PREP delivery
(`auth_revocation.tla`). Ordered by impact-to-effort ratio. Each ticket is
intended to be turned into a GitHub issue when the owning sprint picks it up.

The R-PREP delivery already closed 3 CRITICAL + 1 HIGH (the auth-revocation
cluster). The 8 remaining tickets below cover 1 CRITICAL + 6 HIGH + 1
infrastructure follow-up.

## FT-1 — `auth_pat_hybrid.tla` (CRITICAL)

- **Severity:** CRITICAL
- **Invariants:** INV-AUTH-PAT-HMAC-SIG-VERIFIED (CRITICAL),
  INV-AUTH-PAT-VERIFY-CONSTANT-TIME, INV-AUTH-PAT-HASH-ARGON2ID-2024
- **Domain:** AUTH — PAT verify hybrid path (HMAC fast-fail + indexed lookup +
  Argon2id).
- **Why TLA+:** state machine over verify-path steps 3a/3b/3c MUST short-
  circuit BEFORE DB hit on HMAC mismatch (phishing + DDoS defense). A trace
  where step 3b fires while 3a is still pending is a P0 vulnerability.
- **Cost:** ~3 days. Model uses 3 abstract phases + adversarial sig-mismatch
  action; bounded state space.
- **Owner:** Auth working group (post-S-03 hardening cycle).

## FT-2 — `key_lifecycle.tla` (HIGH × 3)

- **Severity:** HIGH × 3 + CRITICAL co-located
- **Invariants:** INV-KEY-NO-SKIP, INV-KEY-OVERLAP, INV-ADMIN-DUAL-APPROVAL
  (HIGH × 3); implicitly covers INV-KEY-AUDIT via audit-chain inheritance.
- **Domain:** KEY — per-asset rotation state machine with overlap windows
  (PAT 24 h / audit 24 h / TDK 7 d / BYOK 7 d / Ed25519 30 d).
- **Why TLA+:** rotation states (`pending → active → rotated → retired →
  destroyed`) with overlap window — writes during overlap MUST succeed under
  either key version; INV-KEY-NO-SKIP MUST reject writes against retired keys.
- **Cost:** ~5 days. CONSTANTS per asset class; rotation worker actions +
  caller actions; explicit overlap window counter.
- **Owner:** S-13 sprint owner (planned in `invariant_registry.md §4.2`).

## FT-3 — TLC jar SHA-256 pin drift

- **Severity:** infrastructure / CI hygiene
- **Issue:** the pinned SHA in `.github/workflows/tla_check.yml` and
  `scripts/run_tlc_corelink.sh`
  (`d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`) does
  NOT match the current asset at
  `https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar`
  (verified 2026-05-15: actual
  `25780ac9578e1ec356ff423f7f1cf70b0d663f2af8b83ae99745701dcdf299b0`).
- **Implication:** the install step in `tla_check.yml` will FAIL on every PR
  until the pin is updated OR the upstream re-publishes the old asset.
- **Action:** Security review + ADR-0042 §A1 update per the pin-rotation
  protocol, then bump the constant in both files.
- **Owner:** Security WG + Architect.

## FT-4 — `.cfg` function-literal parser brittleness

- **Severity:** infrastructure / spec hygiene
- **Issue:** TLC v1.8.0 (and newer 2026.05.12 nightly) reject the `[k |-> v]`
  syntax for function-valued constants in `.cfg` files. `region_residency.cfg`
  uses this for `PrimaryRegionOf` and DOES NOT PARSE on either tested TLC
  build — this audit's local TLC run confirmed both `auth_revocation.cfg`
  (before the workaround) and `region_residency.cfg` fail with
  `ConfigFileException`.
- **Action:** either (a) move `PrimaryRegionOf` to a TLA-level operator
  derived from `Tenants`/`Regions` set sizes (same pattern this audit's
  delivery uses for `PatTenant`), or (b) replace with CONSTANT-override via
  `<-` operator binding in cfg. Option (a) preferred.
- **Owner:** S-14 owner (region_residency is theirs).

## FT-5 — `gc_lock_protocol.tla` (HIGH)

- **Severity:** HIGH
- **Invariants:** INV-GC-SINGLE-RUNNING-PER-TENANT-REGION, INV-GC-RECONCILE-
  AUDIT-FAIL-CLOSED (CRITICAL — currently only inherits from audit_immutability).
- **Domain:** GC — distributed lock protocol (single GC worker per
  tenant×region under leader handoff + DO crashes).
- **Why TLA+:** lock-protocol-only invariant in code; no formal proof of
  no-double-run under leader handoff race + checkpoint-resume sequence.
- **Cost:** ~3 days. Lock state per (tenant, region) with crash + handoff
  adversarial actions.
- **Owner:** S-06 GC owner.

## FT-6 — `multipart_finalize.tla` (HIGH)

- **Severity:** HIGH
- **Invariants:** INV-MULTIPART-FINALIZE-IRREVOCABLE,
  INV-MULTIPART-STATE-MONOTONIC, INV-MULTIPART-CONCURRENCY-BOUNDED.
- **Domain:** MULTIPART — session state machine
  (`live → finalized | aborted`) with concurrent abort+finalize race.
- **Why TLA+:** today covered only by property tests (`prop_abort_safety`
  10k iter); a state-machine model would also prove `finalize → abort`
  rejection AND `abort → finalize` rejection AND idempotent re-finalize.
- **Cost:** ~2 days. Small state machine; one tricky race (abort + finalize
  arriving simultaneously).
- **Owner:** S-05 multipart owner.

## FT-7 — `byok_dek_race.tla` (CRITICAL upgrade)

- **Severity:** CRITICAL upgrade (currently runbook-level only)
- **Invariants:** INV-BYOK-CRYPTO-SOVEREIGNTY (CRITICAL).
- **Domain:** KEY / BYOK — DEK cache evict race across regions when KMS is
  disabled. The runbook spec `byok_kill_switch.tla` admits the invariant
  but treats the DEK cache as a single global variable. Real production
  has per-region DEK caches with independent eviction; TLA+ should model
  the race.
- **Cost:** ~4 days. Add per-region cache + region-local KMS check.
- **Owner:** S-14 BYOK owner.

## FT-8 — CI matrix exposure for runbook specs

- **Severity:** medium (governance)
- **Issue:** the 4 runbook specs under
  `specs/03_architecture/tla+/runbooks/` (byok_kill_switch, dpa_versioning_
  grace, residency_failover, signup_atomic) are not in any PR-gate CI
  workflow today. They cover CRITICAL invariants (INV-ONBOARD-DPA-FIRST,
  INV-DATA-RESIDENCY failover side, INV-BYOK-CRYPTO-SOVEREIGNTY, consent
  proof per dpa_version).
- **Action:** add a `tla_runbooks_check.yml` workflow mirroring
  `tla_check.yml` structure, pinned to the runbook directory.
- **Cost:** ~1 day.
- **Owner:** R-PREP infra team.

## FT-9 — re-signup idempotency in `signup_atomic.tla`

- **Severity:** HIGH
- **Issue:** the existing runbook spec covers crash-and-compensate but does
  NOT model re-signup with the same email after a compensated failure.
  A bug class here: Stripe webhook arriving AFTER compensation but before
  the user retries.
- **Action:** extend the existing spec with a `ReSignupSameEmail` action;
  no new spec file needed.
- **Cost:** ~1 day.
- **Owner:** S-19 onboarding owner.

---

## Summary

| Ticket | Severity | Spec | Sprint owner |
|---|---|---|---|
| FT-1 | CRITICAL | `auth_pat_hybrid.tla` | Auth WG |
| FT-2 | HIGH × 3 | `key_lifecycle.tla` | S-13 |
| FT-3 | infra | TLC SHA pin drift | Security + Architect |
| FT-4 | infra | .cfg function literal | S-14 |
| FT-5 | HIGH | `gc_lock_protocol.tla` | S-06 |
| FT-6 | HIGH | `multipart_finalize.tla` | S-05 |
| FT-7 | CRITICAL | `byok_dek_race.tla` | S-14 |
| FT-8 | medium | runbook CI exposure | R-PREP |
| FT-9 | HIGH | `signup_atomic.tla` extension | S-19 |

Total followup count: **9** (1 CRITICAL net-new + 1 CRITICAL upgrade + 5 HIGH
+ 2 infra/medium). Tracks the gaps from §4 of the parent audit
(`2026-05-15-tla-coverage-audit.md`).

---

## Addendum 2026-05-15 — DEBT-005 partial closure (5 / 40)

A separate dispatch (DEBT-005 row in
`specs/_audits/2026-05-15-debt-register.md`) targeted the canonical-
consistency-baseline `critical_no_tla` counter (40 → 35) by writing **5
net-new TLA+ specs**. These are not the FT-1..FT-9 backlog above; they
close CRITICAL INVs registered as `(planned ...)` in
`invariant_registry.md §3.x` but distinct from the auth-revocation +
backlog clusters tracked here.

| Spec (new) | Invariants closed (CRITICAL) | Status |
|---|---|---|
| `specs/tla/auth_jwt_validation.tla` | INV-AUTH-JWT-VALIDATE-RS256-ONLY, INV-AUTH-ISS-EXACT-MATCH | **CLOSED** — PR + nightly cfg + CI matrix |
| `specs/tla/tenant_ctx_propagation.tla` | INV-AUTH-TENANTCTX-IMMUTABLE, INV-AUTH-5-LAYER-ORDERING | **CLOSED** — PR + nightly cfg + CI matrix |
| `specs/tla/merkle_integrity.tla` | INV-AC-MERKLE-VALID, INV-MULTIPART-MANIFEST-VALID | **CLOSED** — PR + nightly cfg + CI matrix |
| `specs/tla/byok_envelope_aad.tla` | INV-BYOK-CRYPTO-SOVEREIGNTY (AAD-binding + cache-TTL liveness) | **CLOSED** — PR + nightly cfg + CI matrix |
| `specs/tla/failover_no_split_brain.tla` | INV-REGION-NO-CROSS-LEAK (write-lease handoff side) | **CLOSED** — PR + nightly cfg + CI matrix |

Net effect on the audit counters:

- `tla_verified` floor: 18 → **23** (canonical-consistency-baseline §4
  BASELINE ratchet updated in same commit).
- `critical_no_tla` floor: 40 → **35**.

The FT-1..FT-9 backlog above is unchanged — its 9 tickets remain open
and tracked under DEBT-014 in the debt register.

