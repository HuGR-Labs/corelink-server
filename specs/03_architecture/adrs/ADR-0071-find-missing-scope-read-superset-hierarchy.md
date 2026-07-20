---
id: "ADR-0071"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-07-20"
updated: "2026-07-20"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "auth", "scope", "pat", "find-missing", "bazel", "reapi", "cache"]
---

# ADR-0071 — Cache `find-missing` is a Read-Superset Capability (distinct least-privilege scope, read ⊇ find-missing)

## Status

ACTIVE (decision ratified + implemented). Tech-lead decision, self-serve PAT
scope model, 2026-07-20. Follow-on to the self-serve scope-vocabulary fix
(PR #867, ADR context in `crates/corelink-container/src/scope.rs`). See
`scope.rs` (`classify_requested_scopes`, `requires_find_missing`, `CacheScope`),
`routes/bazel_v2.rs` (`handle_find_missing`), and `customer_d1.rs`
(`map_requested_scopes` / `scope_bits` / `scope_to_list`).

## Context

`FindMissingBlobs` (the Bazel/sccache REAPI existence probe — "which of these do
you already have?") is a real capability build tools use to avoid re-uploading
known objects. The self-serve dashboard (`admin-ui` `KeysClient`) offers it as a
`cache:find-missing` scope, and `corelink-pat` / `auth_model.md §3.1` define a
distinct `SCOPE_CACHE_FIND` bit.

Two facts drove this decision (both verified live, 2026-07-20):

1. **The live customer plane read-gates find-missing.** Real prod Bazel
   `FindMissingBlobs` traffic hits the container's HTTP REAPI surface,
   `routes/bazel_v2.rs::handle_find_missing`, which gated on `can_read()`. So a
   `cache:read` / `cache:read-write` PAT already did find-missing everywhere
   customers reach.

2. **The strict "non-implying" find-missing plane is DORMANT.** The
   `corelink-reapi` gRPC handler (`handler/cas.rs`, `require_scope(CacheFindMissing)`,
   with `cache_find_missing_does_not_imply_cache_read`) is **never mounted** — the
   prod binary removed the tonic server, the only `PatValidator` is a test stub,
   and no live path converts a PAT scope into an `AuthScope`. It is aspirational
   dead code, not an enforcement surface.

Before this ADR the self-serve mint provisioned only a coarse `read-only` /
`read-write` string (no `SCOPE_CACHE_FIND` path), so `cache:find-missing` was
rejected (`401`) — the "Find missing" checkbox 401'd on use. The choice was how
to make it real without either (a) minting a mislabeled token or (b) breaking the
existing read-scoped PATs that do find-missing today.

## Decision

**`find-missing` is a distinct, independently-grantable scope, and it is a
STRICT SUBSET of read: `find-missing ⊂ read-only ⊂ read-write`.**

- A **find-only** PAT (`cache:find-missing` requested ALONE) grants
  `FindMissingBlobs` existence probes and **nothing else** — no download (read),
  no upload (write). This is the TRUE least-privilege cache credential.
- **Read is a superset of find-missing** (`requires_find_missing` is satisfied by
  any read grant): if a caller can download a blob it can certainly probe whether
  the blob exists. So every existing `read-only` / `read-write` / `cas:*` / `admin`
  PAT satisfies find-missing **unchanged** — zero regression.
- Enforcement moves from `can_read()` to `can_find_missing()` at
  `bazel_v2::handle_find_missing` ONLY. `can_find_missing() = can_read() ||
  <explicit find token>`, so the gate widens to admit find-only PATs and admits
  nothing it did not already admit via read.
- **Storage — an additive marker, NOT a 4th `pat.scope` value.** `pat.scope`
  carries `CHECK (scope IN ('read-write','read-only','admin'))` (0037); adding a
  value would need a DESTRUCTIVE rebuild of the live credential table (SQLite
  cannot ALTER a CHECK), banned by INV-AUTH-MIGRATION-ADDITIVE. Instead a
  find-only PAT stores the CHECK-safe base `scope = 'read-only'` PLUS an **additive
  `find_only` column** (migration 0093, `ALTER TABLE pat ADD COLUMN`, same pattern
  as the runner-job marker 0086). The Worker (`extractAuth`, sole
  `x-corelink-scope` authority) reads `find_only` and — when `= 1` — forwards the
  literal `x-corelink-scope: find-missing` (NOT the base `read-only`), so the
  container narrows to `can_find_missing()` only. `scope_to_list` surfaces a
  find-only PAT as `cache:find-missing` (via the marker), never `cache:read`. An
  EMPTY request stays `read-only` (back-compat), NOT find-only.
- `admin`/`owner` remain **never** self-serve-grantable; the fail-CLOSED
  exact-token grammar (rt-nuclear #15) is unchanged — truly-unknown tokens still
  `Err`.

**We deliberately do NOT revive the dormant `corelink-reapi` gRPC plane.** Its
`cache_find_missing_does_not_imply_cache_read` stance (read does NOT imply find)
is superseded by this hierarchy as the canonical model; if that plane is ever
mounted for customer traffic it MUST adopt read ⊇ find-missing (and thread the
mint's FIND bit) rather than reject read-scoped PATs.

## Consequences

- The dashboard "Find missing" checkbox works: find-only → a find-only PAT;
  find+read → read; find+write → read-write (no mislabel).
- No existing PAT loses find-missing (read ⊇ find). No caller gains read/write
  from a find-only grant (a find-only PAT is 403'd on CAS read/write — proven in
  `bazel_v2::tests::find_only_scope_denied_on_cas_read`).
- The `corelink-reapi` distinct-non-implying find-missing handler + its
  `AC-FM-5/6` tests remain dormant/aspirational and are NOT the canonical model;
  this ADR is the anti-drift record so nobody mounts them without adopting the
  hierarchy.
- **Adapter-plane fail-close (security).** Because the marker stores the
  CHECK-safe base `scope = 'read-only'`, any plane that authorizes from the D1
  `scope` DIRECTLY — not the Worker's `x-corelink-scope` header — would see
  `read-only` and grant read. The **OCI registry `/token` exchange
  (`routes/oci.rs`) does exactly that** (no header gate), so a find-only PAT would
  otherwise `docker pull`. The shared adapter verifier
  (`adapter_pat::verify_capability`) therefore selects `find_only` and **rejects a
  find-only PAT fail-CLOSED on EVERY adapter surface** (npm/pip/brew/cargo/OCI have
  no find-missing op) — defense-in-depth that does not depend on each adapter
  having its own header gate. Regression-locked in
  `adapter_pat::tests::find_only_pat_is_rejected_on_the_adapter_plane`.

## Alternatives considered

- **Fold find-missing into read (remove the UI checkbox).** Honest on the live
  plane but forfeits the genuine least-privilege find-only credential; rejected in
  favour of a real distinct scope.
- **Strict non-implying (honour the dormant reapi model): read does NOT imply
  find.** Would break every existing read-scoped PAT's find-missing (a live
  regression) and revive dead infrastructure. Rejected.
- **A new `pat.scope` value `"find-missing"` (a DESTRUCTIVE pat-table rebuild to
  widen the 0037 CHECK).** Risky rebuild of the live credential table +
  INV-AUTH-MIGRATION-ADDITIVE waiver, disproportionate for a dormant-plane
  distinction. Rejected in favour of the additive `find_only` marker.
- **Full bitset mint with a mounted strict plane.** Over-engineered for a dormant
  surface with no demand; large and launch-risky. Rejected.

## Verification

- `scope.rs::classify_requested_scopes_is_exact_token_and_fail_closed` (find-only →
  `FindMissing`; find+read → `ReadOnly`; find+write → `ReadWrite`; empty →
  `ReadOnly`), `scope.rs::find_missing_capability_hierarchy` (read ⊇ find; find ⊄
  read; fail-closed on empty/unknown).
- `routes/bazel_v2::tests::find_missing_find_only_scope_passes_gate` (find-only →
  200) + `find_only_scope_denied_on_cas_read` (find-only → 403 on read) +
  `find_missing_missing_scope_returns_403` (no scope → 403, unchanged).
- `customer_d1::tests::{scope_map_is_frozen, keys_create_find_only_stores_read_only_base_plus_marker}`
  (find-only → base `"read-only"` + `find_only = 1`; display `cache:find-missing`).
- `worker/tests/find_only_scope_forward.test.ts` (find_only=1 → forwarded
  `x-corelink-scope: find-missing`; normal PAT → base scope, no regression).
- Live: a dashboard-minted find-only PAT performs `FindMissingBlobs` and is denied
  CAS read (verified post-roll).
