---
id: "AUDIT-PROPTEST-FOLLOWUP-TICKETS"
type: "audit_followup_backlog"
doc_status: "ACTIVE"
audit_status: "OPEN"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: null
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "followup", "proptest", "test-density", "backlog", "wi-pipeline"]
---

# Property Test Density — Follow-up Ticket Backlog

> **Source audit:** [`2026-05-15-proptest-density.md`](2026-05-15-proptest-density.md)
> **Total gap crates remaining:** 4
> **Sprint placement:** opportunistic; pre-GA quality gate.

Each follow-up is sprint-ready: a single WI with concrete acceptance
criteria, the load-bearing INV to cover, and a pointer at the file that
houses the invariant logic.

---

## WI-PROPTEST-FU-001 — `corelink-slack-real` — Slack notify fail-CLOSED proptest

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

## WI-PROPTEST-FU-002 — `corelink-admin-dry-run` — Dual-approval simulator proptest

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

## WI-PROPTEST-FU-003 — `corelink-cf-bindings` — CAS idempotency proptest

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

## WI-PROPTEST-FU-004 — `corelink-d1-migrations` — Migration additivity proptest

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

## WI-PROPTEST-FU-005 — CI density-gate workflow

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

| WI | Crate | Effort | Priority | INV |
|---|---|---|---|---|
| WI-PROPTEST-FU-001 | `corelink-slack-real` | 0.5d | P2 | INV-AUDIT-EMIT-ATOMIC |
| WI-PROPTEST-FU-002 | `corelink-admin-dry-run` | 1.0d | P1 | INV-ADMIN-DUAL-APPROVAL |
| WI-PROPTEST-FU-003 | `corelink-cf-bindings` | 1.0d | P1 | INV-CAS-IDEMPOTENCY |
| WI-PROPTEST-FU-004 | `corelink-d1-migrations` | 0.5d | P2 | INV-AUTH-MIGRATION-ADDITIVE |
| WI-PROPTEST-FU-005 | CI density gate | 0.5d | P3 | — (tooling) |

**Total effort:** 3.5d. **GA-blocker:** WI-PROPTEST-FU-002 + WI-PROPTEST-FU-003
(dual-approval + CAS idempotency). The rest are post-GA quality
hardening.
