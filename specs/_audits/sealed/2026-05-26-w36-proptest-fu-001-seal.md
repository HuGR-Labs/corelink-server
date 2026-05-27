---
id: "SEAL-W36-PROPTEST-FU-001"
type: "seal_audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
closed: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags:
  - "audit"
  - "seal"
  - "wave-36"
  - "proptest"
  - "fu-w33-001"
  - "density-gate"
references:
  - "specs/_audits/proptest-followup-tickets.md"
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/2026-05-15-proptest-density.md"
  - "specs/_audits/2026-05-22-wave33-code-reorg-spec.md"
---

# W36-PROPTEST-FU-001 — Umbrella aggregator double-counting closure SEAL

> **Path taken:** OBSOLETE (with minor allowlist + lib.rs doc hygiene).
> **Followup closed:** WI-PROPTEST-FU-W33-001.
> **Branch:** `worktree-agent-ae1ab0c71943dba00`
> **Base commit:** `f84142fda94c9a5601d44e2fbd838e30bc7d0fa4`
> **Worktree:** `/Users/gustavoschneiter/Documents/HuGR/corelink-server/.claude/worktrees/agent-ae1ab0c71943dba00`

## §1. Mandate recap

Per `specs/_audits/proptest-followup-tickets.md` WI-PROPTEST-FU-W33-001
(authored 2026-05-26): the proptest density gate was reported to
"double-count" Wave-33 umbrella aggregator crates — INV references in
the umbrella's `lib.rs` doc comments inflated the denominator while 0
proptests in the umbrella's own `tests/` directory zeroed the
numerator, producing false-positive ratio < 1.0 readings for 4 crates:

- `corelink-auth` (Wave 33 Stage 1 Stream B aggregator)
- `corelink-cas` (Wave 33 Stage 1 Stream A aggregator)
- `corelink-container` (Wave 33 Stage 2.B apps/server → crate)
- `corelink-core` (Wave 33 Stage 0 apex types)

The ticket offered 3 acceptance options:

(a) Script heuristic to detect aggregator crates and auto-exempt.
(b) Document the absorbed-crate ↔ INV mapping in each umbrella's lib.rs.
(c) Move INV refs out of umbrella lib.rs doc comments (least preferred).

The agent mandate was to **first investigate whether the bug had been
INVALIDATED by Wave 35 Phase 2 absorption**; if so, mark CLOSED with
reason "obsoleted by physical absorption" rather than write code.

## §2. Investigation (evidence)

### §2.1. Density audit live state — post-Wave-35-Phase-2

`bash scripts/audit_proptest_density.sh` snapshot 2026-05-26 (after
the BYOK absorption merge `f84142fda94c9a5601d44e2fbd838e30bc7d0fa4`):

```
crate              | prop_macros | prop_tests | inv_refs | ratio
corelink-auth      | 4           | 14         | 6        | 2.33  ← PASSES (post-absorption)
corelink-cas       | 12          | 72         | 19       | 3.79  ← PASSES (post-absorption)
corelink-container | 0           | 0          | 3        | 0.00  ← INV-pin doc residual
corelink-core      | 0           | 0          | 1        | 0.00  ← INV-pin doc residual
```

### §2.2. Structural analysis — corelink-auth + corelink-cas

Wave 35 Phase 2 (BYOK absorption + auth/cas physical absorption,
merged `f84142fda94c9a5601d44e2fbd838e30bc7d0fa4`) physically moved
the following modules from their `pub use`-re-exported absorbed
crates into the umbrella's own `src/`:

- `corelink-auth/src/schema/` + `schema.rs` (was: `corelink-auth-schema`)
- `corelink-auth/src/webauthn/` + `webauthn.rs` (was: `corelink-webauthn`)
- `corelink-cas/src/chunker/` + `chunker.rs` (was: `corelink-chunker`)
- `corelink-cas/src/dedup/` + `dedup.rs` (was: `corelink-dedup`)
- `corelink-cas/src/edge/` + `edge.rs` (was: `corelink-edge`)
- `corelink-cas/src/eviction.rs` (was: `corelink-eviction` re-export
  body still in original crate but proptests landed here)
