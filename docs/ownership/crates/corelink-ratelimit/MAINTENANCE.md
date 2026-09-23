---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-ratelimit
manifest: crates/corelink-ratelimit/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: author_validated
evidence_set: ratelimit-static-source-20260920
---

# corelink-ratelimit — maintenance

Static maintenance procedures for ownership documentation. Every procedure is
SOURCE-only: it neither executes Cargo nor proves DO/D1, provider, network,
deployment, or production rate-limit behavior.

[Baseline](#m01) · [Surface](#m02) · [Algorithm](#m03) · [Fake](#m04) · [Policy route](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Baseline procedure

[PROC-001](#proc-001).

<a id="proc-001"></a>
### PROC-001 — Establish source evidence

Prerequisite: supplied commit and package path are available. Expected
predicate: manifest and source paths identify `corelink-ratelimit` at that
commit. Execution/evidence state: static read of `Cargo.toml`, `src/lib.rs`,
and affected local modules. Recovery: if a path or revision differs, stop,
record the discrepancy, and obtain the intended baseline; do not substitute it.

1. Confirm package and manifest path.
2. Record the supplied source commit.
3. Inventory changed local source paths.

Stop when the baseline differs or work requires executed evidence.
[Baseline](#m01)


<a id="m02"></a>
## M02 — Public surface procedure

[PROC-002](#proc-002).

<a id="proc-002"></a>
### PROC-002 — Review exported contracts

Prerequisite: changed public names and their root declarations are known.
Expected predicate: every reviewed root export resolves to the stated defining
module in this source tree. Execution/evidence state: static read of
`src/lib.rs` and defining modules. Recovery: if a name cannot be traced,
record it as unresolved and obtain the missing source before compatibility work.

1. Trace each changed root re-export.
2. Compare the defining public type, trait, function, or constant.
3. Note source compatibility risk and unknown callers.

Stop before asserting a wire contract or consumer adoption. [Surface](#m02)


<a id="m03"></a>
## M03 — Bucket procedure

[PROC-003](#proc-003).

<a id="proc-003"></a>
### PROC-003 — Review local algorithm source

Prerequisite: the affected state, key, config, or tier declaration is named.
Expected predicate: its source form either preserves or falsifies the relevant
[R05](REFERENCE.md#r05) predicate. Execution/evidence state: static read of
`src/{bucket,config,key,tier}.rs`. Recovery: on mismatch, preserve the finding
and route correction to the source owner; no test or policy conclusion follows.

1. Trace changed state, decision, key, config, and tier declarations.
2. Compare affected source predicates with R05.
3. Record any mismatch as a source finding.

Stop before claiming tested correctness, policy approval, or a live limit.
[Algorithm](#m03)


<a id="m04"></a>
## M04 — Fake and ports procedure

[PROC-004](#proc-004).

<a id="proc-004"></a>
### PROC-004 — Keep local fake distinct

Prerequisite: a port or its local implementation is in scope. Expected
predicate: the trait, fake, fields, and error relation can each be located in
source without assuming an external backend. Execution/evidence state: static
read of `src/{limiter,audit,metrics,error}.rs`. Recovery: if source cannot
separate port from provider behavior, mark the boundary unknown and request
provider-specific evidence.

1. Identify traits separately from in-memory/no-op/failing types.
2. Inspect declared map, mutex, and injected collaborator fields.
3. Report only source-visible ordering or error relations.

Stop before calling any type a provider, durable store, alert, or delivery.
[Fake](#m04)


<a id="m05"></a>
## M05 — Canonical-routing procedure

[PROC-005](#proc-005).

<a id="proc-005"></a>
### PROC-005 — Preserve policy routing

Prerequisite: the question maps to a canonical tier, retry, audit, metric, or
migration concept. Expected predicate: a local declaration/reference can be
identified while canonical meaning stays in verified OKF routing. Execution/
evidence state: static read of `docs/ownership/WAVE_008_PLAN.md` and local
source references. Recovery: if routing is absent or ambiguous, record that
gap and request canonical-owner direction; do not recreate policy here.

1. Route canonical tier/retry/audit/metric questions to verified OKF material.
2. Record the local declaration or reference only.
3. Keep policy text and validation outside these artifacts.

Stop if asked to copy, redefine, or revalidate canonical policy. [Policy route](#m05)


<a id="m06"></a>
## M06 — Completion procedure

[PROC-006](#proc-006).

<a id="proc-006"></a>
### PROC-006 — Validate documentary output

Prerequisite: all four artifacts exist at their required paths and the checked-in
checker exists at `docs/ownership/tools/check_docs.py`.
Expected predicate: that checker returns success for exact kinds `skill`,
`reference`, `blast_radius`, and `maintenance`, each with profile S; diff
whitespace check is clean. Execution/evidence state: structural-only checker
output plus `git diff --check`; neither is semantic, cold-review, or runtime
approval. Recovery: if the checker is unavailable or fails, record its output,
restore availability or correct the named document, then rerun all four kinds.

Required commands use repository root `<repo>` and positional artifact paths:

```sh
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root <repo> .claude/skills/own-corelink-ratelimit/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root <repo> docs/ownership/crates/corelink-ratelimit/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root <repo> docs/ownership/crates/corelink-ratelimit/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root <repo> docs/ownership/crates/corelink-ratelimit/MAINTENANCE.md
git diff --check 6ed297f5b..HEAD
```

1. Check every artifact with the S-profile checker.
2. Run requested `git diff --check` against baseline.
3. Report paths, checks, and explicit unknowns.

Success is source-accurate procedures; completeness is M01–M06; quality is
clear mode/evidence/stops. Definition of Done excludes Cargo, tests, network,
DO/D1, deployment, and production claims. [Handoff](#m06)

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Start](#m01)
