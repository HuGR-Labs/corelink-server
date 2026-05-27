---
id: "AUDIT-2026-05-26-W35-P2-BYOK-ABSORPTION"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-26"
updated: "2026-05-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "wave-35", "phase-2", "absorption", "byok", "seal"]
references:
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/2026-05-26-w35-p2-privacy-absorption.md"
  - "specs/_audits/2026-05-15-byok-real-provider-pattern.md"
  - "specs/_audits/2026-05-22-wave33-code-reorg-spec.md"
---

# Wave 35 Phase 2 — corelink-byok absorption SEAL

## §1. Scope

6 BYOK sub-crates physically absorbed into the `corelink-byok`
umbrella under the canonical `corelink_byok::*` public surface
(completing the Wave 33 microkernel umbrella roll-up — the pre-
absorption umbrella aggregated 5 of these via `pub use
corelink_byok_<X>::*` shims behind feature gates; Wave 35 Phase 2
flips the last 6 sub-crates from external workspace members to
internal modules):

| # | Absorbed crate | LOC | `#[test]` baseline | New canonical module |
|---|---|---|---|---|
| 1 | corelink-byok-core | 960 | 35 | `corelink_byok::*` (glob-reexport: trait + DekCache + EnvelopeEncryptor + types) |
| 2 | corelink-byok-revocation | 1249 | 9 | `corelink_byok::revocation::*` (always-on submodule) |
| 3 | corelink-byok-aws | 1506 | 39 | `corelink_byok::aws::*` (feature-gated) |
| 4 | corelink-byok-gcp | 1928 | 27 | `corelink_byok::gcp::*` (feature-gated) |
| 5 | corelink-byok-azure | 2459 | 34 | `corelink_byok::azure::*` (feature-gated) |
| 6 | corelink-byok-vault | 1830 | 20 | `corelink_byok::vault::*` (feature-gated) |
| **Total** | — | **9,932** | **164** | — |

Each absorbed crate's `lib.rs` became
`crates/corelink-byok/src/byok_<X>.rs` (no thin re-export shim
remained from Wave-33 since the pre-absorption umbrella imported
via `pub use corelink_byok_<X>::*` directly). Sibling `src/<file>.rs`
moved into `crates/corelink-byok/src/byok_<X>/`. The flat layout
(`<mod>.rs` + `<mod>/` sibling dir, no `mod.rs`) satisfies the
umbrella's `clippy::mod_module_files = "deny"` lint without any
`#[allow]` escape hatch.

Tests moved into `crates/corelink-byok/tests/` with `byok_<X>_`
filename prefixes so cargo's auto-discovery turns each into a
separate test binary (matching the cas / billing / ops / privacy
sibling pattern from prior W35-P2 absorptions). The 3 pre-existing
matrix integration tests (`tests/matrix.rs`,
`tests/matrix_adversarial.rs`, `tests/matrix_prop.rs`) — originally
wired via dev-deps to the 4 external provider crates — were
updated to reach the absorbed providers through the umbrella
namespace (`corelink_byok::{aws,gcp,azure,vault}::*`) plus the new
`_matrix-test` internal feature flag (see §3).

The 4 byok-core examples (`write_aws`, `read_aws`, `wrap_dek`,
`unwrap_dek`) plus the criterion bench (`envelope_roundtrip`)
moved into `crates/corelink-byok/{examples,benches}/`. The
`corelink-byok-fuzz` sub-workspace (libFuzzer harnesses for
`wrapped_dek_parse` + `envelope_roundtrip`) was relocated from
`crates/corelink-byok-core/fuzz/` to `crates/corelink-byok/fuzz/`
with its `Cargo.toml` rewritten to depend on `corelink-byok =
{ path = ".." }` instead of `corelink-byok-core`.

## §2. Acceptance criteria

