# Ownership wave 001 — leaf foundations

This is a frozen execution contract for the first rolling fanout. It does not
freeze the campaign standard, approve an artifact, or authorize issue
publication.

## Baseline and scope

Baseline: `8a4215a40`. Each work package owns exactly one package's four
artifact paths and its own skill path. No work package may edit an index,
registry, shared template, `STANDARD.md`, `CAMPAIGN_STATUS.md`, Cargo manifest,
or another package's ownership directory.

| WP | Package | Manifest | Profile | Owned paths |
|---|---|---|---|---|
| W001-AC | `corelink-ac` | `crates/corelink-ac/Cargo.toml` | S | `own-corelink-ac`, `crates/corelink-ac/` |
| W001-ADAPTER | `corelink-adapter-host` | `crates/corelink-adapter-host/Cargo.toml` | H | `own-corelink-adapter-host`, `crates/corelink-adapter-host/` |
| W001-ANALYTICS | `corelink-analytics` | `crates/corelink-analytics/Cargo.toml` | S | `own-corelink-analytics`, `crates/corelink-analytics/` |
| W001-AUDIT | `corelink-audit` | `crates/corelink-audit/Cargo.toml` | S | `own-corelink-audit`, `crates/corelink-audit/` |
| W001-CHAIN | `corelink-audit-chain` | `crates/corelink-audit-chain/Cargo.toml` | H | `own-corelink-audit-chain`, `crates/corelink-audit-chain/` |
| W001-BAZEL | `corelink-bazel-bridge` | `crates/corelink-bazel-bridge/Cargo.toml` | S | `own-corelink-bazel-bridge`, `crates/corelink-bazel-bridge/` |

## Frozen artifact contract

Each WP must create only these four artifacts:

1. `.claude/skills/own-<slug>/SKILL.md`, with S01–S07 and decision form
   `condition → action → evidence → stop`.
2. `REFERENCE.md`, with R01–R08; public contracts and falsifiable invariants
   tied to source evidence.
3. `BLAST_RADIUS.md`, with B01–B06; atomic relations distinguish dependency,
   flow and impact directions, and unknowns remain explicit.
4. `MAINTENANCE.md`, with M01–M06; procedures state mode, prerequisites,
   expected predicate, stop/recovery and evidence state.

Use the existing `corelink-hash` pilot only as a formatting pattern, not as
evidence for the target package. Evidence classes remain SOURCE, RESOLVED,
EXECUTED_LOCAL, DEPLOYMENT and OBSERVED_RUNTIME. This wave permits SOURCE and
safe resolved-graph evidence only. Do not compile Cargo, execute production
operations, claim runtime reachability, or certify a cold review.

## Acceptance items

Every item below is red at this baseline because its target artifact does not
exist. A WP is ready for lead verification only if all its four items become
green and the documented checker passes at its declared profile.

| Item pattern | Mechanical predicate |
|---|---|
| `W001-<WP>-SKILL` | target `SKILL.md` exists and skill check passes |
| `W001-<WP>-REFERENCE` | target `REFERENCE.md` exists and reference check passes |
| `W001-<WP>-BLAST` | target `BLAST_RADIUS.md` exists and blast check passes |
| `W001-<WP>-MAINTENANCE` | target `MAINTENANCE.md` exists and maintenance check passes |
| `W001-<WP>-SCOPE` | diff contains only that WP's four owned paths |

Semantic completeness, factual source evidence, profile eligibility,
cross-document navigation and cold review are judged gates for the lead; a
checker pass never turns them green automatically.

## Conflict map and integration

All six WPs are conflict-free: their writable paths are pairwise disjoint. The
only shared future files are registry/index/status/standard; the lead owns them
after all package artifacts are independently reviewed. Merge order is any
verified WP, one at a time, followed by a dedicated cold reviewer for each of
the four changed artifacts. No agent self-approves.

## Worker Definition of Done

- Verify the pinned baseline before editing.
- Use only the WP's owned paths and commit one coherent package artifact set.
- Cite concrete source/manifest paths for material claims; label unknowns.
- Keep each document within the S limits; capacity overflow is `BLOCKED`, not
  an excuse to omit relations.
- Run only the four documentary checker invocations and `git diff --check`.
- Return commit SHA, changed paths, checker results, and explicit unknowns.

## Lead gates after return

L0: parent/baseline and owned-path sanity. L2: contract/invariant review.
L5/L6: document structure and checker reproduction. Separate cold reviewers
then issue four independent verdicts; publication remains blocked by campaign
freeze, census certification and duplicate reconciliation.
