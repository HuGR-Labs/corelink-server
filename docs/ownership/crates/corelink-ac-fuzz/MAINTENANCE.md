---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-ac-fuzz
manifest: crates/corelink-ac/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
integration_baseline: 10cce309b9fc642d357ea9cd7a30a2fcde0a2758
profile: S
state: draft
evidence_set: ac-fuzz-source-20260921
---

# corelink-ac-fuzz — maintenance

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) ·
[Validation](#m04) · [Recovery](#m05) · [Escalation](#m06).

<a id="m01"></a>
## M01 — Safe preparation

Use the exact source pin and a disposable worktree. This authoring task is
documentation-only; no Cargo or fuzz execution was performed.

| Item | Verified value / limit |
|---|---|
| Source / integration | `1177dad2ca2a9f21c29b5a118aa7944b77147798` / `10cce309b9fc642d357ea9cd7a30a2fcde0a2758` |
| Package / target | `crates/corelink-ac/fuzz/Cargo.toml` / `hkdf_expand` |
| Toolchain | Workflow declares nightly + cargo-fuzz 0.13.1 on self-hosted Mac; local support/version unknown |
| Input/resource bound | `L <= 255`; future run must add explicit `-max_total_time` and `-max_len`; corpus/artifact paths must be task-owned |
| External state | No credentials, provider, production, database, R2, or GitHub operation |

<a id="m02"></a>
## M02 — Procedure selection

| Situation | Procedure | Mode | Extra authority |
|---|---|---|---|
| Edit/check ownership artifacts | [PROC-001](#proc-001) | `READ_ONLY` | None beyond documentation scope |
| Review a target/dependency change locally | [PROC-002](#proc-002) | `LOCAL_ISOLATED` | Installed toolchain and bounded resource budget |
| Run the configured fuzz target | [PROC-003](#proc-003) | `AUTHORIZED_OPERATION` | Verified package/workflow operator; no production access |
| Triage a finding | [PROC-004](#proc-004) | `LOCAL_ISOLATED` | Data-review route before sharing bytes |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Run structural ownership checks

**Objective/trigger:** after any artifact byte change, prove the four S-profile
documents satisfy the v1.3 structure/size gate. **Mode:** `READ_ONLY`, local
Python. **Preconditions:** exact four paths and the pinned external checker at
`/tmp/corelink-ownership-v13-source/corelink-ownership-v1.3` exist; no install.
**Environment:** repository root, Python 3 stdlib, source pin `1177`; no
network or Cargo. **Permissions:** read files and write task-owned report only.
**Inputs:** four artifact paths, checker path, `--kind`, `--profile S`, and
`--root .`.

1. Run `check_docs.py` for `SKILL.md` as `--kind skill --profile S --root .`.
2. Run it for `REFERENCE.md`, `BLAST_RADIUS.md`, and `MAINTENANCE.md` with their
   matching kinds and the same profile/root.
3. Retain JSON metrics, verify `IMPLEMENTED_CHECKS_PASS`, inspect links/paths,
   and compute final artifact hashes for review.

**Expected predicate:** four structural passes. **Stop:** missing checker,
profile overflow, broken link, or source-pin mismatch. **Recovery:** fix only
affected docs and rerun all four. **Current state:** executed by author after
writing; structural pass is not semantic approval or cold review. **Evidence:**
commands, outputs, exact paths, and hashes.

[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Perform static oracle/contract review

**Objective/trigger:** target, production HKDF, workflow, or dependency change.
**Mode:** `LOCAL_ISOLATED` read-only. **Environment:** disposable worktree at
the source pin with no credentials or provider access. **Permissions:** read
source/manifests and task-owned notes; no workflow dispatch. **Inputs:** exact
source pin, manifest, target, public `corelink_ac::sig` signer, ADR-0021, sealed
S-04 audit boundary, and affected REL cards.

1. Verify the input guards, byte layout, `L` cap, and each assertion against the
   target source.
2. Compare fixed production salt/info/output/key-ID rules with the generalized
   harness; record any non-equivalence explicitly.
3. Reconcile root exclusion, nested workspace, script, and matrix selectors.
4. Update API/INV/REL records and stop on an unowned or runtime-only claim.

**Expected predicate:** every changed material claim has source evidence and a
   stated execution status. **Recovery:** retain the prior artifact and source
pin; do not invent a run result. **Current state:** reviewed-not-executed for
runtime/build behavior. **Evidence:** source paths/blobs and diff.

[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Run a bounded isolated fuzz target

**Objective/trigger:** authorized validation after target/dependency changes.
**Mode:** `AUTHORIZED_OPERATION` in a disposable worktree with task-owned
corpus/artifact/build paths. **Environment:** `RUSTUP_TOOLCHAIN=nightly-x86_64-apple-darwin`,
isolated `CARGO_HOME`/`CARGO_TARGET_DIR`, and working directory
`crates/corelink-ac`. **Permissions:** verified fuzz operator only; no provider,
GitHub, production, or customer-data access. **Inputs:** exact source pin,
installed cargo-fuzz 0.13.1, approved CPU/time/memory budget, target
`hkdf_expand`, and explicit corpus/artifact paths. **Preconditions:** verified
operator and all bounds approved.

1. From `crates/corelink-ac`, run `cargo fuzz run hkdf_expand -- -max_total_time=60 -max_len=4096`.
2. Record toolchain, cargo-fuzz version, flags, source, elapsed time, exit status,
   corpus, artifacts, and host/resource limits.
3. Stop on panic, assertion, timeout, OOM, unknown input provenance, or toolchain
   mutation; quarantine the original input and use PROC-004.

**Expected predicate:** bounded completion without an untriaged finding; this
does not prove production sign/verify or CI reachability. **Recovery:** preserve
run IDs and task-owned artifacts; do not clear shared corpus/cache. **Current
state:** `BLOCKED_FOR_AUTHORIZED_OPERATION` / not executed in this task.

[Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Triage and preserve a finding

**Objective/trigger:** crash, assertion, timeout, or resource finding.
**Mode:** `LOCAL_ISOLATED`. **Environment:** disposable worktree with task-owned
copy of the input and no secrets/provider credentials. **Permissions:** local
read/hash/minimize only; independent data reviewer must authorize sharing.
**Inputs:** source/target pin, original input hash/path, run metadata, and
classification route. **Preconditions:** source/target pin and artifact origin
known; independent data reviewer and sharing route verified.

1. Hash and preserve the original input and metadata without editing it.
2. Map it to the target branch and classify whether it concerns generalized
   HKDF behavior, production-contract drift, or runner/resource failure.
3. Do not upload/share/delete bytes when provenance or sensitivity is unknown.
4. Reproduce only through PROC-003 with a separate minimized copy and explicit
   budget; record non-reproducibility rather than guessing.

**Expected predicate:** provenance, classification, and reviewer decision are
recorded. **Stop/recovery:** quarantine on secret/customer data or missing
authority; preserve original evidence. **Current state:** not executed; no
finding exists. **Evidence:** hashes, source pin, target, classification, review.

[Procedure index](#m02)


<a id="m04"></a>
## M04 — Tests and validation matrix

| Change | Selection | Procedure | Required predicate | Current evidence |
|---|---|---|---|---|
| Docs | Four canonical files | PROC-001; `python3 /tmp/corelink-ownership-v13-source/corelink-ownership-v1.3/tools/check_docs.py <path> --kind <kind> --profile S --root .` ×4 | Four structural passes; size/links valid | Author structural checks; cold review pending |
| Input/assertion | `hkdf_expand`, no Cargo features | PROC-002 + `RUSTUP_TOOLCHAIN=nightly cargo fuzz run hkdf_expand -- -max_total_time=60 -max_len=4096` | Guards, length, determinism, conditional salt property remain bounded | Source reviewed; no run |
| Fuzz lint policy | `hkdf_expand`; default features (none) | `cargo +nightly clippy --manifest-path crates/corelink-ac/fuzz/Cargo.toml --bin hkdf_expand --no-default-features -- -D warnings` | Resolve declared `panic`/`unwrap_used`/`indexing_slicing` denies versus target `expect`/indexing; no warning | Not executed; conflict remains UNKNOWN |
| HKDF/SHA dependency | Manifest features/defaults as resolved by Cargo | `cargo +nightly metadata --manifest-path crates/corelink-ac/fuzz/Cargo.toml --no-deps --format-version 1` then authorized build/run | Resolved selection and oracle behavior match intent | Cargo graph unknown |
| Production AC contract | `corelink-ac` signer + worker/CAS consumers; no fuzz feature | `cargo test -p corelink-ac --test ac_core_canonical_vectors_sig --test ac_core_key_rotation_sig --test ac_core_prop_sig --test ac_core_timing_sig` with `RUST_BACKTRACE=1`; worker/CAS suites selected by their owners | Fixed `ac-sig`/TDK/key-ID compatibility preserved | Not executed; harness is not sufficient |
| Workflow/script | `fuzz-nightly.yml` + `fuzz-all.sh`; nightly env from workflow | `bash scripts/fuzz-all.sh` only under authorized budget, or static PROC-002 review | Target, duration, runner, corpus/artifact policy correct | YAML/script inspected; no run |

Negative cases to preserve: `<4` bytes, incomplete declared slices, empty salt,
`L < 16`, and non-production arbitrary `info`. A structural pass never certifies
these semantics or a production run.

<a id="m05"></a>
## M05 — Recovery and compatibility

| Surface | Safe action | Recovery evidence |
|---|---|---|
| Harness source/assertions | Revert only task-owned change; keep original source pin and findings | Scoped diff + cold re-review of changed bytes |
| Corpus/crash input | Preserve original; quarantine unknown/sensitive bytes; never blind-delete | Hash/inventory before and after |
| Build/target output | Use disposable target directory; remove only outputs created by task after evidence capture | Path inventory and command record |
| Workflow/corpus cache | Coordinate with verified operator; source revert does not cancel jobs or restore data | Run ID, artifact readback, operator record |
| Production AC/worker/CAS state | Not touched by this package | Stop and route to canonical owner; fuzz result cannot authorize compatibility claim |

`git revert` cannot recover deleted corpus/artifact data, cancel a running job,
or repair wire/signature compatibility. Roll forward only after canonical
contract owners and independent reviewers agree.

<a id="m06"></a>
## M06 — Escalation and evidence

| Condition | Minimum evidence | Stop / route |
|---|---|---|
| Production HKDF/TDK/key rotation mismatch | Source pin, API/REL, exact symbol and expected contract | Stop; route to `corelink-ac` owner through authorized process |
| Workflow/resource/toolchain issue | Workflow path, command, host/toolchain, budget, run/artifact ID | Stop; verified workflow operator only |
| Possible sensitive fuzz input | Hash, origin, target, no raw bytes in issue/doc | Quarantine; verified data reviewer before sharing |
| Artifact ready for acceptance | Four exact blobs/hashes, source/integration pins, structural results | Four independent cold verdicts; no self-approval |

After authorized work, record what was inspected, built, executed, deployed, or
observed; reconcile all affected RELs; clean only task-created temporary output;
and submit final hashes. Current status: documents are draft, no fuzz execution,
production operation, or approval is claimed.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01)
