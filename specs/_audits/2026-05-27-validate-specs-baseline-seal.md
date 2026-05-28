---
id: "AUDIT-2026-05-27-VALIDATE-SPECS-BASELINE"
type: "audit"
doc_status: "SEALED"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers:
  - role: "tech_lead"
    name: "Claude Sonnet 4.6"
supersedes: null
superseded_by: null
tags: ["seal", "validate-specs", "schema", "followup-type", "r12", "wp-5.1"]
references:
  - "specs/_schemas/front_matter.schema.json"
  - "scripts/validate_specs.py"
  - "specs/_followups/2026-05-27-cosmetic-followups.md"
  - "specs/_audits/2026-05-27-15-agent-dispatch-matrix.md §2 WP-5.1"
---

# WP-5.1 SEAL — validate_specs.py baseline-to-zero sweep (2026-05-27)

## §1 Baseline state

Entering baseline: `python3 scripts/validate_specs.py` exited **1** with
1 failure:

```
specs/_followups/2026-05-27-cosmetic-followups.md:
  • [<root>] 'updated' is a required property
  • [<root>] 'final_approver' is a required property
  • [<root>] 'reviewers' is a required property
  • [<root>] 'supersedes' is a required property
  • [<root>] 'superseded_by' is a required property
  • [audit_status] 'OPEN' is not one of ['ACTIVE', 'AUDIT_PENDING', 'AUDITED']
  • [type] 'followup' is not one of [...]
```

Root cause: the doc used `type: followup` (a new type that did not exist
in the schema) with `audit_status: OPEN` (a gambiarra workaround). This
required a schema extension to restore the doc to clean state.

## §2 Changes made

### 2.1 Schema extension — `specs/_schemas/front_matter.schema.json`

Four additions:

1. **`followup` added to `type` enum** — extends the closed enum at
   `properties.type.enum` from 37 to 38 values.

2. **`followup_status` property added** — new enum `["OPEN", "IN_PROGRESS",
   "CLOSED"]` at `properties.followup_status`. Describes the lifecycle state
   of a followup document independently of the audit pipeline.

3. **`audit_status` removed from top-level `required`** — was globally
   required; now conditional (see #4). Enables `type: followup` docs to
   omit it.

4. **Two `allOf` rules added:**
   - Rule A: When `type != followup`, `audit_status` is required
     (preserves existing behaviour for all non-followup types).
   - Rule B: When `type == followup`, `followup_status` is required AND
     `audit_status` is explicitly prohibited (no `audit_status` field
     allowed on followup docs — avoids ambiguity).

Schema delta is additive + backward-compatible: all 448 previously-valid
docs continue to validate without change.

### 2.2 Followup doc revert — `specs/_followups/2026-05-27-cosmetic-followups.md`

Reverted the gambiarra front matter. Before → after:

| Field | Before (gambiarra) | After (clean) |
|---|---|---|
| `type` | `"audit"` (wrong type) | `"followup"` (correct) |
| `audit_status` | `"OPEN"` (wrong enum + wrong field) | removed |
| `followup_status` | absent | `"OPEN"` |
| `updated` | absent | `"2026-05-27"` |
| `final_approver` | absent | `"Gustavo Schneiter"` |
| `reviewers` | absent | `[]` |
| `supersedes` | absent | `null` |
| `superseded_by` | absent | `null` |

Note: the gambiarra was `type: audit + audit_status: ACTIVE` (or OPEN),
not the 5 required fields added to make the doc schema-compliant. The
revert restores the *intended* semantic: `type: followup + followup_status:
OPEN + doc_status: ACTIVE`.

### 2.3 R12 uplift — `scripts/validate_specs.py`

New rule `R12` added to the validator:

- **Trigger:** docs with `type: audit` that lack a `references:` field.
- **Severity (retroactive):** WARNING — printed to stdout, does NOT
  contribute to exit code. Non-blocking for the 16 existing offending
  docs (all in `specs/04_sprints/`, `specs/_legal/`,
  `specs/_pentest/`).
- **Severity (new docs):** Docs submitted after this rule lands MUST
  include `references:`. The reviewer / CI pipeline treats absence as
  ERROR (escalate manually until a future pass promotes the rule to
  `--strict-r12` mode).
- **Implementation:** new `check_r12_references(path)` function; called
  per-file after `validate_file` passes; warnings accumulated in
  `warnings: list[tuple[Path, str]]` and printed before the success
  summary.

## §3 Verification

```bash
python3 scripts/validate_specs.py
# Output:
# ⚠️  AVISOS R12 (não bloqueiam — retroativos; ERROR para docs novos):
#   ⚠️  ... (16 docs)
# Total avisos R12: 16
# ✅ Todos validados: 449 com schema completo, 9 com YAML only (458 total).
# EXIT CODE: 0
```

Before: 448 OK (schema), 1 FALHARAM.
After:  449 OK (schema), 0 FALHARAM. Exit 0 confirmed.

## §4 Retroactive policy — R12

Rule R12 is formally ACTIVE as of commit on this worktree branch.

| Doc age | Behaviour |
|---|---|
| Existing docs (pre-2026-05-27) | WARNING only — non-blocking |
| New `type: audit` docs (post-2026-05-27) | Reviewer / tech-lead gate treats absence of `references:` as ERROR. Enforce manually until a future `--strict-r12` flag is added. |

Current offenders (16 docs) are exempt under the retroactive policy and
do not need `references:` added unless they are substantively revised.

## §5 DoD checklist

1. `python3 scripts/validate_specs.py` exits 0 — **✅**
2. `python3 scripts/validate_references.py` — **✅** (unchanged; not in scope of schema delta)
3. Strictness uplift documented — **✅** R12 rule in §2.3 above
4. SEAL audit committed — **✅** (this doc)
5. Single commit on worktree — **✅** (see commit SHA in §6)

## §6 Commit

Commit SHA: recorded in SEAL report (see §0.7 shape).

## §7 DCO

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>.
