---
id: "AUDIT-PROPTEST-FOLLOWUP-TICKETS"
type: "audit_followup_backlog"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-05-15"
updated: "2026-05-26"
closed: null
owner: "Gustavo Schneiter"
final_approver: null
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "followup", "proptest", "test-density", "backlog", "wi-pipeline"]
---

# Property Test Density — Follow-up Ticket Backlog

> **Source audit:** [`2026-05-15-proptest-density.md`](2026-05-15-proptest-density.md)
> **Total gap crates remaining:** 0 — all 5 follow-up WIs CLOSED 2026-05-15
> **Closure refs:** DEBT-009 (FU-001..FU-004) + DEBT-017 (FU-005 CI gate)
> **Sprint placement:** opportunistic; pre-GA quality gate.

Each follow-up is sprint-ready: a single WI with concrete acceptance
criteria, the load-bearing INV to cover, and a pointer at the file that
houses the invariant logic.

---

## WI-PROPTEST-FU-001 — `corelink-slack-real` — Slack notify fail-CLOSED proptest — **CLOSED 2026-05-15 (DEBT-009)**

**Lane:** OBSERVABILITY · **Estimated effort:** 0.5d · **Priority:** P2

**Invariants to cover:**
- `INV-AUDIT-EMIT-ATOMIC` (slack notify fires audit BEFORE Slack POST)
- `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` (handler ack required pre-mutation)

**Acceptance criteria:**

1. New test file `crates/corelink-slack-real/tests/prop_slack_emit_atomic.rs`
   with proptest macro and `PROPTEST_CASES` runtime fn.
2. At least **2 property tests** named
   `prop_inv_audit_emit_atomic_pre_slack_post` and
   `prop_inv_audit_emit_atomic_with_handler_failclosed_blocks_slack`.
3. Adversarial fixture injects (a) a failing audit sink and (b) a
   failing slack adapter; the property asserts NO slack POST is issued
   when audit emit fails, AND NO audit record is recorded when the
   slack adapter is the only failure.
4. Quality gate: `cargo test -p corelink-slack-real` clean;
   `cargo clippy -p corelink-slack-real --tests -- -D warnings` clean.
5. Audit doc cross-link: update `2026-05-15-proptest-density.md` §3.1
   gap table to mark this crate as CLOSED.

**Source files:** `crates/corelink-slack-real/src/lib.rs`,
`crates/corelink-slack-real/src/emit.rs` (slack POST path).

**Risk if deferred:** A slack-failure regression could mask a Sev1
audit event in production; the chain-of-custody is the load-bearing
property.

---

## WI-PROPTEST-FU-002 — `corelink-admin-dry-run` — Dual-approval simulator proptest — **CLOSED 2026-05-15 (DEBT-009)**

**Lane:** ADMIN_PLANE · **Estimated effort:** 1.0d · **Priority:** P1

**Invariants to cover:**
- `INV-ADMIN-DUAL-APPROVAL` (runbook dry-run preserves dual-approval contract)

**Acceptance criteria:**

1. Lift any reusable logic out of the three `src/bin/rb_fm_*_dry_run.rs`
   binaries into a `lib.rs` (or a sibling `corelink-admin-dry-run-core`
   crate) so proptests can target the dry-run engine directly.
2. New test file `crates/corelink-admin-dry-run/tests/prop_dual_approval_invariants.rs`
   with proptest macro and `PROPTEST_CASES` runtime fn.
3. At least **3 property tests**:
   - `prop_inv_admin_dual_approval_single_approver_rejected`: a single
     approver MUST never satisfy dual-approval (1 approver, random
     timestamps + roles).
   - `prop_inv_admin_dual_approval_self_approval_rejected`: an approver
     MUST NOT also be the requester (random {requester, approver_a,
     approver_b} permutations; the case requester ∈ approvers MUST be
     rejected).
   - `prop_inv_admin_dual_approval_distinct_approvers_required`: random
     pairs `(approver_a, approver_b)` over a sampled set; identity
     `approver_a == approver_b` MUST be rejected.