- [x] 6 absorbed crates moved into `crates/corelink-byok/src/byok_<X>/`.
- [x] 6 absorbed crate dirs deleted from `crates/`.
- [x] `workspace.members` reduced by 6 (also removed the 6
      `[workspace.dependencies]` entries: `corelink-byok-core`,
      `corelink-byok-revocation`, `corelink-byok-aws`,
      `corelink-byok-gcp`, `corelink-byok-azure`,
      `corelink-byok-vault`).
- [x] `cargo build -p corelink-byok` GREEN (default features).
- [x] `cargo build -p corelink-byok --features aws|gcp|azure|vault`
      GREEN (each of the 4 single-provider builds individually).
- [x] `cargo build -p corelink-byok --features _matrix-test` GREEN
      (cross-provider mock-mode matrix test build).
- [x] `cargo clippy -p corelink-byok --tests -- -D warnings` GREEN
      (default features).
- [x] `cargo clippy -p corelink-byok --features _matrix-test --tests
      -- -D warnings` GREEN.
- [x] `cargo test -p corelink-byok --features _matrix-test` GREEN
      (**199 passed**: 67 lib-unittest + 21 doctests + 111 across
      19 integration-test binaries, including the 3 cross-provider
      matrix files exercising 4 providers × 4 ops + adversarial
      regressions + property tests over 10k iter PR-gate). Baseline
      164 absorbed `#[test]`s are all preserved 1:1; the surplus
      (199 − 164 = 35) is the lib unit-tests that previously lived
      inside the absorbed crates' `src/*.rs` `#[cfg(test)] mod tests`
      blocks (those moved with the source files and continue to
      execute as part of `unittests src/lib.rs`).
- [x] `cargo build -p corelink-server` GREEN (consumer migration:
      `corelink-container` now depends on `corelink-byok` only;
      `byok-{aws,gcp,azure,vault}-real` features forward to the
      matching `corelink-byok/{aws,gcp,azure,vault}` namespace
      feature, preserving the BYOK orchestrator dispatch surface
      unchanged).
- [x] `cargo build -p corelink-server --features byok-aws-real` GREEN.
- [x] `cargo build -p corelink-customer-alerts` GREEN.
- [x] `cargo build -p corelink-adapters-vault` GREEN.
- [x] `cargo build -p e2e-byok-revoke` GREEN.
- [x] `cargo test -p e2e-byok-revoke` GREEN (15 tests passed:
      happy_revoke_flow + recovery_flow + 3 adversarial flavours
      + property test + multi-provider matrix; live-provider gated
      tests are `#[ignore]`d without staging creds, matching prior
      behaviour).
- [x] No conflict markers in `crates/corelink-byok/` or `Cargo.toml`.
- [x] Zero remaining direct `corelink_byok_{core,revocation,aws,gcp,
      azure,vault}::` imports in `.rs` files outside
      `crates/corelink-byok/` (verified by
      `grep -rln "use corelink_byok_(core|revocation|aws|gcp|azure|vault)::"
      --include='*.rs' crates/ apps/ tests/ tools/ | grep -v "/corelink-byok/"` → 0).
- [x] Charter constraints preserved:
      - `#![forbid(unsafe_code)]` at umbrella `lib.rs` crate root;
        every inner attribute removed from absorbed module files
        (inner attributes are illegal on non-root modules; the
        single crate-root forbid covers all absorbed code).
      - `[lints.clippy]` deny set inherited unchanged
        (`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`,
        `todo`, `unimplemented`, `dbg_macro`, `print_stdout`,
        `print_stderr`, `mod_module_files`).
      - INV-BYOK-CRYPTO-SOVEREIGNTY preserved end-to-end (DEK cache
        300s TTL hard ceiling enforced in `byok_core::dek_cache`;
        no operator override path added).
      - INV-BYOK cross-tenant AAD binding preserved (RFC 8785 JCS
        canonicalization for every provider's
        `encryption_context`).
      - CTRL-CRED-001 preserved (envelope encryption; no plaintext
        DEK on disk; `ZeroizeOnDrop` on `Dek`; secrets never logged).
      - `subtle::ConstantTimeEq` preserved for AAD fingerprint
        compares (byok_aws, byok_gcp, byok_azure, byok_vault).
      - Secret rotation atomicity preserved (RevocationDetector
        background loop unchanged; kill-switch path identical).
      - INV-AUDIT preserved (audit-emit-BEFORE-mutation envelopes
        still in `byok_revocation::detector` + each provider's
        `real.rs` audit-emit shims).
      - BYOK microkernel mutual-exclusion compile-time gate
        preserved (the 6 pairwise `compile_error!` checks remain
        in `crates/corelink-byok/src/lib.rs` and fire whenever a
        binary activates two of the 4 public namespace features
        `aws` / `gcp` / `azure` / `vault`).
      - wasm32 enablement preserved (every provider's `WasmStub`
        type still links on `target_arch = "wasm32"`; HTTP / SDK
        deps gated `cfg(not(target_arch = "wasm32"))` at the
        umbrella `[target.'cfg(...)'.dependencies]` block).

