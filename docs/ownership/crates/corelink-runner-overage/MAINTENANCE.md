---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-runner-overage
manifest: crates/corelink-runner-overage/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: draft
evidence_set: w010-runner-overage-source-static-20260920
---

# corelink-runner-overage — maintenance

[M01](#m01) · [M02](#m02) · [M03](#m03) · [M04](#m04) · [M05](#m05) · [M06](#m06)

This manual is bounded to static source review and documentary validation. This
ownership pass performs no Cargo build/test, network activity, runner action,
meter action, provider action, credential access, deployment, or publishing.

<a id="m01"></a>
## M01 — Prepare a bounded review

**Mode:** READ_ONLY. **Prerequisites:** isolated checkout, expected baseline,
and explicit target path. **Predicate:** package identity is
`corelink-runner-overage`; review scope is its manifest and `src/lib.rs`.
**Stop:** baseline, identity, or requested surface differs. **Recovery:** do
not edit; obtain the corrected target. **Evidence:** SHA, inspected paths, and
`git status --short`.

<a id="m02"></a>
## M02 — Select the procedure

| Situation | Procedure | Mode | Stop condition |
|---|---|---|---|
| Tier, SKU, or allowance change | M03-A | SOURCE review | External entitlement/policy authority is required |
| Integer arithmetic or format change | M03-B | SOURCE review | External rounding/acceptance must be asserted |
| Public contract change | M03-C | SOURCE review | Consumer compatibility is unresolved |
| Ownership artifact change | M04 | DOCUMENTARY | Path is outside the four owned artifacts |
| External, operational, or execution request | M06 | ESCALATION | Do not operate from this guide |

<a id="m03"></a>
## M03 — Source procedures

### M03-A — Tier mapping review

**Mode:** SOURCE review. **Prerequisites:** requested variant/string/value is
identified. **Predicate:** each current tier maps to its listed static allowance
and SKU, and exact current SKU inputs round trip. **Inspect:** `RunnerTier` in
`src/lib.rs`. **Stop:** an external entitlement or metadata decision is needed.
**Recovery:** retain the current mapping and escalate. **Evidence:** affected
match arms and before/after static mapping table.

### M03-B — Arithmetic and rendering review

**Mode:** SOURCE review. **Prerequisites:** input unit, output unit, integer width, saturation, and floor point are stated. **Predicate:** overage uses saturating subtraction; decimal rendering has six floored places; charge uses two saturating multiplies before division; cent conversion divides by 1000. **Inspect:** the four free helpers and constants in `src/lib.rs`; inspect the four tier methods separately when the mapping changes. **Stop:** an external price, rounding, or outcome is required. **Recovery:** split that

request from the source change. **Evidence:** exact expression and boundary predicate.

### M03-C — Compatibility review

**Mode:** SOURCE review. **Prerequisites:** named public item and compatibility
decision. **Predicate:** public signatures, enum non-exhaustiveness, static
SKU values, widths, and output format are compared before alteration.
**Inspect:** `src/lib.rs` and [B05](BLAST_RADIUS.md#b05). **Stop:** a reverse
consumer inventory or selected-build result is necessary. **Recovery:** request
consumer-owner review. **Evidence:** affected public paths and explicit unknowns.

<a id="m04"></a>
## M04 — Documentary validation matrix

| Check | Expected predicate | Result in this ownership pass |
|---|---|---|
| Skill structural check (S) | target exists with S01–S07 | run after authoring |
| Reference structural check (S) | target exists with R01–R08 | run after authoring |
| Blast structural check (S) | target exists with B01–B06 | run after authoring |
| Maintenance structural check (S) | target exists with M01–M06 | run after authoring |
| `git diff --check` | no whitespace error | run after authoring |

These checks inspect documentation bytes only. They do not compile or execute
the crate and cannot establish a runtime, provider, billing, or runner fact.

<a id="m05"></a>
## M05 — Recovery and compatibility

| Surface | Safe recovery | Do not infer |
|---|---|---|
| Ownership documents | Amend only owned artifact and rerun M04 | Approval from a checker pass |
| Tier/SKU mapping | Restore the previous source mapping pending authority review | External catalog state |
| Arithmetic/format helper | Restore the previous expression pending compatibility review | A caller's rounding rule |
| Public API | Coordinate identified static consumers | Complete reverse graph or deployed compatibility |
| Manifest declaration | Preserve package identity and inherited fields | A selected build, publish, or release |

<a id="m06"></a>
## M06 — Escalation and completion record

Escalate runner invocation, usage provenance, price authority, meter/provider
interaction, ledger/persistence, credentials, external formatting acceptance,
network, Cargo execution, deployment, publishing, and production questions to
their responsible owners. Provide the baseline, exact source path, named
predicate, uncertainty, and requested direct evidence. Completion records the
baseline SHA, four changed paths, static relations, documentary check results,
and unknowns. Independent review owns approval.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-runner-overage/SKILL.md#s01)
