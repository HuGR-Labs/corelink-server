---
name: own-corelink-ac-fuzz
description: >-
  Use when changing the independent corelink-ac-fuzz Cargo package, its HKDF
  fuzz oracle, direct dependencies, or the workflow entries that select it.
  Do not use for production corelink-ac signing, TDK/KMS, worker handlers,
  R2, credentials, deployment, or customer data.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-ac-fuzz"
  manifest: "crates/corelink-ac/fuzz/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "ac-fuzz-source-20260921"
---

# Ownership — corelink-ac-fuzz

[Activation](#s01) · [Authority](#s02) · [Reading](#s03) · [Decisions](#s04) ·
[Workflow](#s05) · [Stop](#s06) · [Evidence](#s07).

<a id="s01"></a>
## S01 — Activation

| Activate this skill when | Do not activate it when |
|---|---|
| Editing `crates/corelink-ac/fuzz/Cargo.toml` or `fuzz_targets/hkdf_expand.rs` | Changing `corelink-ac` production APIs, TDK/KMS policy, worker handlers, R2, or deployment |
| Changing the `hkdf_expand` oracle, its input layout, assertions, resource bounds, or corpus policy | Treating the target name or ADR as proof of a production run or cryptographic runtime observation |
| Reviewing `fuzz-nightly.yml`, `scripts/fuzz-all.sh`, or ownership artifacts for this package | Running GitHub/provider jobs, using credentials, or changing customer/production data |

The Cargo manifest is the identity authority. A sibling `corelink-ac` crate is a
different package and owns the production signature contract; this package is a
standalone harness that reuses registry primitives rather than a path dependency.

<a id="s02"></a>
## S02 — Territory and authority

**Implementation owner:** this package owns one `cargo-fuzz` binary,
`hkdf_expand`, its input decoding, assertions, and standalone manifest.

**Contract owner:** the public path `corelink_ac::sig` (implemented under the
private `ac_core` module) owns production HKDF signing and
verification, including `HKDF_INFO_AC_SIG = b"ac-sig"`, TDK handling, key IDs,
and BLAKE3 MAC composition. This harness does not transfer that ownership.

**Composition/operator/review:** `scripts/fuzz-all.sh` and
`.github/workflows/fuzz-nightly.yml` select the target; a verified workflow
operator and independent reviewers are not identified by the pinned sources.
CODEOWNERS is a review request, not proof of authority or independence.

<a id="s03"></a>
## S03 — Reading and routing

| Question | Read |
|---|---|
| What does the target accept and assert? | [Reference R03–R05](../../../docs/ownership/crates/corelink-ac-fuzz/REFERENCE.md#r03) |
| What can a change affect? | [Blast B03–B05](../../../docs/ownership/crates/corelink-ac-fuzz/BLAST_RADIUS.md#b03) |
| How can it be validated safely? | [Maintenance M02–M05](../../../docs/ownership/crates/corelink-ac-fuzz/MAINTENANCE.md#m02) |
| Which canonical concepts apply? | [ADR-0021](../../../docs/knowledge/adr/adr-0021-hkdf-vs-ed25519-ac-signing.md) · [CAS/AC OKF cluster](../../../docs/knowledge/crates/cas-ac-core.md) |
| Which cross-cutting owner routes apply? | [`own-corelink-ac`](../own-corelink-ac/SKILL.md#s04) for the public signer contract · [`own-corelink-core`](../own-corelink-core/SKILL.md#s04) for shared typed/secret boundaries · [`okf-context`](../okf-context/SKILL.md) for verified OKF routing |

Read only records relevant to the target or changed boundary. Keep harness
implementation, production contract, composition, operation, and approval
authority separate. `corelink_ac::sig` is the consumer-facing path; do not
publish the private implementation path `corelink_ac::ac_core::sig` as a
compatibility route.

<a id="s04"></a>
## S04 — Decisions and invariants

| Condition | Action → evidence | Stop when |
|---|---|---|
| Input layout or assertion changes | Re-read the target; update `API`, `INV`, and affected `REL` records; preserve the exact source pin | The oracle no longer states its preconditions or failure boundary |
| Production HKDF, TDK, key rotation, or `ac-sig` changes | Route to `corelink-ac`; document this harness as a generalized property check, not end-to-end proof | Production equivalence, key provenance, or contract owner is unknown |
| Dependency/workspace/feature changes | Reconcile manifest, root exclusion, standalone workspace, and workflow selection; use a future authorized Cargo gate | Resolved graph, toolchain, or target support is required but unavailable |
| Workflow duration, runner, cache, or artifact changes | Review both workflow surfaces and resource/artifact isolation; require run evidence for execution claims | Operator, budget, or recovery authority is unverified |

Maintain these falsifiable boundaries: short or incomplete inputs return before
HKDF; successful output has the requested length; determinism is checked by two
expands; salt sensitivity is checked only for non-empty salts and `L >= 16`;
the harness's arbitrary `info`/length inputs are not the production fixed
`b"ac-sig"`/32-byte contract.

<a id="s05"></a>
## S05 — Workflow

1. Confirm manifest identity, source pin, integration baseline, and scope. 2. Read the target, production contract references, workflow entries, and the affected `REL` cards; classify the change as static documentation, harness, dependency, or workflow. 3. Select the matching maintenance procedure. Structural checks are read-only; a fuzz run requires an isolated worktree, explicit input/time limits, and authorized resources. 4. Record source, toolchain, command, result, corpus/artifact paths, and whether behavior was inspected,

built, executed, deployed, or runtime-observed. 5. Update only affected records and submit all four artifact hashes for four independent cold reviews. Never self-approve.

<a id="s06"></a>
## S06 — Stop conditions

Stop if the source pin drifts, the target starts claiming production reachability,
the generalized oracle is presented as the fixed AC contract, an input/resource
bound is absent, a finding may contain secret/customer data, or a required
operator/contract owner/reviewer cannot be verified. Escalate through the
authorized project process; do not invent a person or route.

Do not run providers, GitHub workflows, databases, R2, KMS, deployment, or
production code as part of package ownership. Do not infer execution from a
workflow declaration, workspace membership, successful build, or green cron.

<a id="s07"></a>
## S07 — Evidence and handoff

Return package/manifest, source pin, integration baseline, target/API/INV/REL/PROC
records touched, exact commands and results, resource/recovery state, unresolved
owners, and the four independent review states. Mark unrun procedures
`REVIEWED_NOT_EXECUTED` or `BLOCKED_FOR_AUTHORIZED_OPERATION`; never report a
fuzz run, production reachability, or approval from source intent.

[Back to activation](#s01)
