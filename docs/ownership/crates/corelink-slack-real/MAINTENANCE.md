---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-slack-real
manifest: crates/corelink-slack-real/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: slack-real-source-static-20260920
---

# corelink-slack-real — maintenance

Every procedure is STATIC_SOURCE, STATIC_GRAPH, or DOCUMENTARY only. None
authorizes Cargo, tests, network activity, webhook/env inspection, credential
handling, Slack interaction, deployment, or production audit operation.

[Baseline](#m01) · [Surface](#m02) · [Routing/payload](#m03) · [Retry/audit](#m04) · [Fakes/facades](#m05) · [Handoff](#m06).

**Procedure index:** [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005) · [PROC-006](#proc-006).

<a id="m01"></a>
## M01 — Confirm baseline and scope

<a id="proc-001"></a>
### PROC-001 — Pin source and scope
Objective/trigger: start or source drift; mode `READ_ONLY`; environment: pinned local checkout; permission: source reads only. Inputs/preconditions: SHA and package paths. Steps below. Expected predicate: identity/path scope match. Stop: mismatch. Recovery: re-anchor. Evidence: SHA and path list. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; required for acceptance `true`; result `NOT_EXECUTED`; review evidence: R01; execution evidence: none; limitation: no provider/runtime proof.

**Mode:** `READ_ONLY`. **Predicate:** the checkout is
`6be030999de1f0e0fe62d3a9abb04ec2a4fefde6`, the package is
`corelink-slack-real`, and paths are the four assigned artifacts or explicitly
authorized crate source. **Procedure:** record SHA, `Cargo.toml`, root module
inventory, and path list. **Stop:** identity, baseline, or scope differs.
**Recovery:** make no conclusion; obtain the reconciled target. **Evidence:**
revision and porcelain/path output.

[Procedure index](#m01)

<a id="m02"></a>
## M02 — Review public interface changes

<a id="proc-002"></a>
### PROC-002 — Trace exported API
Objective/trigger: root export/trait/error change; mode `READ_ONLY`; environment: pinned source; permission: no external access. Inputs/preconditions: changed symbol. Expected predicate: root route and defining module agree. Stop: unresolved caller compatibility. Recovery: mark unknown/request consumer evidence. Evidence: declarations/imports. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; required for acceptance `true`; result `NOT_EXECUTED`; review evidence: R02/R06; execution evidence: none; limitation: consumer set unresolved.

**Mode:** `READ_ONLY`. **Predicate:** root exports, `SharedSlackClient`,
`SlackAuditSink`, a public type, or error changes. **Procedure:** trace
`src/lib.rs` into its defining module; classify every changed symbol as local
implementation, injected interface, or fake. **Stop:** compatibility requires
an untraced caller. **Recovery:** retain the unknown and request owner evidence.
**Evidence:** declarations and static imports, never source comments alone.

[Procedure index](#m01)

<a id="m03"></a>
## M03 — Review routing and payload contracts

<a id="proc-003"></a>
### PROC-003 — Trace channel and payload
Objective/trigger: channel, template, escaping or field-bound change; mode `READ_ONLY`; environment: pinned source; permission: no env-value reads. Inputs/preconditions: changed route/payload symbol. Expected predicate: all channel/render branches match R03/R04. Stop: provider configuration/acceptance needed. Recovery: request separately authorized provider evidence. Evidence: channel/template/message/redactor source. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; required for acceptance `true`; result `NOT_EXECUTED`; review evidence: R03/R04; execution evidence: none; limitation: wire acceptance unknown.

**Mode:** `READ_ONLY`. **Predicate:** channel, env-var mapping, registry,
template, message builder, action, field, or escaping changes. **Procedure:**
trace `SlackChannel` → `WebhookRegistry` and `MessageTemplate` →
`SlackMessage::to_block_kit_json`; compare all six variants, the 150-character
header cap, ten-field cap, and escape branches. **Stop:** a claim needs an
environment value, provider payload rule, or accepted request. **Recovery:**
route it to separately authorized provider/configuration evidence.

[Procedure index](#m01)

<a id="m04"></a>
## M04 — Review retry, error, and audit ordering

<a id="proc-004"></a>
### PROC-004 — Trace retry terminal paths
Objective/trigger: retry/error/audit branch change; mode `READ_ONLY`; environment: pinned source; permission: no network or credentials. Inputs/preconditions: changed status/error branch. Expected predicate: decision bound and emit-before-return order match. Stop: timing, remote atomicity, or durability required. Recovery: state unknown and route operational question. Evidence: retry/http/audit source. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; required for acceptance `true`; result `NOT_EXECUTED`; review evidence: R05/R06; execution evidence: none; limitation: no request/retry observed.

**Mode:** `READ_ONLY`. **Predicate:** retry parameters/branches, terminal
outcome, error taxonomy, or audit event changes. **Procedure:** trace status
classification, retry bound/backoff cap, each terminal `emit_audit` call, and
the following return. **Stop:** requested assurance is timing, retry execution,
remote atomicity, provider rate limiting, or durable audit behavior.
**Recovery:** report it as unknown; do not infer it from the source loop.

[Procedure index](#m01)

<a id="m05"></a>
## M05 — Review adapter, fakes, and facade edges

<a id="proc-005"></a>
### PROC-005 — Trace adapter and fake boundaries
Objective/trigger: inquiry adapter, fake, or facade change; mode `READ_ONLY`; environment: pinned source; permission: source/manifest reads only. Inputs/preconditions: changed trait or manifest edge. Expected predicate: mapping/order/re-export match B05/B06. Stop: full consumer or live integration assertion. Recovery: mark graph/operation unknown. Evidence: adapter/fake/facade files and manifests. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; required for acceptance `true`; result `NOT_EXECUTED`; review evidence: B05/B06; execution evidence: none; limitation: no invocation proven.

**Mode:** `READ_ONLY`. **Predicate:** inquiry adapter, in-memory client/sink,
or facade re-export changes. **Procedure:** trace inquiry-trait mapping,
source-local audit-before-record ordering, and direct facade manifests/modules.
**Stop:** full reverse graph, selected consumer, or live integration is needed.
**Recovery:** mark the graph/operation unknown and obtain separate evidence.
**Evidence:** `adapter.rs`, `memory.rs`, `audit.rs`, and facade source/manifest.

[Procedure index](#m01)

<a id="m06"></a>
## M06 — Documentary handoff and escalation

<a id="proc-006"></a>
### PROC-006 — Run documentary gate
Objective/trigger: artifact change; mode `READ_ONLY`; environment: local checkout and checked-in Python checker; permission: read-only validation. Inputs/preconditions: four assigned files/profile S. Expected predicate: four checkers and whitespace diff pass. Stop: nonzero or scope error. Recovery: repair the reported artifact and rerun. Evidence: exact outputs and path list. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; required for acceptance `true`; result `NOT_EXECUTED`; review evidence: supplied contract; execution evidence: none; limitation: structural pass is not approval.

**Mode:** `READ_ONLY`. **Prerequisites:** assigned artifacts and checked-in S
checker are present. **Commands:**
```sh
python3 docs/ownership/tools/check_docs.py .claude/skills/own-corelink-slack-real/SKILL.md --kind skill --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-slack-real/REFERENCE.md --kind reference --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-slack-real/BLAST_RADIUS.md --kind blast_radius --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-slack-real/MAINTENANCE.md --kind maintenance --profile S --root .
git diff --check
```
Inspect the changed path list.

**Predicate:** S01–S07, R01–R08, B01–B07, and M01–M06 exist; each checker
passes and the diff has no whitespace or scope error. **Stop:** a
checker/diff/scope failure or need for Slack/provider evidence. **Recovery:**
correct only an assigned artifact or hand off the exact failed predicate.
Documentary results are not build, test, provider, runtime, or review approval.

[Procedure index](#m01)

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-slack-real/SKILL.md#s01).
