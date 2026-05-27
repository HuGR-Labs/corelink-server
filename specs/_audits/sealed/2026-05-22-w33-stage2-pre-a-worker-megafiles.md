# Wave 33 Stage 2 PRE-A — corelink-worker Mega-File Decomposition SEAL Audit (2026-05-22)

> **Doc kind:** stage closure audit (evidence; `_audits/` excluded from
> canonical schema validation).
>
> **Owner:** Gustavo Schneiter.
>
> **Authored:** 2026-05-22 by Claude Opus 4.7 in branch
> `wt/r-prep-w33-stage2-pre-a-worker-megafiles` (worktree
> `.claude/worktrees/agent-a3bbfd8bc4b870960`).
>
> **Mandate:** wave-33 Stage 2 PRE-A owns the in-place decomposition
> of the 4 mega-files inside `crates/corelink-worker/` that would
> violate L2.10 if moved as-is during Stage 2.A (corelink-worker
> piece-relocation). Per the dispatch, this is a behaviour-preserving
> file split — NO moves to other crates, NO logic change.
>
> **Pattern reference:** Stream A2 precedent
> (`specs/_audits/sealed/2026-05-22-w33-stream-a2-megafiles.md`,
> commits `d15d68b8` / `558fcecf` / `6ce103e7` / `f80f40d4`):
> function-internal helper extraction (when needed) + file split +
> behaviour-preservation, with parent dispatch files re-exporting
> the canonical public surface so consumers continue to import via
> the same paths.

## §1. Scope

PRE-A executes 4 sub-step `c` commits (one per mega-file) plus the
SEAL audit. Per the A2 reconcile.rs precedent (no helper extraction
needed), every mega-file had its largest function well under the
500-LOC cap, so each sub-step is a single `c`-commit (pure file
split). The dispatch's a/b/c three-step cadence collapses to just
`c` for all four files.

| Sub-step | SHA | Title | Strategy | Status |
|---|---|---|---|---|
| 2.PRE-A.1.c | `628ebb18` | Split `auth/revocation.rs` into `revocation/{...}.rs` (7 files) | Pure file split per L2.10 | **SEALED** |
| 2.PRE-A.2.c | `4357e134` | Split `reapi/cas/split_splice.rs` into `split_splice/{...}.rs` (10 files) | Pure file split per L2.10 | **SEALED** |
| 2.PRE-A.3.c | `30ff07bc` | Split `middleware/timing_padding.rs` into `timing_padding/{...}.rs` (10 files) | Pure file split per L2.10 | **SEALED** |
| 2.PRE-A.4.c | `44b3a409` | Split `reapi/ac/handler.rs` into `handler/{...}.rs` (9 files) | Pure file split per L2.10; one mid-split rebalance (methods.rs 520→456 LOC via stash.rs lift) | **SEALED** |

4 of 4 sub-steps SEALED. Hard pause triggers: **NONE** fired (see §7).

## §2. Per-mega-file pre/post LOC

### `auth/revocation.rs` (PRE-A.1)

| State | Total LOC | Largest file | Files in `revocation/` |
|---|---|---|---|
| Pre-PRE-A (main `915e83b2`) | 2024 | 2024 (revocation.rs) | 0 |
| Post-PRE-A.1.c (`628ebb18`) | 2200 | 452 (`types.rs`) | 7 |

### `reapi/cas/split_splice.rs` (PRE-A.2)

| State | Total LOC | Largest file | Files in `split_splice/` |
|---|---|---|---|
| Pre-PRE-A.2 (post-PRE-A.1) | 1758 | 1758 (split_splice.rs) | 0 |
| Post-PRE-A.2.c (`4357e134`) | 1943 | 414 (`tests.rs`) | 9 |

### `middleware/timing_padding.rs` (PRE-A.3)

| State | Total LOC | Largest file | Files in `timing_padding/` |
|---|---|---|---|
| Pre-PRE-A.3 (post-PRE-A.2) | 1601 | 1601 (timing_padding.rs) | 0 |
| Post-PRE-A.3.c (`30ff07bc`) | 1767 | 426 (`tests.rs`) | 9 |

### `reapi/ac/handler.rs` (PRE-A.4)

| State | Total LOC | Largest file | Files in `handler/` |
|---|---|---|---|
| Pre-PRE-A.4 (post-PRE-A.3) | 1578 | 1578 (handler.rs) | 0 |
| Mid-split (methods+stash unsplit) | 1694 | 520 (`methods.rs`) | 7 |
| Post-PRE-A.4.c (`44b3a409`) | 1726 | 456 (`methods.rs`) | 8 |

