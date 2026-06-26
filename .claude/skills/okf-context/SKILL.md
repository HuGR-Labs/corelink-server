---
name: okf-context
description: Pull the relevant code-grounded OKF wiki concepts into context BEFORE modifying a CoreLink subsystem (auth, billing, a cache surface, storage, tenancy, a plane, a crate). Invoke whenever you are about to read/edit code in an area you have not just loaded the architecture for — it surfaces the concepts that declare those exact files so you work WITH the architecture, not blind. Triggers — about to touch a file/subsystem; "how does X work here"; before a refactor/fix in an unfamiliar area.
---

# okf-context — load the architecture wiki before you touch the code

CoreLink ships a code-grounded architecture wiki at `docs/knowledge/` (146 OKF
concepts, anti-drift gated: each concept names the `source_files` it explains).
`scripts/okf_context.py` routes the relevant concepts INTO your context so you
work with the grounded architecture instead of re-deriving it from raw code.

## When to invoke

Invoke BEFORE modifying (or deeply reading) a subsystem you have not just
loaded context for:

- about to edit/read a specific file → query by `--file`.
- about to work an area (auth, billing, storage, tenancy, a surface) → query by
  `--tag` or the taxonomy dir name.
- unsure what's relevant → free text against concept ids/titles.

Skip it only when you have already loaded the matching concepts this session.

## How to invoke

Run from the repo root (stdlib-only, no deps):

```bash
# the file you are about to change — returns concepts that DECLARE it
python3 scripts/okf_context.py --file crates/corelink-container/src/adapter_pat.rs

# the whole area
python3 scripts/okf_context.py --tag auth
python3 scripts/okf_context.py auth          # bare taxonomy dir name == same area

# free text against id/title
python3 scripts/okf_context.py billing quota

# add --full to print the concept BODIES (load them into context), not just ids
python3 scripts/okf_context.py --file worker/src/lib/internal_auth.ts --full
```

## Procedure

1. Identify the file(s) / area you are about to work on.
2. Run `okf_context.py --file <path>` for each load-bearing file; if you don't
   have an exact path yet, run `--tag <area>` or the taxonomy dir name.
3. Re-run with `--full` on the returned ids to pull the concept bodies into
   context. Read them — they carry the invariants, the cheap-then-deep moats,
   the fail-closed rules, the cross-plane contracts.
4. NOW make the change, honoring what the concepts assert. If your change
   contradicts a concept, the concept (or your change) is wrong — reconcile,
   don't silently drift (the wiki is anti-drift gated against `source_files`).

Exit codes: `0` matches printed, `1` no concept matches (fine — area may be
unconcepted), `2` bad usage / missing bundle.
