# GA-Readiness DEFER Scrub + Drift Detector — 2026-05-16 (wave-25)

> **Doc kind:** wave-25 hygiene audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-25 GA-readiness DEFER-scrub agent (Claude Opus 4.7) — branch `wt/r-prep-ga-checklist-drift-detector`.
> **Base:** `main` @ `e9ee8eb` ("merge wt/r-prep-pat-clerk-mutation-sweep into main (wave-24)" — wave-24 SEAL tip).
> **Scope:** scrub the stale "Docs CI billing reinstatement" DEFER row from the two GA-readiness sign-off docs and add a drift detector that catches equivalent stale signals on every future PR.
>
> **Companion docs:**
> - `specs/_audits/2026-05-16-ga-readiness-final.md` (the audit whose §11 DEFER counter was decremented 8 → 7).
> - `specs/_audits/2026-05-16-ga-final-checklist.md` (the operator-runnable checklist whose §G row was removed; G-08 promoted to G-07).
> - `scripts/ga-readiness-defer-drift.py` (the new drift detector).
> - `.github/workflows/spec_validation.yml` (the workflow that wires the detector as an advisory CI step).
> - User memory: `feedback_ci_local` (the source-of-truth stance: CI runs locally; GHA infra is not the canonical CI surface).

---

## §1. Why this scrub

The wave-22 GA-readiness audit landed with **8 external DEFER items** in §11. Row #7 read:

> **Docs CI billing reinstatement** — User-bound (ops) — Gustavo (GitHub-billing acct issue) — None — out-of-stream resolution; local `pnpm build` used until resolved — Anytime before GA.

This row is **stale by construction.** The canonical CI surface for this codebase is the local validator chain (`validate_specs.py`, `validate_references.py`, `cargo test`, `pnpm build`, …) run synchronously per the `feedback_ci_local` user-memory stance. GitHub Actions hosts an *advisory* mirror of those validators, but GHA-billing reinstatement is **not** on the GA-blocker path because nothing on the cutover plan reads a GHA workflow status. Treating it as a DEFER item misframes the GA-readiness picture and inflates the apparent external-dependency surface by one.

Wave-25 removes the row, decrements the §11 counter, and re-classifies the residual external dependencies as `5 user-bound + 1 vendor-bound + 1 mixed = 7`.

## §2. What changed

### §2.1 `specs/_audits/2026-05-16-ga-readiness-final.md`

- §1.1 recommendation prose: "5 user-bound + 3 vendor-bound external dependencies" → "5 user-bound + 1 vendor-bound + 1 mixed external dependencies"; "those 8 external items" → "those 7 external items"; appended inline scrub annotation.
- §1.2 state-of-the-world table: External-dependencies row "8 DEFER (5 user-bound + 3 vendor-bound)" → "7 DEFER (5 user-bound + 1 vendor-bound + 1 mixed)".
- §1.2 net-verdict sentence: "8 external DEFER items" → "7 external DEFER items".
- §11 lead paragraph: "8 items below" → "7 items below"; appended inline scrub annotation.
- §11 table: row #7 ("Docs CI billing reinstatement") removed; former row #8 ("Owner sign-off ADR-0034b 2-key") promoted to #7.
- §11 trailer: "Total DEFER counter: 8 (5 user-bound + 2 vendor-bound + 1 mixed)" → "Total DEFER counter: 7 (5 user-bound + 1 vendor-bound + 1 mixed)"; appended inline scrub annotation.
- Final-line sign-off banner: "the 8 external DEFER items in §11" → "the 7 external DEFER items in §11".

### §2.2 `specs/_audits/2026-05-16-ga-final-checklist.md`

- Lead rule callout: "(the 8 external DEFER items)" → "(the 7 external DEFER items, scrubbed wave-25)".
- §G checklist: G-07 ("Docs CI billing reinstatement (§11#7)") removed; former G-08 ("Owner sign-off (ADR-0034b 2-key) … (§11#8)") promoted to G-07 (and re-pointed to §11#7); inline scrub annotation appended on the promoted row.
- Roll-up "Total rows" 95 → 94 (G: 8 → 7).
- Roll-up "Decision rule" reference to §11 counter: 8 max → 7 max post wave-25 scrub.
- Roll-up "Expected DEFER count at signature" 8 → 7.

### §2.3 New: `scripts/ga-readiness-defer-drift.py`

A pure-stdlib, dependency-free Python 3.11 script that scans the two GA-readiness docs for stale DEFER signals on **active rows only** (§11 table data rows + checklist `- [ ]` rows). It is exempt-aware: any line containing a wave-25 scrub marker is skipped, so the inline scrub annotations do not themselves trip the detector.

Stale signals (case-insensitive exact phrase):

- `GHA billing`
- `GitHub Actions billing`
- `GitHub-billing`
- `Docs CI billing`
- `CI offline`
- `Workflows offline`

Exit codes: `0` clean / `1` drift detected. Tested locally:

- Positive: synthetic injection of `- [ ] G-99 — GHA billing reinstatement` correctly flagged and exempted-when-scrub-marker.
- Negative: post-scrub corpus runs clean (exit 0).

### §2.4 New advisory step: `.github/workflows/spec_validation.yml`

Appended a `python3 scripts/ga-readiness-defer-drift.py` step after the existing cost-regression check. The new step inherits the dependency-free install from `requirements-ci.txt` (no new pip deps required). `actionlint` clean.

The detector is **also** runnable locally as part of any reviewer's pre-merge sweep: `python3 scripts/ga-readiness-defer-drift.py` from repo root.

## §3. Quality gates

| Gate | Result |
|---|---|
| `python3 scripts/ga-readiness-defer-drift.py` | exit 0 (clean) |
| `python3 scripts/validate_specs.py` | PASS (446 + 9 = 455 docs) |
| `python3 scripts/validate_references.py` | PASS (no dangling refs) |
| `actionlint .github/workflows/spec_validation.yml` | exit 0 (clean) |
| Smoke-test detection + exemption | PASS |

## §4. Forward note

If a future agent is tempted to re-add a "Docs CI billing" / "GHA billing" / "Workflows offline" DEFER row, the detector will trip on the PR and surface the row with the resolution text: "CI runs locally per `feedback_ci_local`; GHA infrastructure is not the canonical CI surface." That keeps the §11 counter honest without requiring a human to re-litigate the same scrub.

## §5. Sealed artifacts

- `specs/_audits/2026-05-16-ga-readiness-final.md` — scrubbed (8 → 7 DEFER).
- `specs/_audits/2026-05-16-ga-final-checklist.md` — scrubbed (G-07 removed, G-08 → G-07).
- `scripts/ga-readiness-defer-drift.py` — new drift detector (synchronous, stdlib).
- `.github/workflows/spec_validation.yml` — advisory step wired in.
- `specs/_audits/2026-05-16-ga-readiness-defer-scrub.md` — this audit.