The mid-split snapshot is documented because `methods.rs` momentarily
hit 520 LOC, tripping the L2.10 HARD CAP. Per the charter's
"if any resulting file is >500, re-split further" rule, the stash
methods + `ActionResultStash` were lifted into a sibling `stash.rs`
(92 LOC), bringing `methods.rs` to 456 LOC. The rebalance happened
in the same commit (`44b3a409`); the mid-split state never landed
on the branch.

### Aggregate

| Pre-split monolith total | Post-split aggregate | Largest single file |
|---|---|---|
| 2024 + 1758 + 1601 + 1578 = **6961 LOC** | 8 + 11 + 10 + 9 = **38 files**, 7636 LOC | **456 LOC** (`ac/handler/methods.rs`) |

The +675 LOC delta vs. the pre-split monoliths reflects per-submodule
headers + import groups + the parent dispatch files' module rustdoc
+ the `pub use` re-export blocks. No file exceeds the 500 LOC HARD
CAP.

## §3. Helper extraction log

**None required.** Per the A2 reconcile.rs precedent, every mega-file's
largest function fit well under the 500-LOC cap:

| Mega-file | Largest fn | LOC | Decision |
|---|---|---|---|
| `auth/revocation.rs` | `mass_revoke` | ~92 | No extraction; pure file split |
| `reapi/cas/split_splice.rs` | `append_chunk_inner` | ~103 | No extraction; pure file split |
| `middleware/timing_padding.rs` | `Service::call` | ~95 | No extraction; pure file split |
| `reapi/ac/handler.rs` | `get_action_result_inner` | ~215 | No extraction; pure file split |

All inner methods are under the 300-LOC sweet-spot threshold that
triggers helper extraction per the A2 precedent
(`batch_read_blobs` at 641 LOC was the canonical trigger; nothing
in this dispatch comes close). The 38 resulting files split along
type-vs-trait-vs-impl-vs-test boundaries identical to the A2
reconcile.rs pattern.

## §4. L2.10 audit table — every NEW file with LOC

### `auth/revocation/` (8 files incl. parent)

| File | LOC | L2.10 classification |
|---|---|---|
| `revocation/types.rs` | 452 | advisory (200-500; under cap) |
| `revocation/tests.rs` | 450 | advisory |
| `revocation/orchestrator.rs` | 414 | advisory |
| `revocation/in_memory_meta.rs` | 303 | sweet-spot |
| `revocation/traits.rs` | 209 | sweet-spot |
| `revocation.rs` (parent) | 137 | sweet-spot |
| `revocation/in_memory_store.rs` | 136 | sweet-spot |
| `revocation/in_memory_broadcast.rs` | 99 | sweet-spot |

### `reapi/cas/split_splice/` (10 files incl. parent)

| File | LOC | L2.10 classification |
|---|---|---|
| `split_splice/tests.rs` | 414 | advisory |
| `split_splice/handler_methods.rs` | 402 | advisory |
| `split_splice/errors.rs` | 238 | sweet-spot |
| `split_splice/handler.rs` | 228 | sweet-spot |
| `split_splice/tests_splice.rs` | 203 | sweet-spot |
| `split_splice.rs` (parent) | 116 | sweet-spot |
| `split_splice/handler_trait.rs` | 102 | sweet-spot |
| `split_splice/tests_common.rs` | 94 | sweet-spot |
| `split_splice/builder.rs` | 90 | sweet-spot |
| `split_splice/types.rs` | 56 | sweet-spot |

### `middleware/timing_padding/` (10 files incl. parent)

| File | LOC | L2.10 classification |
|---|---|---|
| `timing_padding/tests.rs` | 426 | advisory |
| `timing_padding/stats.rs` | 234 | sweet-spot |
| `timing_padding/service.rs` | 209 | sweet-spot |
| `timing_padding.rs` (parent) | 173 | sweet-spot |
| `timing_padding/layer.rs` | 147 | sweet-spot |
| `timing_padding/config.rs` | 139 | sweet-spot |
| `timing_padding/padding.rs` | 127 | sweet-spot |
| `timing_padding/proptests.rs` | 120 | sweet-spot |
| `timing_padding/policy.rs` | 97 | sweet-spot |
| `timing_padding/predicate.rs` | 95 | sweet-spot |

### `reapi/ac/handler/` (9 files incl. parent)

| File | LOC | L2.10 classification |
|---|---|---|
| `handler/methods.rs` | 456 | advisory |
| `handler/tests.rs` | 393 | advisory |
| `handler/errors.rs` | 186 | sweet-spot |
| `handler/builder.rs` | 178 | sweet-spot |
| `handler/envelope_store.rs` | 150 | sweet-spot |
| `handler/handler_impl.rs` | 148 | sweet-spot |
| `handler/stash.rs` | 92 | sweet-spot |
| `handler.rs` (parent) | 79 | sweet-spot |
| `handler/handler_trait.rs` | 44 | sweet-spot |

