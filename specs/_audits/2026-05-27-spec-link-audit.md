---
id: "AUDIT-2026-05-27-SPEC-LINK-AUDIT"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "spec-hygiene", "link-check", "post-w36"]
---

# Spec link audit — post Wave 36

Scope: enumerate broken relative-path markdown links and stale crate
path anchors across `specs/**.md` after the Wave 33 reorg + Wave 35
Phase 2 absorption sweeps (which collapsed 30+ leaf crates into the
19 umbrella crates) + Wave 36 closure.

Method: AST-light scan — `find specs/ -name '*.md'` piped through a
Python regex matcher for `[label](path)` markdown links (skipping
`http://`, `https://`, `mailto:`, anchor-only `#…`); each relative
path resolved with `os.path.normpath(join(dirname(f), path))` and
checked with `os.path.exists`. Crate-anchor sweep via `grep -rln` for
the 7 absorbed-crate path prefixes called out by the orchestrator
(`corelink-canary`, `corelink-chunker`, `corelink-webauthn`,
`corelink-quota`, `corelink-abuse`, `corelink-oncall`,
`corelink-ac-core`).

## §1. Broken markdown links

49 unique broken `[label](path)` resolutions surfaced. Three
classes:

### §1.1 Real broken file references — DRAFT specs (1 site, 1 link, FIXED inline)

| Source file | doc_status | Link target (as written) | Resolved-to | Disposition |
|---|---|---|---|---|
| `specs/03_architecture/slo_catalog.md` L126 | DRAFT | `../../05_quality/runbooks/RB-SLO-AVAIL-CP.md` | `05_quality/runbooks/RB-SLO-AVAIL-CP.md` (one `../` too deep — file is at `specs/05_quality/runbooks/RB-SLO-AVAIL-CP.md`) | **FIXED** to `../05_quality/runbooks/RB-SLO-AVAIL-CP.md` (§4.1) |

### §1.2 Real broken crate-path references — DRAFT specs (FIXED inline as §1.4 below)

These were flagged by the broken-link scan because the link text
embeds a `crates/<absorbed-crate>/…` filesystem reference that no
longer exists post Wave 35 P2. The DRAFT-status hits are mapped to
the umbrella path equivalent in §1.4. Note: these were captured as
markdown link breakages only when the spec actually wrote them as
`[label](crates/...)`; the FROZEN ADR/spec-contract textual mentions
(no markdown link, just inline backticks) are catalogued in §2.

| Source file | doc_status | Original link target | Suggested umbrella path | Disposition |
|---|---|---|---|---|
| `specs/03_architecture/tenant-offboarding-spec.md` L? | DRAFT | `../../crates/corelink-tenant-offboarding/src/{audit,error,orchestrator,state,store}.rs` (5 links) | `../../crates/corelink-ops/src/tenant_offboarding/{audit,error,orchestrator,state,store}.rs` | **ESCALATED** (§5) — file is DRAFT but the link block looks like a vendored design index; orchestrator should decide whether to repath in this audit pass or fold into a larger spec rewrite |
| `specs/03_architecture/tenant-offboarding-spec.md` | DRAFT | `../../crates/corelink-tenant-offboarding/tests/prop_tenant_offboarding.rs` | `../../crates/corelink-ops/tests/` (test file may have moved with rename — needs human confirm) | **ESCALATED** (§5) |
| `specs/03_architecture/tenant-offboarding-spec.md` | DRAFT | `../../crates/corelink-tenant-offboarding/` (root) | `../../crates/corelink-ops/src/tenant_offboarding/` | **ESCALATED** (§5) |
| `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` | **FROZEN** | `../../crates/corelink-drata-sync/src/stream.rs` | `../../crates/corelink-ops/src/drata/stream.rs` | **ESCALATED** (§5) — FROZEN per `__no_modify__` policy |
| `specs/_runbooks/RB-DRATA-SYNC-FAILURE.md` (2 links) | DRAFT | `../../crates/corelink-drata-sync/{src/record.rs, /}` | `../../crates/corelink-ops/src/drata/record.rs`, `../../crates/corelink-ops/src/drata/` | **ESCALATED** (§5) — full repath would need full runbook audit (multiple anchors, line-number refs not just paths) |
| `specs/_runbooks/RB-TENANT-OFFBOARDING.md` | DRAFT | `../../crates/corelink-tenant-offboarding/` | `../../crates/corelink-ops/src/tenant_offboarding/` | **ESCALATED** (§5) — same rationale as RB-DRATA-SYNC-FAILURE |