- `corelink-cas/src/lru_tracker/` (was: `corelink-lru-tracker`)
- `corelink-cas/src/manifest/` + `manifest.rs` (was: `corelink-manifest`)
- `corelink-cas/src/meta.rs`
- `corelink-cas/src/multipart_schema/`
- `corelink-cas/src/r2_multipart.rs`

Each absorbed module brought its proptest harness along into the
umbrella's `tests/` directory:

- `corelink-auth/tests/` — 7 test files including
  `schema_prop_schema.rs` + `webauthn_prop_webauthn.rs` (4 proptest!
  blocks total).
- `corelink-cas/tests/` — 16 test files including 8 `*_prop.rs`
  files (12 proptest! blocks total).

**Result:** the original "double-counting via `pub use` re-export
without local proptests" framing is **INVALIDATED** for both crates.
Their density-gate ratios are now 2.33 (auth) and 3.79 (cas),
genuinely above the 1.0 threshold. No exemption needed.

### §2.3. Structural analysis — corelink-container + corelink-core

Inspection of `crates/corelink-container/src/lib.rs` (47 LOC) and
`crates/corelink-core/src/lib.rs` (50 LOC) confirmed:

- **corelink-container** is `apps/server`'s library-half (15 src
  files of route/webhook/byok orchestrator logic). Its `lib.rs` is a
  module-index only (`pub mod byok_orchestrator`, `pub mod routes`,
  `pub mod wall_clock`, `pub mod webhook`, etc.). It was **never** a
  `pub use` re-export façade — it owns its own logic.
- **corelink-core** is the Wave-33 Stage 0 apex types crate
  (TenantId, Digest, Region, SecretWrap, CoreError, Clock). Its
  `lib.rs` is a module-index plus type re-exports (`pub use
  errors::{...}; pub use types::{...}`). Types live in `src/types/`,
  not in re-exported absorbed crates.

For both, the INV references are **route-/policy-pin annotations**
documenting which invariant is enforced AT the boundary, NOT the
load-bearing property tests:

| Crate | INV ref | Property-test owner |
|---|---|---|
| corelink-container | `INV-AUTH-MIGRATION-ADDITIVE` | `corelink-d1-migrations/tests/prop_migration_additivity.rs` |
| corelink-container | `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` | `corelink-audit` + `corelink-slack-real/tests/prop_slack_emit_atomic.rs` |
| corelink-container | `INV-TENANT-ISOLATION` | `corelink-tenant-path` + `corelink-auth::schema` (`corelink-auth/tests/schema_prop_schema.rs`) |
| corelink-core | `INV-DATA-RESIDENCY` | `corelink-signup` (regional-pin at signup) |

The "double-counting" framing **doesn't strictly apply** to these
two because they were never the re-export aggregator pattern the
ticket described — they are instead an **"INV-pin documentation"
pattern**: a crate documents which invariant it enforces at its API
boundary while the property test lives in the owning crate.

## §3. Decision

**Path: OBSOLETE** (with minor allowlist + lib.rs doc hygiene
satisfying ticket option (b) for the residual two crates).

Rationale:

1. The ticket's original problem statement is **fully invalidated**
   for `corelink-auth` + `corelink-cas` by Wave 35 Phase 2 physical
   absorption. No code change can "close" something that's already
   structurally resolved.
2. The ticket's problem statement **never applied** to
   `corelink-container` + `corelink-core` — they are not `pub use`
   re-export aggregators. They fit the separate "INV-pin
   documentation" pattern.
3. Implementing ticket option (a) (script heuristic for aggregator
   detection) would be **scope creep**: the structural surface for
   which it would apply has collapsed from 4 crates to 0 strict
   aggregators (the two remaining are not aggregators). The
   allowlist already serves as the exemption mechanism for the
   INV-pin doc residual; explicit per-INV ownership documentation
   is more honest than a structural heuristic.
4. Ticket option (b) ("document the absorbed-crate ↔ INV mapping")
   is the minimum-scope right answer for the residual two crates;
   land that as part of closure.

## §4. Closure scope (this session)

### §4.1. Files changed

1. `scripts/proptest-density-allowlist.txt` — removed `corelink-auth`
   and `corelink-cas` from the W33-001 section; rewrote the W33-001
   block to document the closure narrative + the residual INV-pin
   documentation pattern (container + core) with per-crate ownership
   pointers.
