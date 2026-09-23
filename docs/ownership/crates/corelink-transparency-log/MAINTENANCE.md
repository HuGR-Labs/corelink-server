---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-transparency-log
manifest: crates/corelink-transparency-log/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: author_validated
evidence_set: transparency-log-static-source-20260920
---

# corelink-transparency-log — maintenance

Source-static modes only. This guide does not authorize Cargo, tests, network,
Rekor access, cryptographic verification, audit operation, retry, persistence,
or deploy. Canonical verified OKF material is only routed through [the OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md).

[Baseline](#m01) · [Triage](#m02) · [Wire mode](#m03) · [Seam mode](#m04) · [Parser/fake mode](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — READ_ONLY baseline

**Predicate:** package, manifest, recorded SHA, changed symbol, and source path
are identified. **Action:** inspect only the manifest and relevant local source
at `16d9f0303d849a1ab3df14688bd2c7cdbfee8140`. **Stop:** baseline or package
differs. **Recovery:** refresh the static map before making a claim.
**Evidence:** SHA and paths read; no execution result.

<a id="m02"></a>
## M02 — READ_ONLY triage

**Predicate:** one atomic relation and one falsifiable axiom are selected for a
source change. **Action:** map entry/digest to B01, wire to B02, submit/outcome
to B03, parser to B04, or fake/test source to B05, then read its R07 axiom.
**Stop:** a request needs external behavior or a complete consumer graph.
**Recovery:** retain the unknown and hand off the exact boundary. **Evidence:**
symbol, B section, axiom, and source paths.

<a id="m03"></a>
## M03 — READ_ONLY wire review

**Predicate:** a `SignedEntry`, digest, base64, serde field, or schema-version
change is proposed. **Action:** inspect `src/entry.rs:63-90`,
`src/lib.rs:84-98`, R03, AX-001 through AX-003, B01, and B02; record exact
input/output fields and a changed expression that would falsify the claim.
**Stop:** historical compatibility, remote schema,
or privacy beyond the local representation is required. **Recovery:** request
bounded consumer or service-owner evidence. **Evidence:** source expressions,
not a submitted request.

<a id="m04"></a>
## M04 — READ_ONLY seam review

**Predicate:** trait, error, logging, or `WitnessOutcome` changes. **Action:**
inspect `src/submit.rs:24-38`, `48-53`, and `87-118`, plus
`src/error.rs:17-49`, R04, AX-004, and B03; enumerate all source match arms and
distinguish the result shape from comment intent. **Stop:** a
claim concerns write-path isolation, retries, delivery, queueing, storage, or
service availability. **Recovery:** transfer to the consumer/runtime owner.
**Evidence:** signature, mapping, and declared unknowns.

<a id="m05"></a>
## M05 — READ_ONLY parser/fake review

**Predicate:** witness-record, proof-field, parser, fake, example, or
test-source changes. **Action:** inspect `src/witness.rs:65-162` and fake state
at `src/submit.rs:201-255`, plus R05, AX-005/AX-006, B04, and B05; name each
required field and each validation that is absent. **Stop:** proof validity,
checkpoint trust, timestamp truth,
test execution, concurrency, or persistence is required. **Recovery:** request
direct external or execution evidence. **Evidence:** source shape and unrun
test paths.

<a id="m06"></a>
## M06 — LOCAL_ISOLATED handoff
Procedure index: [PROC-001](#proc-001) · [PROC-002](#proc-002).

**Predicate:** only the assigned ownership skill and three ownership records
changed; S01–S07, R01–R08, B01–B06, and M01–M06 are navigable; static claims
have falsifiers and unknowns. **Action:** run the four documentary checks below,
then `git diff --check` against the fixed baseline and inspect changed paths.
**Stop:** checker, link, whitespace, scope, baseline, or evidence predicate
fails. **Recovery:** amend only the four owned documents or hand off the exact
finding. **Evidence:** JSON results, diff status, SHA, and paths.

```sh
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-transparency-log/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-transparency-log/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-transparency-log/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-transparency-log/MAINTENANCE.md
```

The external candidate checker validates document structure only. A pass is not
semantic completeness, independent approval, test execution, cryptographic
verification, audit evidence, or runtime proof. Handoff reports the selected
mode, predicate, stop/recovery, source paths, affected B/R entries, checker and
diff results, and the R08 unknowns.

<a id="proc-001"></a>
### PROC-001 — Review a transparency seam change
**Objective/trigger:** entry, submit, parser, or fake contract changes. **Preconditions/inputs:** pinned source, symbols, diff, R03–R05 APIs, R07 axioms, B01–B05. **Mode/environment/permissions:** `READ_ONLY`; local source only, no Cargo or network.

**Steps:** 1. Trace one changed boundary and error path. 2. Update API, axiom, REL, and unknown. 3. Keep parsing distinct from cryptographic verification.

**Expected predicate:** source facts have falsifiers; remote behavior stays unknown. **Stop:** transport, queue, proof validity, or caller wiring required. **Recovery:** remove unsupported claim and route to owner. **Evidence:** source paths and IDs.

**Review:** REVIEWED. **Execution:** REVIEWED_NOT_EXECUTED; no operation was run.
[Procedure index](#m06)

<a id="proc-002"></a>
### PROC-002 — Validate ownership documents
**Objective/trigger:** four artifacts are ready. **Preconditions/inputs:** root, checker, exact paths, baseline diff. **Mode/environment/permissions:** `LOCAL_ISOLATED`; local Python, docs only.

**Steps:** 1. Run the four commands in M06. 2. Run `git diff --check`. 3. Record outputs and changed paths.

**Expected predicate:** four passes and clean diff. **Stop:** checker or scope failure. **Recovery:** fix owned docs and rerun. **Evidence:** checker JSON and diff.

**Review:** REVIEWED. **Execution:** REVIEWED_NOT_EXECUTED; record actual output in handoff.
[Procedure index](#m06)

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Return](#m01)
