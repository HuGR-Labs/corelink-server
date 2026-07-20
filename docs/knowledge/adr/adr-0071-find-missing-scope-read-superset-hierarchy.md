---
type: "ADR"
title: "ADR-0071 — Cache find-missing is a read-superset capability (read ⊇ find-missing)"
description: "Why cache find-missing is a distinct, independently-grantable least-privilege scope that is a STRICT SUBSET of read — a find-only PAT probes existence and nothing else, while every read PAT satisfies find-missing unchanged; the strict non-implying corelink-reapi plane is dormant and NOT the canonical model."
source_files:
  - "specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md"
checkpoint_sha: "51cbc85b2da2ba57eec2d485163f61e86ddaa027"
provenance: "AUTHORED"
tags: ["adr", "auth", "scope", "pat", "find-missing", "bazel", "reapi", "cache"]
timestamp: "2026-07-20T00:00:00Z"
---

# ADR-0071 — Cache find-missing is a read-superset capability (read ⊇ find-missing)

`FindMissingBlobs` (the Bazel/sccache REAPI existence probe — "which of these do you already have?") is a
real capability build tools use to avoid re-uploading known objects. This ADR records the decision to make
`cache:find-missing` a distinct, independently-grantable self-serve scope that is a STRICT SUBSET of read:
`find-missing ⊂ read-only ⊂ read-write`. A find-only PAT runs existence probes and nothing else (the true
least-privilege cache credential), while every existing read/read-write/admin PAT satisfies find-missing
unchanged — because read is a superset of find-missing (if you can download a blob you can probe whether it
exists). It supersedes the dormant `corelink-reapi` "non-implying" model as the canonical hierarchy.

# Context

The self-serve dashboard offers find-missing as a `cache:find-missing` scope and `corelink-pat` defines a
distinct `SCOPE_CACHE_FIND` bit, but two facts drove the decision (both verified live 2026-07-20): the LIVE
customer plane already read-gated find-missing (`bazel_v2::handle_find_missing` gated on `can_read()`, so a
read PAT already did find-missing everywhere customers reach), and the strict "non-implying" find-missing
plane (`corelink-reapi` gRPC, `cache_find_missing_does_not_imply_cache_read`) is DORMANT — never mounted,
the only `PatValidator` is a test stub, aspirational dead code, not an enforcement surface. Before this ADR
the mint provisioned only a coarse `read-only`/`read-write` string, so `cache:find-missing` 401'd on use
(`specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:30-55`).

# Decision

- A **find-only** PAT (`cache:find-missing` requested ALONE) grants `FindMissingBlobs` probes and nothing
  else — no download, no upload — the TRUE least-privilege cache credential; **read is a superset**
  (`requires_find_missing` is satisfied by any read grant), so every existing `read-only`/`read-write`/
  `cas:*`/`admin` PAT satisfies find-missing unchanged (zero regression).
- Enforcement moves from `can_read()` to `can_find_missing()` at `bazel_v2::handle_find_missing` ONLY;
  `can_find_missing() = can_read() || <explicit find token>`, so the gate widens to admit find-only PATs
  and admits nothing it did not already admit via read.
- Storage is an ADDITIVE marker, NOT a 4th `pat.scope` value. The 0037 CHECK
  (`scope IN ('read-write','read-only','admin')`) forbids a new value — adding one is a DESTRUCTIVE rebuild
  of the live credential table (SQLite cannot ALTER a CHECK), banned by INV-AUTH-MIGRATION-ADDITIVE — so a
  find-only PAT stores the CHECK-safe base `scope = 'read-only'` PLUS an additive `find_only` column
  (migration 0093, `ALTER TABLE pat ADD COLUMN`, mirroring the runner-job marker 0086). The Worker
  (`extractAuth`, the sole `x-corelink-scope` authority) reads `find_only` and — when `= 1` — forwards the
  literal `x-corelink-scope: find-missing` (NOT the base `read-only`), so the container narrows to
  `can_find_missing()` only; `scope_to_list` surfaces such a PAT as `cache:find-missing`, never `cache:read`.
  The minted `PatScopes` bitset mirrors the effective grant (a find-only PAT carries ONLY `SCOPE_CACHE_FIND`;
  read/read-write also carry the FIND bit). An EMPTY request stays `read-only` (back-compat), NOT find-only;
  `admin`/`owner` remain never self-serve-grantable and the fail-CLOSED exact-token grammar is unchanged. The
  dormant `corelink-reapi` gRPC plane is deliberately NOT revived — its non-implying stance is superseded by
  this hierarchy as the canonical model
  (`specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:59-93`).

# Consequences

- The dashboard "Find missing" checkbox works: find-only → a find-only PAT; find+read → read; find+write →
  read-write (no mislabel). No existing PAT loses find-missing (read ⊇ find), and no caller gains read/write
  from a find-only grant (a find-only PAT is 403'd on CAS read/write). The `corelink-reapi` distinct
  non-implying handler + its `AC-FM-5/6` tests remain dormant/aspirational and are NOT canonical; this ADR
  is the anti-drift record so nobody mounts them without adopting the hierarchy
  (`specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:95-105`).
- **Adapter-plane fail-close (defense-in-depth).** Because a find-only PAT stores the CHECK-safe base
  `scope = 'read-only'`, any plane that authorizes from the D1 `scope` DIRECTLY — not the Worker's
  `x-corelink-scope` header — would see `read-only` and grant read. The OCI registry `/token` exchange
  (`routes/oci.rs`) does exactly that (no header gate), so a find-only PAT would otherwise `docker pull`.
  The shared adapter verifier therefore selects `find_only` and REJECTS a find-only PAT fail-CLOSED on
  EVERY adapter surface (npm/pip/brew/cargo/OCI have no find-missing op) — a per-plane guarantee that does
  not depend on each adapter owning its own header gate. Regression-locked by
  `adapter_pat::tests::find_only_pat_is_rejected_on_the_adapter_plane`
  (`specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:106-116`).

# Citations

1. `specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:30-55` — the Context: the live plane already read-gated find-missing (`handle_find_missing` on `can_read()`) and the strict `corelink-reapi` non-implying plane is dormant/unmounted dead code.
2. `specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:59-93` — the Decision: `find-missing ⊂ read-only ⊂ read-write`; the `can_read()` → `can_find_missing()` move at `handle_find_missing` only; storage as an ADDITIVE `find_only` marker (migration 0093, base `scope = 'read-only'`) — NOT a 4th `pat.scope` value (the 0037 CHECK forbids it) — with the Worker forwarding `x-corelink-scope: find-missing` when `find_only = 1`; the mirrored FIND bit; empty stays read-only; the dormant reapi plane is NOT revived.
3. `specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:95-105` — the Consequences: the working dashboard checkbox, no read PAT regression, find-only 403'd on CAS read/write, and the anti-drift record for the dormant non-implying handler.
3a. `specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:106-116` — the adapter-plane fail-close Consequences bullet: the shared adapter verifier rejects a find-only PAT fail-CLOSED on every adapter surface (the header-less OCI `/token` exchange authorizes from the D1 `read-only` base directly, so it would otherwise `docker pull`); regression-locked by `find_only_pat_is_rejected_on_the_adapter_plane`.
4. `specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:118-131` — the Alternatives considered: fold-into-read (forfeits least-privilege), strict non-implying (breaks read PATs, revives dead code), a new `"find-missing"` `pat.scope` value (a destructive rebuild of the live table to widen the 0037 CHECK), and full bitset mint with a mounted strict plane — all rejected in favour of the additive `find_only` marker.