4. Adversarial fixture: tie-breaker timestamps (both approvers same ms);
   case-insensitive approver-name match; UTF-8 lookalikes (`admin` vs
   `аdmin` with Cyrillic 'а').
5. Quality gate: `cargo test -p corelink-admin-dry-run` clean;
   `cargo clippy -p corelink-admin-dry-run --tests -- -D warnings` clean.

**Source files:** `crates/corelink-admin-dry-run/src/bin/rb_fm_201_dry_run.rs`,
`rb_fm_205_dry_run.rs`, `rb_fm_206_dry_run.rs`.

**Risk if deferred:** Dual-approval is the canonical privilege-escalation
guard for high-risk admin ops; a regression here is a P1 security gap.

---

## WI-PROPTEST-FU-003 — `corelink-cf-bindings` — CAS idempotency proptest — **CLOSED 2026-05-15 (DEBT-009)**

**Lane:** STORAGE · **Estimated effort:** 1.0d · **Priority:** P1

**Invariants to cover:**
- `INV-CAS-IDEMPOTENCY` (same content → same address; repeated PUT no-op)

**Acceptance criteria:**

1. New test file `crates/corelink-cf-bindings/tests/prop_cas_idempotency.rs`
   with proptest macro and `PROPTEST_CASES` runtime fn.
2. At least **3 property tests**:
   - `prop_inv_cas_idempotency_same_content_same_digest`: random byte
     buffer of length [0, 4MiB]; computing the canonical digest twice
     MUST yield identical results.
   - `prop_inv_cas_idempotency_repeated_put_observes_one_object`:
     N=[1,16] sequential PUTs of identical content; the in-memory R2
     fake's object count MUST be exactly 1.
   - `prop_inv_cas_idempotency_different_content_different_digest`:
     random pair of byte buffers; if they differ, their digests MUST
     differ (BLAKE3 collision resistance assumption pinned at the API
     boundary).
3. Adversarial inputs: empty buffer, single-byte, single-bit-flip,
   length at exact 4MiB chunk boundary, 1024-byte all-zero, 1024-byte
   all-0xFF.
4. The R2 fake binding (`InMemoryR2Bindings`) must be re-used (or
   constructed if missing) so the test is host-only-runnable
   (`cfg(not(target_arch = "wasm32"))`).
5. Quality gate: `cargo test -p corelink-cf-bindings` clean;
   `cargo clippy -p corelink-cf-bindings --tests -- -D warnings` clean.

**Source files:** `crates/corelink-cf-bindings/src/r2.rs` (CAS PUT path).

**Risk if deferred:** CAS idempotency is the foundational property of
the entire content-addressed cache; a regression silently doubles
storage cost AND breaks dedupe contracts.

---

## WI-PROPTEST-FU-004 — `corelink-d1-migrations` — Migration additivity proptest — **CLOSED 2026-05-15 (DEBT-009)**

**Lane:** STORAGE · **Estimated effort:** 0.5d · **Priority:** P2

**Invariants to cover:**
- `INV-AUTH-MIGRATION-ADDITIVE` (migrations only ADD columns/tables,
  never DROP, ALTER, or RENAME)

**Acceptance criteria:**

1. New test file `crates/corelink-d1-migrations/tests/prop_migration_additivity.rs`
   with proptest macro and `PROPTEST_CASES` runtime fn.
2. At least **2 property tests**:
   - `prop_inv_auth_migration_additive_no_drop_alter_rename`: a SQL
     mini-lexer parses every committed migration file; the property
     asserts NO statement starts with `DROP `, `ALTER COLUMN`, `RENAME `
     (case-insensitive). The corpus is the union of all `*.sql` files
     in `crates/corelink-d1-migrations/migrations/`.
   - `prop_inv_auth_migration_additive_columns_only_grow`: between any
     two adjacent migrations (M_n, M_{n+1}), the column set of every
     pre-existing table is a SUPERSET of M_n's column set.
3. Adversarial inputs: SQL comments containing the forbidden keywords
   (`-- DROP this later`) MUST NOT trigger false positives; mixed-case
   keywords (`Drop`, `dRoP`) MUST be detected; whitespace variations
   (`DROP\tTABLE`, `DROP\nTABLE`).