### §1.3 False positives — placeholder-template markdown (4 hits, NO ACTION)

| Source file | Link as written | Reason it's not a real breakage |
|---|---|---|
| `specs/_templates/sprint_contract.md` L266 | `[→](work_items/WI-SXX-001.md)` | Template literal — `SXX`/`001` are placeholders rendered per-sprint, not a real path |
| `specs/_templates/sprint_contract.md` L267 | `[→](work_items/WI-SXX-002.md)` | Same |
| `specs/_templates/work_item.md` L933 | `[→](ST-001.md)` | Template placeholder for first sub-task — concrete sprint instances replace it |
| `specs/_templates/work_item.md` L934 | `[→](ST-002.md)` | Same |

### §1.4 False positives — markdown-link regex catching non-link text (3 hits, NO ACTION)

| Source file | Captured text | Reason it's not a real breakage |
|---|---|---|
| `specs/_audits/sealed/2026-05-16-debt-015-build-closure.md` L54 | `` [`permissions.mdx`](./permissions) `` | Inside an inline code-span prose passage describing a path; the captured "link" is a quoted code fragment, not a renderable link target |
| `specs/_audits/sealed/2026-05-16-wave24-adversarial-review.md` L10/172 | `` [0,10](10 - 1.5·P0 - 0.5·P1 - 0.15·P2 - 0.05·P3) `` | Mathematical interval notation `[0,10]` followed by a parenthesised score formula; markdown parser will NOT render this as a link (the bracket-paren pair contains a space-bearing math expression, not a URL) |
| `specs/_audits/sealed/2026-05-16-wave25-adversarial-review.md` L? | same `[0,10](10 - 1.5·…)` | same — score formula notation duplicated across review docs |

### §1.5 Historical-FROZEN sprint-contract → `sprint.md` chain (40 hits, NO ACTION)

5 FROZEN sprints (`S07`/`S08`/`S09`/`S10`/`S11`) carry 40 broken
`../sprint.md` links because the planned per-sprint `sprint.md` was
never authored — the canonical doc became `_spec_contract.md`. This
was acknowledged at SEAL time:

> `specs/04_sprints/_sealed/S07/_spec_contract.md` v1.6.0 changelog: "Parent:
> `[S-07](../sprint.md)` → `../_spec_contract.md` across all 5 WIs
> (sprint.md not authored)"

The remediation flipped the WI-internal references; the residual 40
back-pointers in WIs across S08–S11 are leftover from the same
intent but never got the same sweep. All 5 sprint contracts +
their work-items are `doc_status: "FROZEN" | audit_status:
"AUDITED"` — **immutable under audit policy §4**. Flagged here for
the historical record only.

## §2. Stale path references to absorbed crates (inline backticks, not markdown links)

36 files contain inline-backtick references to absorbed-crate
paths. Mapping table:

| Absorbed crate | Wave 35 P2 audit | Umbrella destination |
|---|---|---|
| `corelink-canary` | `2026-05-26-w35-p2-telemetry-absorption.md` | `corelink-telemetry::canary` → `crates/corelink-telemetry/src/canary/` |
| `corelink-chunker` | `2026-05-26-w35-p2-cas-absorption.md` | `corelink-cas::chunker` → `crates/corelink-cas/src/chunker/` |
| `corelink-webauthn` | — (NOT W35-P2; older absorption into `corelink-auth`) | `corelink-auth::webauthn` → `crates/corelink-auth/src/webauthn/` |
| `corelink-quota`, `corelink-quota-cas`, `corelink-quota-fsm` | `2026-05-26-w35-p2-billing-absorption.md` | `corelink-billing::quota` → `crates/corelink-billing/src/quota/` |
| `corelink-abuse` | `2026-05-26-w35-p2-billing-absorption.md` | `corelink-billing::abuse` → `crates/corelink-billing/src/abuse/` |
| `corelink-oncall` | `2026-05-26-w35-p2-ops-absorption.md` | `corelink-ops::oncall` → `crates/corelink-ops/src/oncall/` |
| `corelink-ac-core` | `2026-05-26-w35-p2-ac-absorption.md` | `corelink-ac::ac_core` → `crates/corelink-ac/src/ac_core/` |
| `corelink-drata-sync` | `2026-05-26-w35-p2-ops-absorption.md` | `corelink-ops::drata` → `crates/corelink-ops/src/drata/` |
| `corelink-tenant-offboarding` | `2026-05-26-w35-p2-ops-absorption.md` | `corelink-ops::tenant_offboarding` → `crates/corelink-ops/src/tenant_offboarding/` |
| `corelink-dr-drill` | `2026-05-26-w35-p2-ops-absorption.md` | `corelink-ops::dr::drill` → `crates/corelink-ops/src/dr/drill/` |

