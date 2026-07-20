---
type: "ADR"
title: "ADR-0071 — Cache find-missing is a read-superset capability (read ⊇ find-missing)"
description: "Why cache find-missing is a distinct, independently-grantable least-privilege scope that is a STRICT SUBSET of read — a find-only PAT probes existence and nothing else, while every read PAT satisfies find-missing unchanged; the strict non-implying corelink-reapi plane is dormant and NOT the canonical model."
source_files:
  - "specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md"
checkpoint_sha: "92140a5af7b41d50d8b43e2dfa7ab45686a596ac"
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
- The mint persists a new `pat.scope` value `"find-missing"` (forwarded verbatim as `x-corelink-scope`);
  the embedded `PatScopes` bitset mirrors it (read/read-write also carry the FIND bit). An EMPTY request
  stays `read-only` (back-compat), NOT find-only; `admin`/`owner` remain never self-serve-grantable and the
  fail-CLOSED exact-token grammar is unchanged. The dormant `corelink-reapi` gRPC plane is deliberately NOT
  revived — its non-implying stance is superseded by this hierarchy as the canonical model
  (`specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:59-86`).

# Consequences

- The dashboard "Find missing" checkbox works: find-only → a find-only PAT; find+read → read; find+write →
  read-write (no mislabel). No existing PAT loses find-missing (read ⊇ find), and no caller gains read/write
  from a find-only grant (a find-only PAT is 403'd on CAS read/write). The `corelink-reapi` distinct
  non-implying handler + its `AC-FM-5/6` tests remain dormant/aspirational and are NOT canonical; this ADR
  is the anti-drift record so nobody mounts them without adopting the hierarchy
  (`specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:90-98`).

# Citations

1. `specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:30-55` — the Context: the live plane already read-gated find-missing (`handle_find_missing` on `can_read()`) and the strict `corelink-reapi` non-implying plane is dormant/unmounted dead code.
2. `specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:59-86` — the Decision: `find-missing ⊂ read-only ⊂ read-write`; the `can_read()` → `can_find_missing()` move at `handle_find_missing` only; the new `"find-missing"` `pat.scope` value + mirrored FIND bit; empty stays read-only; the dormant reapi plane is NOT revived.
3. `specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:90-98` — the Consequences: the working dashboard checkbox, no read PAT regression, find-only 403'd on CAS read/write, and the anti-drift record for the dormant non-implying handler.
4. `specs/03_architecture/adrs/ADR-0071-find-missing-scope-read-superset-hierarchy.md:102-109` — the Alternatives considered: fold-into-read (forfeits least-privilege), strict non-implying (breaks read PATs, revives dead code), and full bitset mint with a mounted strict plane (over-engineered for a dormant surface) — all rejected.