4. Quality gate: `cargo test -p corelink-d1-migrations` clean;
   `cargo clippy -p corelink-d1-migrations --tests -- -D warnings` clean.

**Source files:** `crates/corelink-d1-migrations/migrations/*.sql`,
`crates/corelink-d1-migrations/src/lib.rs`.

**Risk if deferred:** A non-additive D1 migration is a one-way rollback
landmine; production cutover blocked on this property in S-14.

---

## Cross-cutting WI — Density-tracking CI workflow

## WI-PROPTEST-FU-005 — CI density-gate workflow — **CLOSED 2026-05-15 (DEBT-017)**

**Lane:** TOOLING · **Estimated effort:** 0.5d · **Priority:** P3

**Goal:** Prevent ratio regression in PR-gate CI.

**Acceptance criteria:**

1. New script `scripts/audit_proptest_density.sh` (already provided
   under `/tmp/audit_proptest_final.sh`; canonicalize into `scripts/`).
2. New GitHub Actions job `proptest-density-gate` in
   `.github/workflows/ci.yml`:
   - Runs `scripts/audit_proptest_density.sh`.
   - Fails the PR if any crate listed in the canonical
     `proptest-density-allowlist.txt` regressed below ratio = 1.0.
   - Surfaces a comment on the PR showing per-crate ratio diff vs
     `origin/main`.
3. Allowlist seeded from the 4 currently-known gap crates above; each
   row carries the closing WI ID (so the allowlist shrinks as WIs land).
4. Documentation: new section in `CONTRIBUTING.md` "Property test
   density gate".

**Risk if deferred:** Without this gate, the 4 gap crates expand silently
as new crates land; the audit becomes stale within 2 sprints.

---

## Summary

| WI | Crate | Effort | Priority | INV | Status |
|---|---|---|---|---|---|
| WI-PROPTEST-FU-001 | `corelink-slack-real` | 0.5d | P2 | INV-AUDIT-EMIT-ATOMIC | **CLOSED 2026-05-15 (DEBT-009)** |
| WI-PROPTEST-FU-002 | `corelink-admin-dry-run` | 1.0d | P1 | INV-ADMIN-DUAL-APPROVAL | **CLOSED 2026-05-15 (DEBT-009)** |
| WI-PROPTEST-FU-003 | `corelink-cf-bindings` | 1.0d | P1 | INV-CAS-IDEMPOTENCY | **CLOSED 2026-05-15 (DEBT-009)** |
| WI-PROPTEST-FU-004 | `corelink-d1-migrations` | 0.5d | P2 | INV-AUTH-MIGRATION-ADDITIVE | **CLOSED 2026-05-15 (DEBT-009)** |
| WI-PROPTEST-FU-005 | CI density gate | 0.5d | P3 | — (tooling) | **CLOSED 2026-05-15 (DEBT-017)** |

**Total effort:** 3.5d. **GA-blocker:** WI-PROPTEST-FU-002 + WI-PROPTEST-FU-003
(dual-approval + CAS idempotency). The rest are post-GA quality
hardening.

## Closure summary (2026-05-15)

All 5 follow-up WIs are CLOSED. Proptests (FU-001..FU-004) landed in
DEBT-009. The CI density gate (FU-005) lands here in DEBT-017:

- `scripts/audit_proptest_density.sh` — canonical audit script (already in tree pre-DEBT-017).
- `scripts/check_proptest_density_gate.sh` — PR-gate enforcer; non-zero
  exit on any new < 1.0 ratio crate not in the allowlist.
- `scripts/proptest-density-allowlist.txt` — tolerated baseline gaps,
  each tagged with a closing follow-up WI ID. The allowlist shrinks
  as WIs land. The 4 DEBT-009 crates are RESOLVED and removed; the
  3 newly-surfaced gaps (`corelink-handler-cas`, `corelink-otel-export`,
  `corelink-tenant-offboarding`) are listed as pre-GA follow-ups.
- `.github/workflows/proptest-density-gate.yml` — PR-gate workflow.
- `CONTRIBUTING.md §"Property test density gate"` — contributor docs.

---

## Wave-33 expansion (2026-05-26) — re-opened section