Files containing such inline references:

| File | doc_status | Absorbed crate(s) referenced | Disposition |
|---|---|---|---|
| `specs/_runbooks/RB-ONCALL-POLICY.md` | DRAFT | `corelink-oncall` | **FIXED §4 →** `crates/corelink-ops/src/oncall/threshold.rs` |
| `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` | DRAFT | `corelink-oncall` (2 sites) | **FIXED §4** |
| `specs/_compliance/BCP-DR-DRILL-CADENCE.md` | DRAFT | `corelink-oncall`, `corelink-dr-drill` (2 site pairs) | **FIXED §4** |
| `specs/03_architecture/invariant_registry.md` L1033 | DRAFT | `corelink-webauthn` (inline backtick in alias prose) | **ESCALATED §5** — invariant body text; rephrase requires INV-AUTH-WEBAUTHN family editor (not pure repath) |
| `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md` | **FROZEN** | `corelink-chunker` | **ESCALATED §5** |
| `specs/03_architecture/adrs/ADR-0032-webauthn-level3.md` | **FROZEN** | `corelink-webauthn` | **ESCALATED §5** |
| `specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md` | **FROZEN** | `corelink-chunker` | **ESCALATED §5** |
| `specs/tla/multipart_determinism.tla` | (no frontmatter — TLA+ source) | `corelink-chunker` (comment annotation) | **ESCALATED §5** — out of audit scope (`.tla` not `.md`) |
| `specs/04_sprints/_sealed/S05/work_items/WI-S05-002-corelink-chunker-fastcdc-adr-0022.md` | **FROZEN** | `corelink-chunker` | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S05/sprint.md` | (locked at S05 seal) | `corelink-chunker` | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S05/_spec_contract.md` | **FROZEN** | `corelink-chunker` | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S03/_spec_contract.md` | **FROZEN** | `corelink-webauthn` | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S03/work_items/WI-S03-006-webauthn-level3-admin.md` | **FROZEN** | `corelink-webauthn` (12+ sites — all of file 's anchors) | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S07/_spec_contract.md` | **FROZEN** | `corelink-quota` | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S07/PRR-S07.md` | **FROZEN** | `corelink-quota` | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S07/work_items/WI-S07-003-quota-enforcement-middleware.md` | **FROZEN** | `corelink-quota` | **ESCALATED §5** |
| `specs/04_sprints/S08/_spec_contract.md` | **FROZEN** | `corelink-quota`, `corelink-abuse` | **ESCALATED §5** |
| `specs/04_sprints/S08/PRR-S08.md` | **FROZEN** | `corelink-quota`, `corelink-abuse` | **ESCALATED §5** |
| `specs/04_sprints/S08/work_items/WI-S08-003-quota-checker-middleware-atomic-cas.md` | **FROZEN** | `corelink-quota` | **ESCALATED §5** |
| `specs/04_sprints/S08/work_items/WI-S08-004-abuse-detection-heuristica-scoring.md` | **FROZEN** | `corelink-abuse` | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S09/_spec_contract.md` | **FROZEN** | `corelink-canary` | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S09/PRR-S09.md` | **FROZEN** | `corelink-canary` | **ESCALATED §5** |
| `specs/04_sprints/_sealed/S09/work_items/WI-S09-007-synthetic-canary-3-regions-runbook-dry-run.md` | **FROZEN** | `corelink-canary` | **ESCALATED §5** |
| `specs/04_sprints/S10/_spec_contract.md` | **FROZEN** | `corelink-quota` | **ESCALATED §5** |
| `specs/04_sprints/S10/PRR-S10.md` | **FROZEN** | `corelink-quota` | **ESCALATED §5** |
| `specs/04_sprints/S10/work_items/WI-S10-005-quota-state-machine-overage-email.md` | **FROZEN** | `corelink-quota` | **ESCALATED §5** |
| `specs/_audits/sealed/2026-05-22-wave33-code-reorg-spec.md` | (audit doc — historical) | all 7 absorbed | **ESCALATED §5** — by design (charter spec describing the absorption itself) |
| `specs/_audits/2026-05-26-w35-p2-{ac,telemetry,billing,ops,cas}-absorption.md` (5 files) | SEALED audits | all absorbed (by design — these audits *document* the absorption) | **NO ACTION** (intentional historical mention) |
| `specs/_audits/2026-05-16-debt-008-wave2{3,4}-mutation-sweep.md`, `2026-05-16-pre-ga-pentest-scope.md`, `2026-05-16-inv-draft-sweep.md`, `2026-05-22-w33-stream-a-data-path.md`, `2026-05-01-pentest-s03-internal.md` | sealed audits | various | **NO ACTION** (sealed historical audit content) |
| `specs/_audits/sealed/stride-per-crate/STRIDE-corelink-rate-limit.md` | sealed | `corelink-quota` | **NO ACTION** (sealed) |

## §3. Method

1. **Worktree confirm:** `WT="$(pwd)"; echo "$WT" | grep -q "\.claude/worktrees/agent-"`.
2. **Broken-link sweep:** `find specs/ -name '*.md'` → Python 3 regex
   `\[([^\]]+)\]\(([^)#]+)\)` per file → skip schemes
   (`http://`, `https://`, `mailto:`, `#…`) → `os.path.normpath(os.path.join(dirname(f), path))` → `os.path.exists` check.
   Sorted, de-duplicated. **49 unique results.**
3. **Absorbed-crate sweep:** `grep -rln` for 7 known-absorbed crate
   paths against `specs/`. **36 unique files.**
4. **doc_status partition:** every flagged file was inspected via
   `grep -E "^doc_status:|^audit_status:"` against its YAML
   frontmatter. **FROZEN / sealed / AUDITED → no-touch per audit
   policy §4.** DRAFT / unstated → eligible for low-risk repath.
5. **Umbrella mapping:** extracted from the 8 W35-P2 audit docs
   (`2026-05-26-w35-p2-*-absorption.md`) — each lists "Absorbed |
   New canonical path" table.

## §4. Fixes applied (LOW-RISK, DRAFT-only, mechanical repath)

| File | Edit |
|---|---|
| `specs/03_architecture/slo_catalog.md` | L126 — `../../05_quality/runbooks/RB-SLO-AVAIL-CP.md` → `../05_quality/runbooks/RB-SLO-AVAIL-CP.md` (one `../` too deep — file lives at `specs/05_quality/runbooks/…`, source is in `specs/03_architecture/`) |
| `specs/_runbooks/RB-ONCALL-POLICY.md` | L80 — `crates/corelink-oncall/src/threshold.rs` → `crates/corelink-ops/src/oncall/threshold.rs` (absorbed Wave 35 P2 from `corelink-oncall`); annotation appended |
| `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` | L24 — `crates/corelink-oncall` → `crates/corelink-ops/src/oncall/` (annotation appended) |
| `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` | L245 — `crates/corelink-oncall/` → `crates/corelink-ops/src/oncall/` (annotation appended) |
| `specs/_compliance/BCP-DR-DRILL-CADENCE.md` | L23 — `crates/corelink-dr-drill` + `crates/corelink-oncall` → `crates/corelink-ops/src/dr/drill/` + `crates/corelink-ops/src/oncall/` (annotation appended) |
| `specs/_compliance/BCP-DR-DRILL-CADENCE.md` | L327-328 — `crates/corelink-dr-drill/src/lib.rs` + `crates/corelink-oncall/src/lib.rs` → `crates/corelink-ops/src/dr/drill/` + `crates/corelink-ops/src/oncall/` (annotation appended) |

**Total: 6 edits across 4 files. All targets had `doc_status: DRAFT`
+ `audit_status: ACTIVE`.** Each edit preserved the original
crate-name + a Wave 35 P2 absorption breadcrumb for future archeology.

## §5. Orchestrator escalations

The following stale path references were **not** repathed because
they fall outside the inline-fix policy (audit policy §4):

### §5.1 FROZEN / AUDITED specs (immutable under audit policy)

- `specs/_compliance/DRATA-INTEGRATION-COVERAGE.md` (FROZEN)
- `specs/03_architecture/adrs/ADR-0022-chunk-size-vs-part-size-decoupling.md` (FROZEN)
- `specs/03_architecture/adrs/ADR-0032-webauthn-level3.md` (FROZEN)
- `specs/03_architecture/adrs/ADR-0039-chunker-public-api-stability.md` (FROZEN)
- All `specs/04_sprints/{S03,S05,S07,S08,S09,S10,S11}/**` (5+ sprints' worth — every per-sprint contract, PRR, and work-item-doc that touches absorbed crates is FROZEN/AUDITED)

**Recommendation:** establish a `THAWED` re-baseline pass (post-GA) that
treats Wave 35 P2 as a "spec re-anchor event" — at that point each
FROZEN ADR/contract/WI gets a controlled THAW → repath → RE-FREEZE
cycle. Adding a "supersedes/superseded-by-anchor" stanza in the
YAML frontmatter would make this auditable without rewriting the
prose.

### §5.2 DRAFT specs with non-trivial structural drift

- `specs/03_architecture/tenant-offboarding-spec.md` (8 broken links + likely test-file moves) — needs full architect pass: not just path-rename but module-boundary re-statement (the file describes a free-standing crate's design, and post-absorption it's a module of `corelink-ops`, with possibly different lifecycle / API stability promises).
- `specs/_runbooks/RB-DRATA-SYNC-FAILURE.md` (2 broken links + 8+ inline anchors) — operator runbook; the prose treats DRATA sync as a crate-scoped component, but it's now a sub-module of `corelink-ops::drata` sharing umbrella binaries. Path-only repath would leave the *story* stale.
- `specs/_runbooks/RB-TENANT-OFFBOARDING.md` — same shape as the DRATA runbook.
- `specs/03_architecture/invariant_registry.md` L1033 — INV-AUTH-WEBAUTHN family alias text references `corelink-webauthn/README.md`; needs INV-family editor pass (umbrella's `corelink-auth/src/webauthn/` has no top-level README, so the alias needs re-routing to either the umbrella crate's README or a section of the umbrella's docs/).

### §5.3 Historical sprint `../sprint.md` chain (40 broken links across S08-S11 work-items)

All FROZEN. The 5 sprint families never authored `sprint.md` and
the canonical doc became `_spec_contract.md`. S07 already SEALed the
in-WI fix in v1.6.0 changelog; **S08/S09/S10/S11 retain leftover
`../sprint.md` pointers in 40 WIs**. Mechanical sweep would be safe
in principle (10s of identical replacements), but all targets are
FROZEN/AUDITED.

**Recommendation:** include in the post-GA re-baseline pass (§5.1)
under "errata-only THAW" — single-token edit, can be batched.

### §5.4 Out of audit-scope

- `specs/tla/multipart_determinism.tla` — TLA+ source, not markdown; comment-level reference to `corelink-chunker`. TLA reorg belongs to a model-checking sweep.

## §6. Summary

| Bucket | Count |
|---|---|
| Broken markdown links scanned | 49 |
| ↳ real DRAFT breakages fixed inline | 1 site (slo_catalog `../05_quality` typo) + 6 absorbed-crate path repaths across 3 DRAFT files = **7 edits across 4 files** |
| ↳ false positives (template placeholders) | 4 |
| ↳ false positives (markdown-link regex over math/code-span) | 3 |
| ↳ FROZEN historical `../sprint.md` chain | 40 (deferred to §5.3) |
| ↳ DRAFT specs with structural drift | 1 (tenant-offboarding-spec.md, 8 links — §5.2) |
| ↳ FROZEN spec broken links | 4 (drata + 2 RB DRAFT runbooks with line-anchor refs — §5.2) |
| Inline absorbed-crate path references | 36 files |
| ↳ DRAFT files repathed | 3 (RB-ONCALL-POLICY, ONCALL-ESCALATION-MATRIX, BCP-DR-DRILL-CADENCE) |
| ↳ FROZEN / sealed / out-of-scope | 33 |

**Net inline fixes:** 7 mechanical edits to 4 DRAFT files. Zero
FROZEN files modified. Zero new files created (other than this
audit doc).
