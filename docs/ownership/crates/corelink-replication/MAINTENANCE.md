---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-replication
manifest: crates/corelink-replication/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: corelink-replication-static-20260920
---

# corelink-replication — maintenance

These procedures are source/static-only. They do not authorize Cargo, a build,
test execution, network access, deployment, GitHub operations, external
replication, or runtime rollout claims. The verified canonical OKF remains an
external reference, without copying, redefining, or revalidating it here.

[Mode](#m01) · [Baseline](#m02) · [Façade](#m03) · [Controller](#m04) · [Boundary](#m05) · [Close](#m06).

<a id="m01"></a>
## M01 — Evidence mode

| Mode | Predicate | Action | Stop / recovery |
|---|---|---|---|
| Manifest declaration | Dependency or target changes | Compare `Cargo.toml` with R01 and B01 | Stop if a dependency implementation must change. |
| Source invariant | State, abort, audit, trigger, or budget code changes | Trace R04–R07 and B02–B05 | Stop if a runtime conclusion is needed. |
| Structural documentation | Ownership artifacts change | Use only M06’s checker and diff | Correct only the four authorized artifacts. |

<a id="m02"></a>
## M02 — Baseline and scope

| Predicate | Action | Expected evidence | Stop / recovery |
|---|---|---|---|
| Before interpreting source | Run `git diff --quiet 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6 HEAD -- crates/corelink-replication` | Exit `0` means the checked package source equals the stated baseline. | If nonzero, stop and re-establish source evidence. |
| Before authoring docs | Confirm only the skill and three ownership documents are changed | `git diff --name-only` | If code, Cargo, or unrelated paths appear, remove them from this work scope. |
| Before closeout | Keep `source_commit` equal to the stated baseline in all three document frontmatters | Static text comparison | If it differs, correct the authorized artifact. |

<a id="m03"></a>
## M03 — Façade or alias change

| Predicate | Action | Evidence | Stop / recovery |
|---|---|---|---|
| A re-export path changes | Preserve or deliberately version the public façade path | R01–R04, B01, `lib.rs` | Coordinate with the dependency owner before changing its API. |
| `region_resolver` alias changes | Trace the `corelink-worker` type boundary | R02, `lib.rs` | Stop if resolver behavior rather than alias shape changes. |
| New dependency is proposed | Record manifest relation and source boundary | R01, B01 | Do not infer callers or runtime use from declaration alone. |

<a id="m04"></a>
## M04 — Controller, abort, and audit change

| Predicate | Action | Evidence | Stop / recovery |
|---|---|---|---|
| `start` or active-session code changes | Preserve signed-artifact, budget, and one-session checks or explicitly revise R05 | `controller.rs`, R05, B02 | Stop if distributed uniqueness is required. |
| Advance, hold, probe-driven rollback, completion, or abort changes | Preserve state-machine order and audit-before-local-mutation relation; distinguish driver output in `probe_and_advance` from the caller-provided `auto_rollback` trigger | `controller.rs`, `state_machine.rs`, B03–B04 | Stop if audit delivery or deployed behavior is claimed. |
| Sink or error changes | Trace `RolloutAuditSink` and `RolloutError` propagation | `audit.rs`, `error.rs`, R07 | Resolve concrete transport outside this package. |

<a id="m05"></a>
## M05 — Trigger or budget change

| Predicate | Action | Evidence | Stop / recovery |
|---|---|---|---|
| Trigger or dwell relation changes | Trace all three trigger counters, threshold, and immediate successor rule | `auto_rollback.rs`, `types.rs`, `state_machine.rs` | Do not call constants measured policy execution. |
| Budget record or cap changes | Separate `start`/`probe_and_advance` reads of `consumed_ratio` from the trait/in-memory `record_rollback` relation; preserve the source-visible filter, 3000-bps denominator, and strict `> 1.0` predicate unless intentionally revised | `budget.rs`, `controller.rs`, [B05 controller read](BLAST_RADIUS.md#b05-controller-read), [B05 tracker record](BLAST_RADIUS.md#b05-tracker-record) | Stop if a concrete tracker query or stored record is needed. |
| Manual override is requested | Treat it as outside the static controller fixture | R08 | Escalate to the authority owning the concrete control path. |

<a id="m06"></a>
## M06 — Controlled closeout

| Check | Command / expected evidence | Stop / recovery |
|---|---|---|
| Skill structure | `python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-corelink-replication/SKILL.md` → `IMPLEMENTED_CHECKS_PASS` | Fix only the authorized skill. |
| Reference structure | `python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/corelink-replication/REFERENCE.md` → `IMPLEMENTED_CHECKS_PASS` | Fix only the authorized reference. |
| Blast structure | `python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/corelink-replication/BLAST_RADIUS.md` → `IMPLEMENTED_CHECKS_PASS` | Fix only the authorized blast document. |
| Maintenance structure | `python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/corelink-replication/MAINTENANCE.md` → `IMPLEMENTED_CHECKS_PASS` | Fix only the authorized maintenance document. |
| Baseline diff | `git diff --check 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6 HEAD` → exit `0` | Correct whitespace in authorized documentation only. |

The four checker results are structural checks, not semantic approval, runtime
proof, cold review, or a statement that external replication or rollout ran.
