# TLA+ Spec Comment-Anchor Update — SEAL

**Date:** 2026-05-27
**Agent:** TLA-COMMENT-UPDATE
**Mandate source:** `specs/_audits/2026-05-27-tla-drift-audit.md` (commit `9eca41fd`)
**Branch / worktree:** `worktree-agent-a387aaa3aa14ee83e`

---

## 1. Scope

Update stale `crates/corelink-<absorbed>/...` path **comment anchors** inside
TLA+ specs after Wave 35 Phase 2 crate absorptions (107 → 68) and Wave 33
module reorganisation. These anchors are documentation cross-references —
they are **NOT** TLA+ EXTENDS / module imports — so updating them is a pure
comment change and cannot affect state-space, operators, invariants, or
model behaviour.

**Hard constraints honoured:**

- Zero changes to TLA+ logic, actions, invariants, operators, or modules.
- Zero changes to `.cfg` files (TLC config: constants, properties).
- Zero TLC runs claimed. Re-verification of invariants requires the TLA+
  Toolbox and was explicitly out of scope per mandate §3 / §8.
- No `--no-verify`; no merge markers introduced.

## 2. Files modified — 15 specs

(The drift-audit headline mentioned "16 spec files" but its §5 per-spec
table actually enumerates 15 distinct files; all 15 are addressed below.)

| # | Spec file | Anchors updated | Notes |
|---|---|---|---|
| 1 | `specs/tla/ac_integrity.tla` | 2 | Wave 33 reorg: `corelink-ac/src/{sig,merkle}.rs` → `corelink-ac/src/ac_core/{sig,merkle}.rs` |
| 2 | `specs/tla/audit_no_raw_pii.tla` | 2 | `corelink-obs/src/tracing_filter.rs` → `corelink-telemetry/src/logpush/redaction.rs`; **redactor.rs flagged TODO (ambiguous)** |
| 3 | `specs/tla/auth_audit_pseudonymization.tla` | 1 | **pseudonymize.rs flagged TODO (ambiguous: auth/schema vs privacy)** |
| 4 | `specs/tla/auth_schema_rls_default_on.tla` | 1 | `corelink-auth/src/orm.rs` → `corelink-auth/src/schema/` (directory anchor; module split) |
| 5 | `specs/tla/backup_fresh.tla` | 1 | `corelink-backup-verify/src/lib.rs` → `corelink-ops/src/dr/backup_verify.rs` |
| 6 | `specs/tla/backup_restore_ephemeral.tla` | 1 | same absorption as above |
| 7 | `specs/tla/cas_immutability.tla` | 1 | `corelink-cas/src/store.rs` → `corelink-cas/src/dedup.rs` (Wave 33 split; note added re: manifest.rs) |
| 8 | `specs/tla/consent_proof_verifiable.tla` | 1 | `corelink-privacy-consent-ledger/src/lib.rs` → `corelink-privacy/src/consent.rs` |
| 9 | `specs/tla/digest_verification.tla` | 1 | `corelink-worker/src/r2_put.rs` → `corelink-worker/src/storage/r2.rs` |
| 10 | `specs/tla/multipart_determinism.tla` | 2 | `corelink-chunker/src/fastcdc.rs` → `corelink-cas/src/chunker/fastcdc.rs`; `corelink-multipart/src/adapter.rs` → `corelink-r2-multipart/src/adapter.rs` |
| 11 | `specs/tla/obs_no_pii.tla` | 2 | `corelink-otel-export/src/{lib,audit}.rs` → `corelink-telemetry/src/otel/{exporter,audit}.rs` |
| 12 | `specs/tla/offboarding_audit_complete.tla` | 1 | `corelink-tenant-offboarding/src/lib.rs` → `corelink-ops/src/tenant_offboarding.rs` |
| 13 | `specs/tla/rollout_cosign_gate.tla` | 1 | `corelink-rollout-controller/src/lib.rs::start` → `corelink-replication/src/rollout_controller/controller.rs::start` |
| 14 | `specs/tla/signup_token_idempotent.tla` | 2 | `apps/server/src/routes/signup.rs` (legacy pre-workspace monolith) → `crates/corelink-signup/src/{orchestrator,request,idempotency}.rs`; archival wave-29 commit `b3c359f` preserved as historical reference |
| 15 | `specs/tla/sub_processor_audit_fail_closed.tla` | 1 | `corelink-privacy-sub-processors/src/lib.rs` → `corelink-privacy/src/sub_processor.rs` |

**Totals:** 15 files, ~20 anchor entries updated, 47 lines inserted / 19
deleted (per `git diff --numstat`). All inserts are inside `(* ... *)`
TLA+ comment delimiters.

## 3. Ambiguous cases — TODO-flagged (per mandate §3)

Two stale references could not be unambiguously remapped from the audit
alone; both are now annotated inline with a `TODO(tla-drift-2026-05-27)`
comment so future readers can disambiguate before the next TLC run:

| Original reference | Candidates | Spec |
|---|---|---|
| `crates/corelink-audit/src/pseudonymize.rs` | `corelink-auth/src/schema/pseudonymize.rs` **or** `corelink-privacy/src/pseudonymize.rs` | `auth_audit_pseudonymization.tla` |
| `crates/corelink-audit/src/redactor.rs` | possibly renamed to `corelink-audit/src/redact.rs`, or a separate module | `audit_no_raw_pii.tla` |

These TODOs are pure comments — they do not block TLC and do not alter
any operator definition.

## 4. Acceptance evidence

```text
chunker:                0 stale (expect 0)
backup-verify:          0 stale (expect 0)
tenant-offboarding:     0 stale (expect 0)
otel-export:            0 stale (expect 0)
privacy-consent-ledger: 0 stale (expect 0)
privacy-sub-processors: 0 stale (expect 0)
rollout-controller:     0 stale (expect 0)
multipart:              0 stale (expect 0)
obs:                    0 stale (expect 0)

merge markers (<<<<<<<, >>>>>>>): NONE
TODO(tla-drift-2026-05-27) anchors present: 2 (as expected)
```

All 9 absorbed-crate prefixes return **zero** remaining hits in `*.tla`
files. No merge markers introduced. `.cfg` files untouched (`git status`
shows only `specs/tla/*.tla`).

## 5. What was intentionally NOT done

- **No TLC re-run** — out of scope; requires TLA+ Toolbox + heavy compute.
  These were documentation anchors, not module imports, so the state space
  and operators are byte-identical modulo comments.
- **No `.cfg` edits** — config files contain TLC properties/invariants
  only; no `crates/...` references exist there per audit §5 footnote.
- **No edits to other 35 specs** — they had no stale anchors.
- **No semantic rewording** — only path strings inside the same anchor
  lines were changed (plus brief "(post Wave 35 P2: …)" parentheticals).

## 6. Follow-ups

1. Disambiguate the 2 TODO-flagged references via Wave 35 P2 audit/privacy
   SEAL logs and remove the TODO once resolved.
2. When the TLA+ Toolbox CI lane is available, run `tlc` against each of
   the 15 modified specs to certify that only comments changed (invariants
   must remain green; the diff should be model-irrelevant).
3. Consider a CODEOWNERS / pre-commit hook to detect new
   `crates/corelink-<name>/` references inside `.tla` files and require
   them to point at extant workspace members.

---

*SEAL closed — comment-only update; spec semantics unchanged; TLC re-run
deferred per mandate §3 / §8.*