2. `specs/_audits/proptest-followup-tickets.md` — version bump
   `1.2.0 → 1.3.0`; WI-PROPTEST-FU-W33-001 marked CLOSED with full
   closure narrative; summary table updated.
3. `specs/_audits/2026-05-26-wave-33-34-closure-followups.md` — §2
   roster row #4 and §6 prose updated to CLOSED with pointer to this
   SEAL audit.
4. `crates/corelink-container/src/lib.rs` — added `# INV pin map`
   doc block enumerating each pinned INV → property-test-owner crate.
5. `crates/corelink-core/src/lib.rs` — added `## INV pin map` doc
   block for `INV-DATA-RESIDENCY` → `corelink-signup` ownership.
6. `specs/_audits/2026-05-26-w36-proptest-fu-001-seal.md` (this file).

### §4.2. Charter compliance

- ✅ `PROPTEST_CASES` env behavior unchanged.
- ✅ No proptests disabled.
- ✅ No `#[ignore]` / `#[allow]` added.
- ✅ No new proptests added (FU-002's scope is untouched).
- ✅ No non-FU-001 file modified outside the closure paper trail.
- ✅ No `--no-verify` used.

## §5. Verification

### §5.1. Density gate

```
$ bash scripts/check_proptest_density_gate.sh
…
ALLOWED gap: corelink-clerk-cf (ratio=0.00, inv_refs=2) — see proptest-density-allowlist.txt
ALLOWED gap: corelink-container (ratio=0.00, inv_refs=3) — see proptest-density-allowlist.txt
ALLOWED gap: corelink-core (ratio=0.00, inv_refs=1) — see proptest-density-allowlist.txt
ALLOWED gap: corelink-statuspage-real (ratio=0.00, inv_refs=2) — see proptest-density-allowlist.txt
ALLOWED gap: corelink-wasm (ratio=0.75, inv_refs=8) — see proptest-density-allowlist.txt
proptest density gate: PASS (no regressions outside the allowlist).
```

Note: `corelink-auth` and `corelink-cas` are absent from the
"ALLOWED gap" list — their ratios (2.33 and 3.79) clear the 1.0
threshold naturally without exemption. Confirms physical-absorption
closure.

### §5.2. Density audit ratios (post-closure)

The relevant rows of `scripts/audit_proptest_density.sh` output:

```
corelink-auth      | 4  | 14 | 6  | 2.33
corelink-cas       | 12 | 72 | 19 | 3.79
corelink-container | 0  | 0  | 3  | 0.00  (allowlist: INV-pin doc)
corelink-core      | 0  | 0  | 1  | 0.00  (allowlist: INV-pin doc)
```

### §5.3. Build/clippy/tests

Pure documentation + allowlist text changes; no Rust source semantics
changed. The only Rust files touched are
`crates/corelink-container/src/lib.rs` and
`crates/corelink-core/src/lib.rs`, where doc comments were added
(extending the existing `//!` block). `#![deny(missing_docs)]` is
already in force on both crates and pre-existing doc coverage is
preserved.

No `cargo build` / `cargo clippy` / `cargo test` run was deemed
necessary because:
- The only Rust delta is doc comments (no code, no API change, no
  trait impls).
- `#![deny(missing_docs)]` validates doc presence at build time;
  adding doc content cannot violate it.
- The audit script + gate were re-run live and confirm GREEN.

## §6. Residual scope

WI-PROPTEST-FU-W33-002 remains **OPEN**:

- `corelink-clerk-cf` — needs per-INV proptest for
  `INV-AUTH-PAT-VERIFY-CONSTANT-TIME` + `INV-CLERK-JWKS-CACHE-TTL`.
- `corelink-statuspage-real` — needs per-INV proptest for
  `INV-STATUSPAGE-RATE-LIMIT-1-PER-5MIN` +
  `INV-STATUSPAGE-AUDIT-FAIL-CLOSED`.
- `corelink-wasm` — needs per-INV proptest for `INV-WASM-*` (or
  documented infeasibility under wasm32 constraints).

This residual is per-charter outside FU-001 scope and remains tracked
under W33-002.

## §7. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

**End of W36-PROPTEST-FU-001 SEAL audit.**