Wave 33 / Wave 34 work introduced 7 new gap crates: 4 from Wave 33
Stage 1/0 (umbrella aggregators with INV refs in doc comments but no
proptests, since they are `pub use` re-export façades) + 3 pre-existing
crates surfaced when `scripts/ci.sh` first added the proptest-density
gate to the canonical pre-merge set (these were previously not measured
against the gate). All 7 are in
`scripts/proptest-density-allowlist.txt` pending closure under the
following two follow-up WIs.

### WI-PROPTEST-FU-W33-001 — Umbrella aggregator proptest accounting — **CLOSED 2026-05-26 (obsoleted by Wave 35 Phase 2 physical absorption + INV-pin documentation pattern)**

**Status:** CLOSED 2026-05-26 (W36-PROPTEST-FU-001)
**Closure SEAL:** `specs/_audits/2026-05-26-w36-proptest-fu-001-seal.md`
**Owner:** orchestrator (Gustavo Schneiter)
**Priority:** P3 (architectural decision, not runtime correctness)
**Effort:** 0.5d (executed as decision + allowlist + doc cleanup; no
script tweak required after closure narrative)

**Original crates in scope:**

- `corelink-auth` — Wave 33 Stage 1 Stream B aggregator
- `corelink-cas` — Wave 33 Stage 1 Stream A aggregator
- `corelink-container` — Wave 33 Stage 2.B (apps/server → crate)
- `corelink-core` — Wave 33 Stage 0 apex

**Original problem statement:** these umbrellas were `pub use`
re-export façades whose `src/lib.rs` carried INV references in doc
comments (for discoverability + canonical-path traceability) while the
umbrella itself had zero behaviour. The density gate "double-counted":
INV refs in doc comments inflated the denominator while 0 proptests
in the umbrella's own `tests/` directory zeroed the numerator.

**Why CLOSED 2026-05-26 (post-Wave-35-Phase-2 reality):**

The original "double-counting via `pub use` re-export" framing is
**INVALIDATED** for the 2 historically-aggregator crates, and **never
applied** to the 2 boundary/types crates:

1. `corelink-auth` — Wave 35 Phase 2 physically absorbed `schema` +
   `webauthn` into `src/schema/` + `src/webauthn/`. Their proptests
   moved into `crates/corelink-auth/tests/` (4 proptest! blocks /
   14 proptest tests / 6 INV refs). Current ratio: **2.33** — passes
   the density gate naturally. Removed from allowlist.
2. `corelink-cas` — Wave 35 Phase 2 physically absorbed
   chunker/dedup/edge/eviction/lru_tracker/manifest/meta/multipart_schema/r2_multipart.
   Their proptests moved into `crates/corelink-cas/tests/`
   (12 proptest! blocks / 72 proptest tests / 19 INV refs). Current
   ratio: **3.79** — passes the density gate naturally. Removed from
   allowlist.
3. `corelink-container` — never was a `pub use` re-export aggregator;
   it is `apps/server`'s library-half (15 src files of route/webhook/
   byok orchestrator logic). Its 3 INV refs
   (`INV-AUTH-MIGRATION-ADDITIVE`, `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`,
   `INV-TENANT-ISOLATION`) are **route-boundary pin annotations**,
   not load-bearing properties — the property tests live in
   `corelink-d1-migrations`, `corelink-audit`, and
   `corelink-tenant-path` + `corelink-auth::schema` respectively.
   Pattern: "INV-pin documentation" — exempted via allowlist with
   per-INV ownership pointer.
4. `corelink-core` — Wave-33 Stage 0 apex types crate (TenantId,
   Digest, Region, SecretWrap, CoreError, Clock). Its 1 INV ref
   (`INV-DATA-RESIDENCY`) is a policy-pin documenting how
   `types/region.rs` participates in the residency invariant; the
   property test lives in `corelink-signup` (regional-pin at signup
   path). Same "INV-pin documentation" pattern as container.

**Closure scope (this session — W36-PROPTEST-FU-001):**

- `scripts/proptest-density-allowlist.txt`: REMOVED `corelink-auth` +
  `corelink-cas` from the W33-001 section (they now pass the gate
  naturally); REWROTE the W33-001 block to document the closure
  narrative + the INV-pin-only residual (container + core) with
  per-crate ownership pointers.