### Aggregate

| Total new files | Largest | Sweet-spot count | Advisory count |
|---|---|---|---|
| 38 | 456 LOC | 28 | 10 |

All 38 ≤ 500 LOC HARD CAP. 28 of 38 ≤ 320 LOC (sweet-spot). The
10 advisory-band files are cohesive (a single trait + impl, a
single test module, a single inner-methods impl block) — splitting
them further would obscure rather than clarify.

## §5. Behaviour-preservation evidence

### Test count delta = 0

- `corelink-worker` (default features): **95 tests** pre + 95 post
  (36 lib + 5 + 6 + 26 + 0 + 8 + 6 + 7 + 0 + 1 doc-test across 9
  test binaries).
- `corelink-worker` (`--all-features`, includes `tower-middleware`):
  **389 tests** pre + 389 post (242 lib + 13 + 5 + 11 + 6 + 26 +
  4 + 5 + 6 + 4 + 8 + 4 + 8 + 6 + 7 + 9 + 4 + 11 + 1 + 8 + 1 across
  test binaries; 2 ignored at both pre + post).
- **Combined: 389 tests at parity; ZERO regressions.**

### No `pub` API change

The 4 parent dispatch files re-export the pre-split symbol set
verbatim:

- `auth/revocation.rs` `pub use` block re-exports: `InMemoryBroadcast`,
  `InMemoryMetaRevocationSink`, `MonotonicTestClock`, `TestAuditRow`,
  `TestClock`, `InMemoryRevocationStore`, `DriftRow`, `IngestOutcome`,
  `MassRevokeResponse`, `ReconciliationSummary`, `RevocationOrchestrator`,
  `KvSessionCacheInvalidator`, `MetaRevocationSink`,
  `RevocationBroadcast`, `RevocationStore`, `SessionCacheInvalidator`,
  `HookOutcome`, `MassRevokeId`, `MassRevokeRow`, `MetaMassRevokeOutcome`,
  `MetaRevokeOutcome`, `PropagationOutcome`, `PropagationStatus`,
  `RevocationDedupKey`, `RevocationError`, `RevocationReason`,
  `RevokeRequest`, `RevokeResponse`, `RevokedEntry`, `SessionCacheKey`,
  + 5 constants. **31 names; identical to pre-split.**

- `reapi/cas/split_splice.rs` re-exports: `Clock`, `FakeClock`,
  `SplitSpliceHandlerBuilder`, `SystemClock`, `SpliceError`,
  `SplitError`, `SplitSpliceHandlerImpl`, `SplitSpliceHandler`,
  `FinalizeSplitOutcome`, `InitSplitOutcome`, `SpliceOutcome`,
  `MAX_CHUNK_BYTES`. **12 names; identical to pre-split.** The
  `reapi::cas` umbrella `pub use split_splice::{...}` block in
  `reapi/cas.rs` resolves at parity (verified by `cargo build`).

- `middleware/timing_padding.rs` re-exports: `TimingPaddingConfig`,
  `TimingPaddingError`, 5 constants, `TimingPaddingLayer`,
  `canonical_pad_target`, `JitterPolicy`, `MissArm`, `MissMarker`,
  `miss_predicates` (module), `PredicateKind`, `TimingPaddingService`,
  `bootstrap_median_ci`, `mann_whitney_u_p_value`,
  `sidak_per_test_alpha`, `BootstrapMedianCi`. **The `middleware.rs`
  umbrella `pub use timing_padding::{...}` block resolves at parity.**

- `reapi/ac/handler.rs` re-exports: `ActionCacheHandlerBuilder`,
  `ActionCacheHandlerImpl`, `Clock`, `FakeClock`, `SystemClock`,
  `AcEnvelopeStore`, `InMemoryAcEnvelopeStore`, `AcError`,
  `GetActionResult`, `UpdateActionResult`, `AC_ENVELOPE_VERSION`,
  `DEFAULT_AC_TTL_EXTEND_MS`, `ActionCacheHandler`. **The
  `reapi/ac.rs` umbrella `pub use handler::{...}` block resolves at
  parity.**

### `unsafe_code` / `unwrap` / `expect` / `panic!`

- `#![forbid(unsafe_code)]` preserved (workspace lint unchanged;
  worker crate `Cargo.toml` lint table unchanged).
