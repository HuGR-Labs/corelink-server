---
name: okf-reconcile
version: 1.0.0
description: The LLM half of the OKF self-healing loop. Invoke when the OKF freshness gate (C5) goes red, when `scripts/okf_reconcile.py` reports a stale concept, or when a PR touches a cited code line-range under a knowledge concept. Runs the deterministic reconcile reporter, then re-authors each stale concept faithfully against HEAD (cite the implementing line, not the doc-comment), advances its checkpoint_sha, and re-runs validate_okf until green. Refuses phantom reconciles (C5b).
---

# /okf-reconcile — re-author drifted knowledge concepts against HEAD

The OKF knowledge wiki (`docs/knowledge/`) is gated by `scripts/validate_okf.py`
against the FROZEN contract `docs/internal/okf-wiki/01-okf-corelink-profile.contract.md`.
When code under a concept's cited line ranges changes, **C5** (freshness) goes
red. `scripts/okf_reconcile.py` computes the deterministic worklist — *which*
concepts drifted, *which* source files moved, *which* cited ranges were hit, and
the diff hunks. This skill is the judgement half: read the changed code, re-write
the stale concept so every claim is true again, and advance its checkpoint.

> The deterministic tool tells you WHAT drifted. You decide HOW to reconcile it.
> Never advance a `checkpoint_sha` or a `source_blobs` anchor without a real body edit — that is a phantom
> reconcile and **C5b** will (rightly) reject it.

## Procedure

1. **Get the worklist.** Run the reporter:
   - `python3 scripts/okf_reconcile.py` (human brief), or
   - `python3 scripts/okf_reconcile.py --json` (structured).
   Exit is always 0; "0 stale" means nothing to do.

2. **For each stale concept** in the worklist:
   a. **Read the stale concept** end-to-end: `docs/knowledge/<id>.md`. Note its
      `source_files`, `checkpoint_sha`, and every `# Citations` entry.
   b. **Read the changed code at HEAD** for each drifted `source_file`, focused on
      the `changed_cited_ranges` the worklist names. Use the diff hunks to see
      exactly what moved (a function renamed, a guard reordered, lines shifted).
   c. **Re-author faithfully against HEAD.** Update prose, `# How it works`, and
      `# Invariants` so every claim matches the code as it is now. Follow the
      frozen contract verbatim:
      - **Cite the implementing line, not the doc-comment.** Point `path:line` at
        the code that *does* the thing, never at a comment or signature that
        merely describes it (the doc-extraction truth-audit rule).
      - Every bullet under `# How it works` / `# Invariants` carries ≥1 `path:line`
        token (C6c). Every `source_files` path appears ≥1× under `# Citations`
        (C6). Every cited file is in `source_files` (C6b).
      - Fix line numbers to HEAD coordinates; add/remove a `source_files` entry if
        behavior moved to a new file (then cite it — C6/C6b).
      - Keep it grounded: no claim that is not visible in the cited lines. Prefer
        deleting an unprovable sentence over guessing.
   d. **Advance the checkpoint.** Set `checkpoint_sha:` to the current HEAD SHA
      (`git rev-parse HEAD`). Because step (c) edited the body, C5b is satisfied.
      If a concept is `provenance: GENERATED`, keep the `> GENERATED` marker.
   e. **Advance the BLOB anchors** (concepts that carry `source_blobs`). For every
      re-authored file, set its entry to the file's blob id at the content you just
      cited: `git hash-object <path>` (equivalently `git rev-parse HEAD:<path>` once
      committed).

   > **Mechanize (d)+(e) — do NOT hand-edit the sha.** Run
   > `python3 scripts/okf_reanchor.py <concept.md…>` (or bare, to auto-detect every
   > modified `docs/knowledge/*.md`). It writes the FULL 40-hex `git rev-parse HEAD`
   > into `checkpoint_sha` and rewrites each `source_blobs` entry to its
   > `git hash-object`. Hand-editing these has cost two CI cycles: a SHORT sha (the
   > gate demands 40-hex) and an ORPHANED sha written before a rebase (the gate then
   > falls back to the base-ref and reports the concept stale). Run it AFTER the body
   > re-author in (c) — never as a substitute for one (a bump with no body edit fails
   > C5b). **Especially re-run it after any rebase/force-push**, since the rebase
   > orphans the commit the old `checkpoint_sha` named.

      The anchor is what C5 actually compares against, so a stale entry
      keeps the concept red; a MISSING entry for a path that had one fails C4c
      (blob addressing is a ratchet — never delete an anchor to make a red go away).
      A blob anchor is immutable under rebase/squash/cherry-pick, so unlike
      `checkpoint_sha` it does NOT need re-touching after a rebase: if the reconcile
      you are doing was triggered only by a rebase and the cited files are
      byte-identical, a blob-addressed concept has nothing to reconcile at all.

3. **Re-validate until green.** Run from the repo root:
   `python3 scripts/validate_okf.py` (PyYAML in a venv if the manifest is parsed).
   Iterate on any C1–C10b failure. Re-run `python3 scripts/okf_reconcile.py` to
   confirm `0 stale`. If a `source_files` set changed, also run
   `python3 scripts/okf_status.py` so the manifest `status:` stays in sync.

4. **Open a reconciliation PR.** One concern: the drifted concepts and their
   checkpoint advances (plus any `source_files`/citation corrections). Title e.g.
   `docs(okf): reconcile <N> concept(s) drifted by <change>`. The okf_wiki.yml gate
   must pass. End the commit with the `Co-Authored-By: Claude …` trailer and the PR
   body with the Generated-with footer.

## Guardrails

- **No phantom reconcile.** A `checkpoint_sha` bump with no body change fails C5b
  by design. If the cited code genuinely did not change meaning, re-examine — the
  diff hunk intersected a cited range for a reason.
- **Don't widen the contract.** This skill transcribes the frozen profile; it
  never relaxes a check. Contract changes are the lead's call (a `profile_version`
  bump), not a reconcile.
- **Ground every claim.** The point of the wiki is a knowledge layer that cannot
  silently lie. If you cannot cite it at HEAD, it does not go in.

## See also

- Contract: `docs/internal/okf-wiki/01-okf-corelink-profile.contract.md` (§3 body,
  §4 checks C1–C10b + C5/C5b/C6c).
- Validator: `scripts/validate_okf.py`. Reporter: `scripts/okf_reconcile.py`.
  Status sync: `scripts/okf_status.py`. Nightly WARN: `.github/workflows/okf_nightly.yml`.