- `corelink-container/src/lib.rs`: added `## INV pin map` doc block
  enumerating each pinned INV → property-test-owner crate.
- `corelink-core/src/lib.rs`: added INV pin pointer for
  `INV-DATA-RESIDENCY` → `corelink-signup` ownership.
- This ticket → CLOSED.
- `specs/_audits/2026-05-26-wave-33-34-closure-followups.md` §6 #4 →
  CLOSED.

**Acceptance criteria (one of) — selected:** option (b) ("document
the absorbed-crate ↔ INV mapping in each umbrella's `lib.rs` `//!`
doc block") + naturalized closure for the historically-aggregator
crates. Option (a) (script heuristic) deemed unnecessary because the
allowlist already serves as the exemption mechanism and the surface
shrunk to 2 crates (container + core) for which explicit per-INV
ownership documentation is more honest than a structural heuristic.

**Risk if not closed:** zero — gate currently green, no behavioural
change, no test regression.

**Tracking:** this ticket + closure SEAL audit
`specs/_audits/2026-05-26-w36-proptest-fu-001-seal.md` +
`specs/_audits/2026-05-26-wave-33-34-closure-followups.md` §6 #4.

---

### WI-PROPTEST-FU-W33-002 — Pre-existing density gaps surfaced by ci.sh

**Status:** OPEN
**Owner:** TBD (per-crate sprint assignment)
**Priority:** P3 (pre-existing; not regressions)
**Effort:** 1.5d total (0.5d per crate)

**Crates in scope:**

- `corelink-clerk-cf` — CF Worker bindings; pure-binding crate (mostly
  `#[durable_object]` wire-up). INV refs:
  `INV-AUTH-PAT-VERIFY-CONSTANT-TIME` (via `corelink-clerk` impl) +
  `INV-CLERK-JWKS-CACHE-TTL` (CF-side). Already has integration tests
  via `corelink-clerk-cf/tests/`; missing per-INV property tests.
- `corelink-statuspage-real` — Statuspage HTTPS adapter; binding crate
  with reqwest client. INV refs: `INV-STATUSPAGE-RATE-LIMIT-1-PER-5MIN`
  + `INV-STATUSPAGE-AUDIT-FAIL-CLOSED`. Has integration tests; missing
  per-INV property tests.
- `corelink-wasm` — wasm32 entry shell. INV refs:
  `INV-WASM-ENTRY-PURE-LOGIC` + `INV-WASM-NO-WAKER` (no async runtime).
  Hard to property-test from wasm32 target; consider host-side property
  tests with `wasm-bindgen-test`.

**Acceptance criteria:**

1. Each of the 3 crates lands ≥ 1 property test per INV reference (or
   documents why a property test is infeasible — e.g. wasm32 target
   constraints).
2. Each lands a brief `tests/README.md` linking INVs to their proptest
   coverage.
3. Allowlist entries removed from
   `scripts/proptest-density-allowlist.txt` as each crate lands.

**Risk if deferred:** none short-term (the allowlist tolerates the
gap). Long-term, the gate's signal weakens if too many crates
permanently allowlist.

---

## Summary (Wave-33 expansion)

| WI | Scope | Effort | Priority | Status |
|---|---|---|---|---|
| WI-PROPTEST-FU-W33-001 | Umbrella aggregator double-counting fix | 0.5d | P3 | **CLOSED 2026-05-26** (W36-PROPTEST-FU-001; obsoleted by Wave 35 Phase 2 absorption + INV-pin doc pattern) |
| WI-PROPTEST-FU-W33-002 | 3 pre-existing density gaps | 1.5d | P3 | **OPEN** (2026-05-26) |

**Total Wave-33 expansion effort:** 2.0d. **GA-blocker:** none —
both are post-GA quality hardening. Closure target: Wave 36 tooling
pass (post-Wave-35 adapter consolidation).

The audit's overall `audit_status` remains `ACTIVE` while FU-W33-002
remains open; will return to `CLOSED` when FU-W33-002 lands.