- Zero `unwrap()` / `expect()` / `panic!()` introduced in `src/`
  (only in `#[cfg(test)]` blocks where workspace lints `allow` them
  per per-file `#![allow(...)]`).

### `subtle::ConstantTimeEq` preservation

- `timing_padding.rs` and submodules: **no `subtle` usage present in
  the pre-split monolith.** The timing defense is wall-clock-padding
  (`tokio::time::sleep_until`), not constant-time crypto compare —
  there is no `ConstantTimeEq` hot path to preserve in this module.
  The hard pause trigger #7 is consequently **not applicable** to
  this dispatch.
- Verified via `grep -n "ConstantTime\|subtle" crates/corelink-worker/src/middleware/timing_padding*` → 0 matches at both pre- and post-split.

### Security-critical paths byte-identical

For `timing_padding`, the load-bearing security paths preserved
byte-identical from the pre-split monolith:

- Per-request seed mixing (`server_secret XOR counter XOR header_seed`)
  + SplitMix64 finalizer (`padding.rs`).
- `canonical_pad_target` with seeded `ChaCha20Rng` jitter window.
- Per-arm `MissMarker` discriminator emit hook in
  `service.rs::Service::call` (codex round-5 P1 fix).
- `generate_server_secret` `OsRng` path + epoch fallback.

For `auth/revocation`, the load-bearing paths preserved:

- `INV-AUTH-REVOCATION-IDEMPOTENT` short-circuit in
  `RevocationOrchestrator::revoke` on `MetaRevokeOutcome::AlreadyRevoked`.
- `INV-AUTH-MASS-REVOKE-ATOMIC` two-phase (Phase 1 atomic UPDATE +
  Phase 2 chunked outbox INSERT) in
  `RevocationOrchestrator::mass_revoke`.
- `MASS_REVOKE_OUTBOX_CHUNK_SIZE` = 1000 + `MASS_REVOKE_BROADCAST_BATCH_SIZE`
  = 100 constants pinned identical.

For `reapi/cas/split_splice`, the load-bearing 5-Layer Defense paths
preserved:

- `check_region_split` / `check_region_splice` + `require_split_scope`
  / `require_splice_scope` (Layers 1+3) in `handler.rs`.
- `SessionKey::new(tenant_id, ...)` + `ChunkKey::new(tenant_id, ...)`
  + `ManifestKey::new(tenant_id, ...)` keying (Layer 2) preserved
  in `handler_methods.rs`.
- `INV-MULTIPART-IDEMPOTENT` echo path in `init_split_inner` +
  `INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST` cancel-first in
  `splice_blob_inner` preserved.

For `reapi/ac/handler`, the load-bearing 5-Layer Defense + AC-flow
paths preserved:

- 7-step GET flow + 10-step UPDATE flow in `methods.rs` byte-identical
  to pre-split.
- `AcKey::new(tenant_id, ...)` keying + `AcEnvelope::canonicalize`
  preimage rebuild from row fields (NEVER trust envelope bytes) on
  the GET hot path preserved.
- `MissMarker`-flavored audit emit hook on `GetSigInvalid` /
  `UpdateMerkleInvalid` / `UpdateOutputsMissing` etc. preserved.

### Hard pause triggers

See §7.

## §6. Gates run

All gates green at SEAL boundary (HEAD `44b3a409` on
`wt/r-prep-w33-stage2-pre-a-worker-megafiles`):

| Gate | Result |
|---|---|
| `cargo build --workspace` | green (~1 min clean build) |
| `cargo test -p corelink-worker` | green (95 tests at parity) |
| `cargo test -p corelink-worker --all-features` | green (389 tests at parity; Δ=0 vs. pre-split baseline) |
| `cargo clippy --workspace --all-targets -- -D warnings` | green (full workspace) |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | green |
| `python3 scripts/validate_specs.py` | green (449 with schema, 9 YAML; 458 total) |
| `python3 scripts/validate_references.py` | green (zero dangling) |
| `python3 scripts/check_migrations_additive.py` | green (59 files) |
| `python3 scripts/validate_inv_inheritance.py` | green (16 child chains across 8 parent targets) |

Note on `cargo build --workspace --all-features`: not run because
the workspace contains mutually-exclusive `corelink-byok` provider
features (`azure` vs. `vault`) that intentionally `compile_error!`
when both are enabled simultaneously. This is a pre-existing
constraint orthogonal to this dispatch (the byok crate is not
touched). The default-feature workspace build + the per-crate
`--all-features` build for `corelink-worker` exercise the full
relevant surface.

### LOC verification (per-sub-step boundary)

