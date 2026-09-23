# Ownership wave 016 — independent fuzz packages

Static/source-only ownership authoring for the ten independent fuzz Cargo
packages excluded from the workspace. Their parent directory is not ownership
evidence. The pinned remote `main` and integration branch are separate inputs;
no package is approved by this plan.

## Identity and bounded packages

Source pin: remote `main` was re-read as
`1177dad2ca2a9f21c29b5a118aa7944b77147798` on 2026-09-21. All ten listed
package trees/manifests and the root manifest were compared against the
integration source tree and had no path differences. Recheck the exact source
pin before authoring or review. The integration baseline is supplied per
dispatch packet. Default profile S applies; fuzz-target count alone does not
qualify a package for H.

| Cargo package | Manifest | Static target/dependency seed | Package-specific boundary |
|---|---|---|---|
| `corelink-cli-fuzz` | `tools/cli/fuzz/Cargo.toml` | Five bins: `cli_input`, `config_toml`, `json_deserialize`, `auth_resolution`, `secret_redaction_check`; direct path dep `corelink-cli` | Independent workspace; its five harnesses are one Cargo-package ownership unit. |
| `corelink-tenant-path-fuzz` | `crates/tenant-path/fuzz/Cargo.toml` | `derive_prefix`, `derive_prefix_extended`; direct path dep `corelink-tenant-path` | UUID v7 and zeroization are harness deps; don't infer broader tenant-service behavior. |
| `corelink-hash-fuzz` | `crates/corelink-hash/fuzz/Cargo.toml` | `digest_parse`, `verify_body`; direct path dep `corelink-hash` | Digest parser and streaming body harnesses are separate atomic paths. |
| `corelink-worker-fuzz` | `crates/corelink-worker/fuzz/Cargo.toml` | `r2_path`, `r2_put_get_roundtrip`; direct paths `corelink-worker`, `corelink-hash`, `corelink-tenant-path` | Description advertises R2-like behavior; never infer credentials, service use or runtime R2. |
| `corelink-meta-fuzz` | `crates/corelink-meta/fuzz/Cargo.toml` | `commit_put_roundtrip`, `audit_idempotency`; direct paths `corelink-meta`, `corelink-hash` | Identify in-memory/fake vs external persistence from source, not directory name. |
| `corelink-reapi-fuzz` | `crates/corelink-reapi/fuzz/Cargo.toml` | `proto_decode_batch_update`, `audit_request_id_total`, `parse_read_resource_name`; direct path `corelink-reapi` | Fuzz binaries have `test=false`, `doc=false`; do not call those fields evidence of execution. |
| `corelink-client-verify-fuzz` | `crates/corelink-client-verify/fuzz/Cargo.toml` | `verify_sync`, `verify_ffi`; direct path `corelink-client-verify` enables `stream` and `ffi` | Harness allows unsafe for C-ABI fuzzing while library denies unsafe; do not copy library policy to harness. |
| `corelink-byok-fuzz` | `crates/corelink-byok/fuzz/Cargo.toml` | `wrapped_dek_parse`, `envelope_roundtrip`; direct path `corelink-byok` | Source can exercise envelope logic only; no provider/KMS/secret/runtime claim. |
| `corelink-audit-chain-fuzz` | `crates/corelink-audit-chain/fuzz/Cargo.toml` | `merkle_append`, `jcs_canonicalize`; direct paths `corelink-audit-chain`, `corelink-analytics` | Analytics dependency is declared in this fuzz package; not proof of a production relation. |
| `corelink-ac-fuzz` | `crates/corelink-ac/fuzz/Cargo.toml` | `hkdf_expand`; direct deps are `hkdf` and `sha2`, with **no** `corelink-ac` dependency | Directory placement does not prove the harness tests the parent implementation; document only evidenced targets and source. |

Every author owns exactly the package's four canonical artifact paths: its
`own-<package>` skill and `REFERENCE.md`, `BLAST_RADIUS.md`, `MAINTENANCE.md`
under `docs/ownership/crates/<package>/`. Authoring worktrees are disjoint.
Do not edit parent implementation crates, manifests, fuzz code, CI, the
shared standard, registry, indexes, rollout, or campaign status.

## Acceptance axioms

**Success criteria:** a maintainer can route fuzz-package work, distinguish
fuzz target intent from execution, find its boundary and choose safe static
maintenance actions.

**Completeness criteria:** each independent manifest identity, all targets,
features and direct dependencies, exact harness calls/assumptions, inverse
consumers, source/re-export/workflow/build boundaries, failures, and explicit
unknowns are recorded without inferring test coverage from placement.

**Quality standards:** use the exact v1.3 candidate inputs in the controlled
extraction, atomic directed relations, falsifiable invariants, stable record
navigation and source evidence. Distinguish implementation owner, public
contract owner, composition root, runtime operator and review authority;
unverified escalation stays blocked.