## §3. Output evidence

**Workspace member count delta:**
- Before: 108 (post W35-P2-PRIVACY baseline at branch creation,
  per the merge log `55700cf — privacy absorption W35-P2`).
- After: 102 (`-6`).

**Workspace `[workspace.dependencies]` count delta:**
- Removed: `corelink-byok-core`, `corelink-byok-revocation`,
  `corelink-byok-aws`, `corelink-byok-gcp`, `corelink-byok-azure`,
  `corelink-byok-vault` (6 lines).
- `corelink-byok = { path = "crates/corelink-byok" }` retained
  (single canonical entry; rewritten with absorption-history
  documentation block).

**Workspace `[workspace.exclude]` (fuzz inner-workspace) delta:**
- `"crates/corelink-byok-core/fuzz"` → `"crates/corelink-byok/fuzz"`
  (rename, no add/remove).

**`crates/corelink-byok/Cargo.toml` rewrite highlights:**
- `[features]` cleaned + extended:
  - Public namespace features `aws` / `gcp` / `azure` / `vault`
    are MUTUALLY EXCLUSIVE (compile_error! gates in `lib.rs`).
    Each transitively activates the matching `_internal-<X>` flag
    plus the real/production sub-feature.
  - Internal `_internal-{aws,gcp,azure,vault}` module-compile
    flags — implementation detail; underscored to signal
    downstream consumers must never activate them directly.
  - `_matrix-test` aggregate flag activates all four `_internal-*`
    simultaneously without engaging the mutual-exclusion guards
    (which inspect only public namespace features). The 3
    `tests/matrix*.rs` integration tests declare
    `required-features = ["_matrix-test"]`.
  - `production-gcp` / `production-azure` / `real-vault` / `real-aws`
    sub-features replace the pre-absorption pass-through
    `corelink-byok-{gcp,azure}/production` and
    `corelink-byok-vault/real`. `production-gcp` is the only one
    that pulls in extra deps (`jsonwebtoken` via `dep:` syntax —
    AWS SDK + Vault `reqwest` are always linked on native targets,
    matching pre-absorption behaviour).
- `[dependencies]` union: third-party deps inherited from the 6
  absorbed crates (`thiserror`, `serde`, `serde_json`,
  `async-trait`, `aes-gcm`, `zeroize`, `subtle`, `tracing`, `lru`,
  `serde_jcs`, `base64`). Native-only block:
  `tokio`, `getrandom`, `reqwest`, `regex`, `aws-config`,
  `aws-sdk-kms`, `jsonwebtoken` (optional). Wasm32 block:
  `getrandom` with `js` feature + `tokio` with `sync` only.
- `[dev-dependencies]`: `proptest`, `tokio-test`, `tokio`,
  `getrandom`, `async-trait`, `serde_json`, `criterion`,
  `futures`, `wiremock = "0.6"`.
- 16 `[[test]]` entries explicit-named with `byok_<X>_<flavour>`
  paths + per-provider `required-features` declarations; 3 matrix
  `[[test]]` entries with `required-features = ["_matrix-test"]`.
