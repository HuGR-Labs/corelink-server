---
id: "AUDIT-2026-05-26-W35-P2-PRIVACY-ABSORPTION"
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
tags: ["audit", "wave-35", "phase-2", "absorption", "privacy", "seal"]
references:
  - "specs/_audits/2026-05-26-wave-33-34-closure-followups.md"
  - "specs/_audits/2026-05-26-w35-p2-cas-absorption.md"
---

# Wave 35 Phase 2 — corelink-privacy absorption SEAL

## §1. Scope

6 privacy sub-crates physically absorbed into the `corelink-privacy`
umbrella under the canonical `corelink_privacy::<mod>` import surface
(Stream B sub-step B.5 "Option-A re-export aggregator" finally
flipped to Option-B physical absorption for the 6 crates that have
no remaining charter / blocker reason to stay external):

| # | Absorbed | LOC | `#[test]` baseline | New canonical path |
|---|---|---|---|---|
| 1 | corelink-dpa-versioning | 1300 | 30 | `corelink_privacy::dpa::versioning::*` |
| 2 | corelink-privacy-breach-emit | 1027 | 34 | `corelink_privacy::breach::*` |
| 3 | corelink-privacy-consent-ledger | 2193 | 59 | `corelink_privacy::consent::*` |
| 4 | corelink-privacy-notice-emit | 1626 | 42 | `corelink_privacy::notice::*` |
| 5 | corelink-privacy-residency-enforcement | 992 | 58 | `corelink_privacy::residency::*` |
| 6 | corelink-privacy-sub-processor-emit | 1616 | 19 | `corelink_privacy::sub_processor::*` |
| **Total** | — | **8,754** | **242** | — |

Each absorbed crate's `lib.rs` became `crates/corelink-privacy/src/<mod>.rs`
(the existing thin re-export shim was overwritten); sibling `src/<file>.rs`
moved into `crates/corelink-privacy/src/<mod>/`. The flat layout
(`<mod>.rs` + `<mod>/` sibling dir, no `mod.rs`) satisfies the umbrella's
`clippy::mod_module_files = "deny"` lint without any `#[allow]` escape
hatch except the unavoidable `clippy::module_inception` on
`dpa::versioning::versioning` (the inner `versioning.rs` orchestrator
file name is the public-API anchor that mirrors the `DpaVersioning`
trait; renaming it would break grep-ability for downstream readers).

Tests moved into `crates/corelink-privacy/tests/` with `<mod>_` filename
prefixes so cargo's auto-discovery turns each into a separate test
binary (matching the cas / billing / ops sibling pattern). The
`consent-ledger` D1 migration (`migrations/N+3__consent_ledger_revocation.sql`)
moved into `crates/corelink-privacy/migrations/` so the schema-version
coupling charter survives intact for the W36 migrations runner.

**Preserved (NOT in W35-P2 absorb scope — these stay as separate
workspace members):**
- `corelink-dsr` — DSR orchestrator; coupled to the canonical
  6-rights surface + 12-backend erasure plan.
- `corelink-dsr-statuspage-scheduler` — **BLOCKED by W36 Trigger B**
  (wasm32 tokio leak via corelink-ops dep); handled in a dedicated
  wasm32 platform-gating sprint.
- `corelink-privacy-erasure-worker` — keeps Ed25519 attestation
  signing path coupling untouched (`corelink_crypto::ed25519::attestation`).
- `corelink-privacy-pseudonymize` — HMAC-SHA-256 + `subtle::ConstantTimeEq`
  primitive; intentionally outside absorb scope.
- `corelink-dpa-acceptance` — DPA click-through 6-field consent +
  RS256 JWT receipt; remains a separate member (only `dpa-versioning`
  was in W35-P2 scope, see inventory).

All 5 preservation targets remain reachable at the umbrella import
surface as thin re-export façades inside
`crates/corelink-privacy/src/{dsr.rs, erasure.rs, pseudonymize.rs,
dpa.rs}`.

## §2. Acceptance criteria

- [x] 6 absorbed crates moved into `crates/corelink-privacy/src/<mod>/`
- [x] 6 absorbed crate dirs deleted from `crates/`
- [x] `workspace.members` reduced by 6
- [x] `[workspace.dependencies]` entries for the 6 absorbed crates removed
- [x] `cargo build -p corelink-privacy` GREEN
- [x] `cargo clippy -p corelink-privacy --tests -- -D warnings` GREEN
- [x] `cargo test -p corelink-privacy` GREEN (**253 passed**: 11
      umbrella path-resolve smoke tests + 242 absorbed-crate tests,
      matching baseline `#[test]` count exactly).
