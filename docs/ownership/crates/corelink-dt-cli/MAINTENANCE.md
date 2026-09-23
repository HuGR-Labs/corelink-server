---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-dt-cli
manifest: tools/dt-cli/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: draft
evidence_set: dt-cli-static-source-20260921
---

# corelink-dt-cli — maintenance

Static ownership procedures only. No procedure authorizes command execution,
Cargo execution, network, provider/API traffic, credential use, alerting,
deployment, or runtime probing. The verified [OKF SRE operations hub](../../../knowledge/ops/sre-operations-hub.md)
is canonical routing context only; do not copy or redefine it here.

[Baseline](#m01) · [Dispatch](#m02) · [Injection](#m03) · [Fallback](#m04) · [Boundary](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Confirm baseline

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_MANIFEST | SHA and package path match this record | Read the intended revision, `tools/dt-cli/Cargo.toml`, and `tools/dt-cli/src/main.rs` | Stop on a different snapshot; obtain the intended baseline without resetting unrelated work |

<a id="m02"></a>
## M02 — Maintain command dispatch

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_SOURCE | Arguments, selector literals, usage, logs, or exits change | Compare minimum-argument handling, both selectors, unknown-command branch, and exit `1` paths against R03/B01 | Stop before inferring a caller, shell invocation, authorization, or execution |

<a id="m03"></a>
## M03 — Maintain mock-input predicates

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_RELATION | Project, severity, environment, synthetic CVE, handler, or result handling changes | Trace flag-before-environment precedence; the `DtProjectUuid::new` `Err` branch to exit `1` without asserting UUID validity; exact `"true"` gate; severity mapping; fallback secret bytes; and, separately, returned-result exits against R04/B02–B05 | Stop before treating a dependency call as a request, injection, alert, valid secret, or observed SLA |

<a id="m04"></a>
## M04 — Maintain fallback-stub boundary

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_SOURCE | `ossindex-fallback` body, comments, logs, or exit changes | Compare argument use, client-call presence, and immediate exit against R05/B06 | Stop if an actual lookup, endpoint contract, request, response, or transfer must be established; obtain its owner evidence |

<a id="m05"></a>
## M05 — Preserve evidence boundary

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| STATIC_RELATION | A relation is reported | Name endpoints, source-order falsifier, evidence mode, and explicit unknown; retain the five axioms in [R02](REFERENCE.md#r02) | Stop if a static relation is relabeled as execution or if the canonical OKF route is copied or redefined |

<a id="m06"></a>
## M06 — Documentary handoff

| Mode | Predicate | Procedure | Stop / recovery |
|---|---|---|---|
| DOCUMENTARY_CHECK | Only the four ownership artifacts changed | Run the checked-in checker four times and a scoped whitespace diff check | A pass is structural only; stop before semantic approval or operational claims |

```bash
python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-dt-cli/SKILL.md
python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-dt-cli/REFERENCE.md
python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-dt-cli/BLAST_RADIUS.md
python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-dt-cli/MAINTENANCE.md
git diff --check 398e586ccef712477f2a4ce51e026443b67e5747..HEAD
```

The checked-in checker results and `git diff --check` are structural only, not
semantic completeness, review, execution, or operational proof. [Reference](REFERENCE.md#r01)
· [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