```
$ find crates/corelink-worker/src/auth/revocation* \
       crates/corelink-worker/src/reapi/cas/split_splice* \
       crates/corelink-worker/src/middleware/timing_padding* \
       crates/corelink-worker/src/reapi/ac/handler* \
       -name "*.rs" | xargs wc -l | sort -rn | head
```

Top-5 (post-PRE-A.4):

```
    456 crates/corelink-worker/src/reapi/ac/handler/methods.rs
    452 crates/corelink-worker/src/auth/revocation/types.rs
    450 crates/corelink-worker/src/auth/revocation/tests.rs
    426 crates/corelink-worker/src/middleware/timing_padding/tests.rs
    414 crates/corelink-worker/src/reapi/cas/split_splice/tests.rs
```

Every line ≤ 456 LOC. **No file exceeds 500 LOC.**

## §7. Hard pause triggers — status

Per the dispatch §"Hard pause triggers":

| # | Trigger | Status |
|---|---|---|
| 1 | Any extracted helper fn ends up > 500 LOC and cannot be further split without behavioural change | **NOT ACTIVATED** — no helper extractions needed; largest fn (`get_action_result_inner` at ~215 LOC) was kept whole as the natural module boundary |
| 2 | Previously-green test goes red (behavioural regression) | **NOT ACTIVATED** — 95/95 default-feature tests + 389/389 all-feature tests green at SEAL boundary (Δ=0 vs. pre-split baseline) |
| 3 | Public symbol path of any consumer breaks (the 63 consumer files identified in escalation report) | **NOT ACTIVATED** — every parent dispatch file re-exports the canonical surface; workspace build green confirms all consumers resolve at parity |
| 4 | `INV-CAS-*`, `INV-AC-*`, `INV-AUTH-*` invariant tests fail | **NOT ACTIVATED** — every invariant test green (`INV-AUTH-REVOCATION-IDEMPOTENT`, `INV-AUTH-MASS-REVOKE-ATOMIC`, `INV-MULTIPART-IDEMPOTENT`, `INV-MULTIPART-STREAMING-VERIFY-FAIL-FAST`, `INV-AC-OUTPUTS-VALID` exercised by the SEAL test corpus) |
| 5 | wasm32 build breaks | **NOT ACTIVATED** — `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` green |
| 6 | Workspace cargo build broken at any sub-step boundary | **NOT ACTIVATED** — verified after PRE-A.1, PRE-A.2, PRE-A.3, PRE-A.4 |
| 7 | `subtle::ConstantTimeEq` accidentally removed from timing_padding.rs hot path | **NOT APPLICABLE** — pre-split monolith has zero `subtle` usage; the timing defense is wall-clock-padding, not constant-time compare. Verified via grep at both pre- and post-split |
| 8 | `Cargo.lock` chaos | **NOT ACTIVATED** — no Cargo dependency changes; `Cargo.lock` untouched by this dispatch |

The mid-split `methods.rs` 520-LOC over-cap event during PRE-A.4
was a **near-trigger-1** that the charter's
"if any resulting file is >500, re-split further" rule resolved
in-band: the stash methods + `ActionResultStash` were lifted into
a sibling `stash.rs` (92 LOC), bringing `methods.rs` to 456 LOC.
This rebalance happened in the same commit (`44b3a409`); the
mid-split state never landed on the branch.

## §8. Next steps

### Merge to main

`wt/r-prep-w33-stage2-pre-a-worker-megafiles` (head: `44b3a409`) is
ready to merge. Branch contains 4 SEAL'd commits + this audit doc
commit.

### Stage 2.PRE-B / Stage 2.A continuation

Per the sequential safe-execution plan ("dispatch 1/7" in the
dispatch §"Background — why this PRE-step exists"), Stage 2.PRE-B
is the next dispatch. The 4 corelink-worker mega-files are now
L2.10-compliant inside their existing crate, so Stage 2.A (the
piece-relocation phase that moves corelink-worker fragments to
context crates) can proceed against stable per-file moves — no
compounded refactor risk.

### Follow-on candidates (out of scope for this dispatch)

- Stage 2.A: relocate the now-decomposed pieces to the canonical
  context crates per wave-33 spec §5 row 5 (R2 reader/writer →
  `corelink-cas`; cache+middleware → `corelink-auth`; reapi adapter
  → `corelink-reapi`; region resolver → `corelink-replication`;
  auth → `corelink-auth`). The submodule layout this dispatch
  produced (parent dispatch file + topical submodule directory)
  makes the moves directory-grained rather than thousand-LOC-grained.
- Future mega-file landings: re-run this pattern.

## §9. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

**End of Wave 33 Stage 2 PRE-A — corelink-worker Mega-File Decomposition SEAL audit.**