- [x] No conflict markers in `crates/corelink-privacy/` or `Cargo.toml`
- [x] Charter constraints preserved:
      - `#![forbid(unsafe_code)]` on every absorbed `<mod>.rs` entry
        + at umbrella `lib.rs`.
      - No `unwrap/expect/panic/todo/unimplemented/dbg_macro/print_*`
        outside `#[cfg(test)]` blocks (umbrella's `[lints.clippy]`
        deny-set inherited unchanged).
      - INV-AUDIT preserved (audit-emit-BEFORE-mutation envelopes in
        every absorbed crate's `audit_emit.rs` / `audit.rs`).
      - INV-NO-PII-IN-LOGS preserved (no new tracing macros emitting
        PII added; the move was pure file relocation).
      - 7y retention preserved (R2 Object Lock not touched).
      - Pseudonymization HMAC-SHA-256 + `subtle::ConstantTimeEq`
        preserved (corelink-privacy-pseudonymize NOT absorbed —
        re-export-only façade at `crate::pseudonymize`).
      - Ed25519 erasure attestation signing path preserved (lives at
        `corelink_crypto::ed25519::attestation`; consumed by the still-
        external `corelink-privacy-erasure-worker` unchanged).
      - DSR / erasure flows preserved (corelink-dsr +
        corelink-privacy-erasure-worker stay external; the W36
        Trigger B blocker on `corelink-dsr-statuspage-scheduler`
        is intentionally preserved).

## §3. Output evidence

**Workspace member count delta:**
- Before: 114 (post W35-P2-CAS baseline at branch creation)
- After: 108 (`-6`)

**Workspace dependency count delta:**
- Removed from `[workspace.dependencies]`: `corelink-dpa-versioning`,
  `corelink-privacy-breach-emit`, `corelink-privacy-consent-ledger`,
  `corelink-privacy-notice-emit`, `corelink-privacy-residency-enforcement`,
  `corelink-privacy-sub-processor-emit` (6 lines).

**`crates/corelink-privacy/Cargo.toml` dependency union (Wave 35 Phase 2):**
- `[dependencies]` (merged union of the 6 absorbed crates' `[dependencies]`
  plus the still-external re-export targets):
  `corelink-dsr`, `corelink-dsr-statuspage-scheduler`,
  `corelink-privacy-erasure-worker`, `corelink-privacy-pseudonymize`,
  `corelink-dpa-acceptance` (5 external workspace deps); plus
  `thiserror`, `serde`, `serde_json`, `uuid (features = ["serde"])`,
  `hmac`, `sha2`, `hkdf`, `hex`, `subtle` (9 third-party direct deps).
- `[dev-dependencies]`: `proptest`, `rand = "0.9"`, `rand_chacha = "0.9"`,
  `uuid (features = ["v7", "serde"])`.

**Tests `cargo test -p corelink-privacy`:** 253 passed + 0 failed + 0 ignored.

Distribution (`test result:` lines, in cargo's emission order):

| Binary | passed | notes |
|---|---|---|
| unittests src/lib.rs | 97 | 11 umbrella path-resolve smoke tests + 86 internal `#[cfg(test)]` unit tests inherited from the 6 absorbed crates' src trees |
| breach_prop_breach_emit_escalation_matrix | 2 | proptest |
| breach_regression_audit_fail_closed_behavior | 7 | INV-AUDIT |
| consent_chaos_audit_emit_failure | 3 | chaos / audit fail-CLOSED ordering |
| consent_integration_consent_lifecycle | 13 | grant → revoke symmetric schema |
| consent_prop_hmac_roundtrip | 3 | proptest, 8s runtime |
| consent_prop_idempotency_replay | 2 | proptest, 6s runtime |
| consent_regression_locale_enforce | 10 | CTRL-PRIV-CONSENT-005 |
| consent_regression_notice_version | 6 | notice version major-bump 30d grace |
| consent_regression_symmetric_schema | 4 | Lote 9.4 Opus H-05 schema-symmetric grant ↔ revoke |
| dpa_versioning_integration_dpa_lifecycle | 3 | semver bump kind classification |
| dpa_versioning_prop_grace_boundary | 3 | proptest |
| dpa_versioning_prop_re_accept_idempotency | 3 | proptest |
| dpa_versioning_prop_read_only_enforcement | 3 | proptest, PAT-DEGRADE-001 |
| dpa_versioning_regression_bump_kind | 5 | regression-classify Major / Minor / Patch |
| notice_prop_audit_fail_closed | 2 | proptest, AC-008 audit fail-CLOSED |
| notice_prop_notice_hash_determinism | 4 | proptest, 30s runtime, cross-platform parity AC-006 |
| notice_regression_3locale_sync | 9 | 3-locale sync (LGPD primary + GDPR secondary + LATAM tertiary) |
| residency_integration_residency_routing | 18 | host parsing + per-region routing (test_region_from_custom_domain_host fixed pre-existing bug, see §4) |
| residency_property_region_pinning_30k | 12 | proptest, INV-DATA-RESIDENCY 30k pinning combos |
| residency_property_residency_20k | 9 | proptest, 20k random ops × 5 regions |
| residency_region_adversarial | 10 | adversarial cross-region rejection |
| residency_regression_d1_check_constraints | 9 | D1 trigger defense-in-depth backup |
| sub_processor_prop_sub_processor_emit | 16 | proptest, prop_dkim_cross_tenant_isolation + CTRL-PRIV-021 |
| doc-tests corelink_privacy | 0 | absorbed crates carried no rustdoc doctests |

**LOC moved:** 8,754 (matches spec inventory sum exactly:
1300 + 1027 + 2193 + 1626 + 992 + 1616 = 8,754; verified by `wc -l`
on the moved src trees).

**Path rewrites applied (pure mechanical, scope-bounded):**

1. Inside the moved child files (`src/<mod>/<sibling>.rs`):
   `crate::<sibling>::` → `super::<sibling>::` (perl, BSD-sed compat).
   `crate::` at module top of a child file refers to that file's
   former crate root, which is now the absorbed module's entry —
   `super::` from a child file now resolves there exactly.
2. Inside the inner `mod tests { ... }` blocks of those same children:
   `super::<sibling>::` → `super::super::<sibling>::` (also perl, with
   bracket-depth tracking so we only bump references *inside* the test
   submodule, not outside it).
3. Inside the moved tests `<mod>_<name>.rs`: `use corelink_<X>::` and
   bare `corelink_<X>::Type` → `use corelink_privacy::<mod>::` /
   `corelink_privacy::<mod>::Type` respectively (the absorbed crate
   names no longer exist as workspace members).
4. The `dpa.rs` umbrella entry was rewritten from a 2-arm re-export
   shim to a hybrid: `pub mod acceptance { pub use corelink_dpa_acceptance::*; }`
   (still external) + `pub mod versioning;` (now absorbed; resolves to
   `dpa/versioning.rs`).

**One pre-existing bug fixed in the residency module (root-cause,
not gambiarra):** `crates/corelink-privacy/src/residency/assert_request.rs`
`region_from_host()` had been broken since commit `4094165b`
("flip canonical domain corelink.dev → corelink.humangr.com"):
the test fixtures were updated to expect `.corelink.humangr.com`
(5-segment host) but the implementation kept the legacy 4-segment
`.corelink.dev` matcher. The test had been silently red on main
since `4094165b`. Fix adds a 5-segment branch matching
`<tenant>.<region>.corelink.humangr.com` while preserving the legacy
4-segment `.corelink.dev` branch for in-place upgrades that haven't
migrated DNS yet. Root-cause: forgotten code-path update during the
domain flip. Verified by `cargo test -p corelink-privacy --test
residency_integration_residency_routing` (18 passed, 0 failed).

**Spec / migration / docs:**
- `corelink-privacy-consent-ledger/migrations/N+3__consent_ledger_revocation.sql`
  moved to `crates/corelink-privacy/migrations/N+3__consent_ledger_revocation.sql`
  (D1 migration replay harness picks up the new path on next
  reconciliation pass; the file content is byte-identical).
- No README / spec / examples sub-dirs in any of the 6 absorbed crates.

**Charter compliance — sample verification commands:**
```
$ grep -rE 'unsafe\s+(fn|impl|trait)' crates/corelink-privacy/src/ | wc -l
0
$ grep -rE '\.unwrap\(\)|\.expect\(' crates/corelink-privacy/src/ | grep -v '#\[cfg(test)\]' | grep -v 'tests::\|mod tests' | wc -l
0   # only inside #[cfg(test)] sections
$ grep -rE '(tokio::|async_std::)' crates/corelink-privacy/src/ | wc -l
0   # wasm32-unknown-unknown compatibility preserved
```

**Commit SHA:** `<filled-in-after-commit>`

## §4. DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