**Definition of Done:** exact four-path diff; S-profile checks and
`git diff --check`; bounded checker/source provenance; complete procedure
schema and evidence states; then four separate, fresh, independent cold
`APPROVE` verdicts and lead scope verification before integration. Checker
PASS is structural only.

**Invariants:** Cargo package name is authoritative; target declaration is not
execution; fuzz placement is not implementation ownership; source reachability
is not runtime observation; fuzz inputs and unsafe boundaries are package-
specific; no credentials/provider/runtime state are inspected or used; any
byte change invalidates that artifact's prior review. No fuzz, Cargo, Rust,
build, network, GitHub, provider, production, database or storage operations
are in scope.

## Parallel dispatch and review

The source snapshot is distinct from the integration branch snapshot. Each
dispatch record pins both. Authors do not review their own artifacts. As each
author finishes, cold review is scheduled in fresh contexts up to available
fleet capacity; changes to an artifact return only that artifact to review.
Every remaining material relation must be documented or explicitly justified
as excluded—never omitted to fit size limits. Keep the v1.3 document caps and
cross-package relation identities; do not infer `corelink-ac` implementation
coverage from `corelink-ac-fuzz`'s folder.

No fuzz command or Rust package command is needed to author these documents,
and none may be run during this wave.

## Dispatch log

Source re-read by the lead: remote `main=1177dad2ca2a9f21c29b5a118aa7944b77147798`;
the ten fuzz trees, manifests and root `Cargo.toml` have no scoped diff from
that snapshot. A local divergent `main` ref in an isolated worktree is not a
substitute for the remote source pin. Integration baseline for this first
parallel dispatch is `8cdc02828132b9b6f03a3b57117b8140325f6762`.

| Package | Manifest | Author worktree/branch | State |
|---|---|---|---|
| `corelink-tenant-path-fuzz` | `crates/tenant-path/fuzz/Cargo.toml` | `/tmp/corelink-ownership-w016-tenant-path-fuzz`, `codex/w016-tenant-path-fuzz` | Candidate `88a1bc0249b8c63415ce0dd465bd0ea60512bc5e`; S checks pass; peer REL reconciliation and cold review pending |
| `corelink-hash-fuzz` | `crates/corelink-hash/fuzz/Cargo.toml` | `/tmp/corelink-ownership-w016-hash-fuzz`, `codex/w016-hash-fuzz` | Candidate `6ea9bdd5d1fe27635393f07dab477bba3a54b367`; four S checks pass; shared REL reconciliation and cold review pending |
| `corelink-worker-fuzz` | `crates/corelink-worker/fuzz/Cargo.toml` | `/tmp/corelink-ownership-w016-worker-fuzz`, `codex/w016-worker-fuzz` | Candidate `06c52efbf03fe190f4e2b84bf79c6d3313145168`; S checks pass; cold review pending |
| `corelink-meta-fuzz` | `crates/corelink-meta/fuzz/Cargo.toml` | `/tmp/corelink-ownership-w016-meta-fuzz`, `codex/w016-meta-fuzz` | Candidate `123fe34efe7dee6a9a8c7b2c14d7f65b815462f9`; S checks pass; inverse consumer/execution/escalation unknowns and cold review pending |
| `corelink-reapi-fuzz` | `crates/corelink-reapi/fuzz/Cargo.toml` | `/tmp/corelink-ownership-w016-reapi-fuzz`, `codex/w016-reapi-fuzz` | Candidate `41cc43faf591bf487431b5811739c2ef5a741a15`; S checks pass; cold review pending |
| `corelink-client-verify-fuzz` | `crates/corelink-client-verify/fuzz/Cargo.toml` | `/tmp/corelink-ownership-w016-client-verify-fuzz`, `codex/w016-client-verify-fuzz` | Candidate `425c19a18dee65b421676da1b5d8b8d0e3002ceb`; S checks pass; conflicting historical Merkle claims and cold review pending |
| `corelink-byok-fuzz` | `crates/corelink-byok/fuzz/Cargo.toml` | `/tmp/corelink-ownership-w016-byok-fuzz`, `codex/w016-byok-fuzz` | Candidate `f95766819e3605867d7c4955d5d4f7a5eb838033`; four S checks pass; cold review and parent BYOK REL reconciliation pending; secrets/provider operations excluded |
| `corelink-audit-chain-fuzz` | `crates/corelink-audit-chain/fuzz/Cargo.toml` | `/tmp/corelink-ownership-w016-audit-chain-fuzz`, `codex/w016-audit-chain-fuzz` | Candidate `42373186f6a6a67786931bbb4fd71c394935c35b`; four S checks pass; cold review and global REL key / escalation reconciliation pending |
| `corelink-ac-fuzz` | `crates/corelink-ac/fuzz/Cargo.toml` | pending | Not dispatched; clean unused worktree/branch removed after fleet limit |
| `corelink-cli-fuzz` | `tools/cli/fuzz/Cargo.toml` | pending | Not dispatched |