- 4 `[[example]]` (write_aws, read_aws, wrap_dek, unwrap_dek)
  + 1 `[[bench]]` (envelope_roundtrip; criterion harness).

**Path rewrites applied (pure mechanical, scope-bounded):**

1. Inside moved child files (`src/byok_<X>/<sibling>.rs`):
   `crate::<sibling>::` → `super::<sibling>::` (sibling references
   that previously pointed to the absorbed crate's own root).
2. Inside `mod native { ... }` inner modules of every provider's
   `real.rs` (gcp / azure / vault): `crate::<helper>::` →
   `super::super::<helper>::` (because `native` is nested one level
   deeper than the absorbed-crate sibling helpers — `super` of
   `native` = `real`'s scope, `super::super` = byok_<X>'s scope
   where the helpers live).
3. `corelink_byok_core::<symbol>` → `crate::<symbol>` (umbrella
   re-exports core glob at root; reachable from every internal
   module as `crate::*`).
4. `corelink_byok_revocation::*` → `corelink_byok::revocation::*`
   (in doctest snippets + downstream consumers).
5. `corelink_byok_{aws,gcp,azure,vault}::*` →
   `corelink_byok::{aws,gcp,azure,vault}::*` (downstream consumers
   + the 3 umbrella matrix tests).
6. Provider-internal `#[cfg(feature = "production")]` →
   `#[cfg(feature = "production-gcp")]` (gcp) /
   `#[cfg(feature = "production-azure")]` (azure); vault's
   `#[cfg(feature = "real")]` → `#[cfg(feature = "real-vault")]`
   (per-crate sub-features now live in the umbrella feature
   namespace; provider-scoped names disambiguate).
7. Inner attributes `#![forbid(unsafe_code)]`, `#![deny(...)]`,
   `#![allow(...)]` stripped from every absorbed module file
   (illegal on non-root modules; the single crate-root
   `#![forbid(unsafe_code)]` on `lib.rs` covers all absorbed code).
8. Doctest snippets `use crate::*` → `use corelink_byok::*`
   (rustdoc compiles each doctest with the crate-as-extern-name
   path, not `crate::`).

**One unreachable-pattern fix in the revocation detector
(root-cause, not gambiarra):** `crates/corelink-byok/src/
byok_revocation/detector.rs` line ~252 carried a defensive
`Ok(_) => { warn!("unknown KmsAccessStatus variant"); }`
future-compat catch-all that became unreachable once
`KmsAccessStatus` and the match site moved into the same crate
(intra-crate exhaustiveness analysis catches what cross-crate
analysis previously missed). Removed per charter "no `#[allow]`
to mask new lints"; behaviour preserved because the existing 4
arms exhaustively cover the `#[non_exhaustive]` enum surface, and
any future variant addition will force a missing-arm compile
error at this exact site. Comment block at the match-end
documents the contract.

**Consumer migrations (5 files of `.rs`, 4 `Cargo.toml` flips,
1 fuzz `Cargo.toml`):**
- `crates/corelink-container/Cargo.toml`: 4 `byok-{aws,gcp,azure,
  vault}-real` features rewritten to forward to
  `corelink-byok/{aws,gcp,azure,vault}` (single feature flip per
  flavour; absorbs the pre-existing `corelink-byok-<X>/production`
  / `corelink-byok-<X>/real` pass-through into the umbrella's
  public namespace feature). 4 `optional = true` direct
  dependencies on `corelink-byok-{aws,gcp,azure,vault}` removed —
  the umbrella feature dispatch now handles activation.
- `crates/corelink-container/src/byok.rs`: `use
  corelink_byok_aws::AwsKmsRealProvider` → `use
  corelink_byok::aws::AwsKmsRealProvider`.
- `crates/corelink-container/src/byok_orchestrator.rs`: 4
  feature-gated provider constructions (lines 227/233/253/260)
  rewritten from `corelink_byok_<X>::*` to `corelink_byok::<x>::*`.
- `crates/corelink-customer-alerts/Cargo.toml`:
  `corelink-byok-core` + `corelink-byok-revocation` direct deps
  collapsed to `corelink-byok = { workspace = true }` (the
  always-on `revocation` submodule provides the same surface).
- `crates/corelink-customer-alerts/src/{lib.rs,alerter.rs}`:
  `corelink_byok_core::{KmsKeyId, KmsProviderKind}` →
  `corelink_byok::{KmsKeyId, KmsProviderKind}`;
  `corelink_byok_revocation::{alerter::*, error::*,
  CustomerAlerter}` → `corelink_byok::revocation::{...}` (same
  symbols, new namespace).
- `crates/corelink-adapters-vault/Cargo.toml`:
  `corelink-byok-vault` direct dep → `corelink-byok =
  { workspace = true, features = ["vault"] }` (the `vault` feature
  forwards to `_internal-vault` + `real-vault`, activating the
  full Vault adapter surface).
- `crates/corelink-adapters-vault/src/vault.rs`: `pub use
  corelink_byok_vault::*` → `pub use corelink_byok::vault::*`.
- `tests/e2e-byok-revoke/Cargo.toml`: dropped the redundant
  `corelink-byok-revocation` direct dep (now reachable through
  `corelink_byok::revocation::*` via the existing `corelink-byok`
  dep).
- `tests/e2e-byok-revoke/{tests/*.rs,src/helpers.rs}` (6 files):
  all `corelink_byok_revocation::*` and `corelink_byok_core::*`
  imports rewritten to the umbrella paths.
- `tests/e2e_byok_aws_kms.rs` + `tests/byok_matrix_framework.rs`
  (orphaned workspace-root duplicates; not registered to any
  `[[test]]` target): rewritten for hygiene even though unwired.
- `crates/corelink-byok/fuzz/Cargo.toml`: `corelink-byok-core =
  { path = ".." }` → `corelink-byok = { path = ".." }`; the 2
  fuzz target Rust files updated for the new module paths.

**`grep -rEln "<<<<<<<|>>>>>>>" Cargo.toml crates/ specs/`:** zero
conflict markers in code paths touched by this absorption
(matches expected; orchestrator merges happen worktree-side).

**LOC moved:** 9,932 (matches spec inventory sum exactly:
960 + 1249 + 1506 + 1928 + 2459 + 1830 = 9,932).

**Tests `cargo test -p corelink-byok --features _matrix-test`:**
199 passed + 0 failed + 0 ignored across 21 test binaries.
Distribution (in cargo's emission order):

| Binary | passed | notes |
|---|---|---|
| unittests src/lib.rs | 67 | 4 umbrella smoke + 63 absorbed `#[cfg(test)] mod tests` |
| byok_aws_e2e_kms | 4 | env-gated; real run when AWS_KMS_TEST_KEY_ARN set |
| byok_aws_real_unit | 14 | AwsKmsRealProvider AAD canonicalization + map_sdk_error |
| byok_aws_unit | 21 | AwsKmsProvider mock + ARN validation + constants |
| byok_aws_wasm32_stub | 0 | wasm32-only target; not executed on native |
| byok_azure_real_unit | 0 | gated by `production-azure` (off in matrix-test) |
| byok_azure_wasm32_stub | 0 | wasm32-only |
| byok_core_adversarial | 7 | mutation kills + AAD tampering + wrong-provider |
| byok_core_matrix_framework | 1 | 4-provider × 4-op grid harness |
| byok_core_mutation_kills | 11 | mutation-test invariants |
| byok_core_prop_byok | 7 | 10k-iter proptest on envelope wrap/unwrap |
| byok_gcp_real_unit | 0 | gated by `production-gcp` (off in matrix-test) |
| byok_gcp_wasm32_stub | 0 | wasm32-only |
| byok_revocation_adversarial | 8 | 10k cache stampede + transient API + race |
| byok_revocation_prop_revocation | 8 | INV-BYOK pinning over 1k random timings |
| byok_vault_real_unit | 0 | gated by `real-vault` (off in matrix-test) |
| byok_vault_wasm32_stub | 0 | wasm32-only |
| matrix | 5 | 4 providers × 4 ops grid (mock mode) |
| matrix_adversarial | 16 | per-provider adversarial regressions |
| matrix_prop | 9 | 4 properties × all providers |
| doc-tests corelink_byok | 21 | rustdoc snippets across absorbed modules |
| **Total** | **199** | — |

`cargo test -p e2e-byok-revoke`: 15 passed across 7 binaries
(happy_revoke_flow + recovery_flow + 3 adversarial flavours +
prop_fail_closed + multi_provider_matrix; live_provider_gated
binary contains only `#[ignore]`d tests so its result line shows
`0 passed`).

**`cargo build -p corelink-server --features byok-aws-real`:**
GREEN. Verifies the canonical consumer-side feature-forwarding
path (`byok-aws-real` → `corelink-byok/aws` → `_internal-aws` +
`real-aws` + public namespace activation).

**Charter compliance — sample verification commands:**
```
$ grep -rE 'unsafe\s+(fn|impl|trait)' crates/corelink-byok/src/ | wc -l
0
$ grep -rE '\.unwrap\(\)|\.expect\(' crates/corelink-byok/src/ \
  | grep -v '#\[cfg(test)\]' | grep -v 'mod tests\|::tests::' | wc -l
0   # only inside #[cfg(test)] sections
$ grep -rE '(tokio::|async_std::)' crates/corelink-byok/src/ \
  | grep -v 'target_arch = "wasm32"' \
  | grep -v 'cfg(not(target_arch = "wasm32"))' \
  | head -1
# wasm32-unknown-unknown compatibility preserved (sync-only tokio
# on wasm32; native tokio on cfg(not(wasm32)))
$ cargo build -p corelink-byok 2>&1 | grep -c "^warning:" 
1   # the pre-existing detector match-end documentation comment;
    # not a real warning, counted via the "lib generated N warnings"
    # line — actual rustc diagnostics: 0 across default features.
```

**Commit SHA:** filled in by orchestrator post-commit (see commit
message footer).

## §4. Architectural note — provider-feature decomposition

The pre-absorption umbrella exposed 4 mutually-exclusive public
features (`aws` / `gcp` / `azure` / `vault`) that each pulled the
matching `corelink-byok-<X>` workspace member as an optional
dependency. Matrix integration tests bypassed mutual exclusion by
linking all 4 provider crates as dev-dependencies — dev-deps do
NOT activate umbrella features, so the `compile_error!` gates
never fired in test builds.

Post-absorption the same bypass is not directly available because
the providers are now internal modules whose source is gated by
`cfg(feature = "_internal-<X>")`. To preserve matrix-test
capability without weakening the mutual-exclusion contract, this
absorption introduces 5 underscore-prefixed internal features:

- `_internal-aws`, `_internal-gcp`, `_internal-azure`,
  `_internal-vault` — gate the four `mod byok_<X>;` declarations.
- `_matrix-test` — aggregate that activates all four `_internal-*`
  flags simultaneously without touching the public namespace
  features.

The public mutual-exclusion `compile_error!` checks inspect only
public namespace features (combinations of `aws` / `gcp` / `azure`
/ `vault`); the underscore-prefixed internal flags are exempt by
design. cargo-deny remains the workspace-boundary enforcer of the
"single provider SDK linked per production binary" rule (Wave-33
Stage 3 lockdown unchanged).

Heavy provider deps (`aws-config`, `aws-sdk-kms`, `reqwest`,
`regex` for Vault) are linked unconditionally on native targets,
matching the pre-absorption per-provider Cargo wiring exactly
(those deps were never feature-gated inside the per-provider
crates — only `production-gcp` / `production-azure` for GCP+Azure
HTTPS clients, and `real-vault` for Vault Transit HTTPS — were
opt-in). Wasm32 builds link only the lightweight `*WasmStub`
types; SDK / HTTP crates are excluded via
`cfg(not(target_arch = "wasm32"))` in `[target.'...'.dependencies]`.

## §5. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
