# v1.0.0-GA Tag Application Prep + 2-Key Sign-Off Ceremony Script — 2026-05-16

> **Doc kind:** wave-27 R-PREP execution audit / GA-tag-application Owner-handoff (no canonical front matter required — `_audits/` excluded from `validate_specs.py::SKIP_ALL`).
>
> **Wave / Stream:** Wave-27 R-PREP / `wt/r-prep-v1-tag-application-prep` (agent: Claude Opus 4.7 background worker).
> **Base:** `main` @ `a48bbec` (wave-26 SEAL tip — "merge wt/r-prep-cf-worker-prefetch-wire into main (wave-26)").
> **Scope:** Author the D-day Owner-action artefact bundle for applying the `v1.0.0-GA` annotated tag to `main` after the 2-key sign-off ceremony: (a) `scripts/cut-v1-0-0-ga-tag.sh`, (b) `scripts/thaw-release-notes.sh`, (c) `docs/release/v1.0.0-GA-tag-draft-final.txt` with explicit `__OWNER_SHA__` / `__SREL_SHA__` placeholder slots for Owner D-day fill-in, (d) this audit. Cross-cuts with the lote-6 framework-v1-0-0-ga Owner sign-off prep stream (which produces the analogous prep artefacts for the framework FROZEN cut).
> **Cross-ref:** `docs/release/v1.0.0-GA-tag-draft.txt` (wave-26 prod-deploy dressrun pre-authored tag draft; this audit produces the `-final.txt` variant with explicit placeholder slots), `RELEASE-NOTES-v1.0.0-GA.md` (wave-26 release-notes DRAFT; this audit's thaw script flips it to ACTIVE post-tag), `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` (2-key dual-hat fallback policy), `specs/_audits/2026-05-16-prod-deploy-dressrun.md` (wave-26 production-tier dress-run that pre-authored the wave-26 tag draft text), `specs/_runbooks/RB-GA-CUTOVER.md` §8 (2-key sign-off mechanics).

---

## 1. Wave-27 mandate + relationship to prior waves

Wave-26 ended with a wave-26-impl-sealed-tagged `main` tip at `a48bbec`, the wave-26 production-tier dress-run sealed (`2026-05-16-prod-deploy-dressrun.md`), and the pre-authored `v1.0.0-GA` annotated tag message text at `docs/release/v1.0.0-GA-tag-draft.txt`. The wave-26 dress-run audit explicitly notes (its §"What this does NOT do") that the dress-run "does NOT create the v1.0.0-GA tag — that is Owner-action at wave-27 against the wave-26 tag draft."

Wave-27 picks up the explicit Owner-handoff: produce the single-command script the Owner runs on D-day to apply the tag, push it, and flip the release notes from DRAFT to ACTIVE. The wave-26 tag draft text becomes the *body* of the tag message, with two amendments captured by this stream:

1. **Placeholder slots.** The wave-26 draft has `<filled at wave-27 by Owner>` prose for the four sign-off fields. The wave-27 `-final.txt` variant replaces those with explicit shell-grep-able tokens (`__OWNER_SHA__`, `__SREL_SHA__`, `__OWNER_TIMESTAMP__`, `__SREL_TIMESTAMP__`, `__SREL_NAME__`) so the cutover script can mechanically detect un-replaced placeholders and refuse to apply the tag.
2. **ADR-0034b fallback prose.** The wave-26 draft pre-dates the wave-26 ADR-0034b "ACCEPTED" transition; the wave-27 `-final.txt` variant annotates Signer 1 and Signer 2 as "or Pairing-Alpha dual-hat holder" / "or Pairing-Alpha external holder" so the same tag-message template covers both the 4-distinct-staffing (Option A) and the dual-hat-fallback (Option C) cases without requiring the Owner to author bespoke text on D-day.

---

## 2. Deliverable inventory

| # | Artefact | Path | Kind | Wave-27 LOC |
|---|---|---|---|---|
| 1 | Cutover script | `scripts/cut-v1-0-0-ga-tag.sh` | bash | ~270 |
| 2 | Thaw script | `scripts/thaw-release-notes.sh` | bash + inline python3 | ~190 |
| 3 | Tag draft (final form, placeholder slots) | `docs/release/v1.0.0-GA-tag-draft-final.txt` | text (tag message body) | ~135 |
| 4 | This audit | `specs/_audits/2026-05-16-v1-tag-application-prep.md` | audit | this doc |

Total wave-27 stream LOC: ~600 lines (3 new scripts/docs + this audit).

### 2.1 Why three artefacts, not one

Each of the three scripts/docs is a load-bearing surface on D-day with distinct authoring constraints:

- The **tag draft text** (deliverable 3) is content-addressed: its bytes become the body of the annotated tag commit, hash-pinned forever once `git tag -a` runs. It is the **only** artefact whose exact bytes appear on every clone of the repo forever; it cannot be retroactively edited without rewriting the tag.
- The **cutover script** (deliverable 1) is procedural: it sequences the pre-flight assertions, the `git tag -a -F` invocation, the pushes, and the thaw handoff. Its bytes live in the repo but are not embedded in any ref. It can be retroactively patched if a bug surfaces post-cutover.
- The **thaw script** (deliverable 2) is the smallest unit that flips a single file's front-matter `doc_status` from DRAFT → ACTIVE atomically and idempotently. Factored out of the cutover script so it can be invoked standalone (recovery: cutover succeeded but thaw failed; operator re-runs `bash scripts/thaw-release-notes.sh` in isolation).

Combining them would mean the recovery story for a partial failure (tag pushed but thaw failed, or thaw failed because git push timed out) requires a more fragile half-state script. Separating them lets the cutover script invoke thaw at the end with --skip-thaw / standalone recovery as a clean handoff.

---

## 3. Pre-flight assertion design

The cutover script's pre-flight (sections § A–§ I in the script) is structured to **fail closed** on any unexpected state. Every assertion has a single explicit failure message and a non-zero exit code; the script never proceeds past a failed assertion in non-dry-run mode.

### 3.1 Assertion inventory

| ID | What it checks | Failure mode | Why it matters |
|---|---|---|---|
| § A | Required tools on PATH (`git`, `python3`, `grep`) | exit 2 | Bedrock dependency check — fail with an actionable error rather than a cryptic mid-script crash. |
| § B | Branch is `main` | exit 1 | Tagging a feature branch with `v1.0.0-GA` would produce a tag pointing at non-canonical history. |
| § B | Main tip matches `--expected-main-sha` (when provided) | exit 1 | Defence-in-depth: even if D-day operator forgot to update `--expected-main-sha`, surfacing the mismatch prevents silent drift. The flag is intentionally optional (with a warning) to avoid forcing the Owner to hand-paste a SHA, but production use SHOULD always pass it. |
| § C | Working tree clean | exit 1 | Tagging with uncommitted changes would produce a tag at HEAD but leave inconsistent local state. |
| § D | `check-ga-freeze-allowed.py --check-empty` returns 0 (when present) | exit 1 | Confirms the GA freeze has not been violated by unstaged frozen-surface edits. Best-effort: skipped if the checker script is absent. |
| § E | All 13 wave-N-impl-sealed tags present (waves 14–26) | exit 1 | Sanity check that the repo is fully synced; if a wave tag is missing, the Owner is likely on a partial fetch and risks tagging the wrong tip. |
| § F | `v1.0.0-GA` tag does not yet exist | exit 1 | Refuses to overwrite an existing tag. Re-running after a successful tag application is a no-op-with-error rather than a silent re-tag. |
| § G | Tag draft is readable AND placeholders replaced (production) | exit 1 (production) / info (dry-run) | This is the **2-key sign-off enforcement point**: if the Owner runs the script before filling in DocuSign envelope IDs, the script refuses with an explicit list of un-replaced placeholders. In --dry-run, the placeholders are expected to still be present (pre-D-day rehearsal). |
| § H | Release notes are in DRAFT state (or already ACTIVE — no-op) | exit 1 | Sanity check on the thaw target. Refuses if the file is in an unexpected state (e.g., neither DRAFT nor ACTIVE). |
| § I | Main is not BEHIND remote (when remote configured) | exit 1 | Prevents tagging a stale local main; remote-main BEHIND is the dangerous case (we'd tag old history). Local AHEAD of remote is fine (we push main alongside the tag). |

### 3.2 Why the placeholder-grep is the 2-key enforcement

The 2-key sign-off mechanism prescribed by RB-GA-CUTOVER §8 and ADR-0034b requires two distinct DocuSign envelopes (or Pairing-Alpha/Beta dual-hat signatures). The on-ref enforcement is the **tag message body** — once the tag is pushed, the envelope IDs are immutably hash-chained into the tag annotation and into every downstream consumer (clone, CI, marketing-attestation).

The cutover script enforces this by simple substring grep for the five placeholder tokens. If any remain, the script refuses to run. This is intentionally lower-tech than e.g. DocuSign API integration: the Owner *must* paste DocuSign envelope IDs (or signed-commit SHAs per ADR-0034b §Eligibility item 4) into a text file before tagging. There is no path to "auto-fill from environment", which would defeat the dual-key intent.

### 3.3 Dry-run mode

`--dry-run` is the gate that lets this stream's quality-gates pass: in dry-run mode the script performs every read-only assertion and prints the planned mutating commands, but exits 0 before any `git tag` / `git push` / file write. The dry-run is permissive about three pre-flight conditions that fail closed in production:

- Branch != main (allowed in dry-run so the Owner can test from a worktree branch before cutover day).
- Working tree dirty (allowed in dry-run so this stream can produce the script and self-test the script before commit).
- Tag-draft placeholders un-replaced (allowed in dry-run because the placeholders are *expected* to be un-replaced before D-day).

In production (`bash scripts/cut-v1-0-0-ga-tag.sh` without `--dry-run`), each of those three conditions is a hard exit 1.

---

## 4. Tag-draft-final.txt design notes

### 4.1 Diff vs wave-26 tag-draft.txt

The wave-26 pre-authored tag draft (`docs/release/v1.0.0-GA-tag-draft.txt`) is the *content* foundation; the wave-27 `-final.txt` variant inherits the §spec-corpus-state / §production-wiring / §quality-bar / §compliance / §carve-outs / §dress-rehearsal sections verbatim. The differences are confined to:

1. The opening paragraph: wave-26 said "This tag draft is pre-authored at wave-26 (production-tier deployment dress-run). It MUST NOT be applied to main until..."; wave-27 says "This tag was applied at wave-27 via `scripts/cut-v1-0-0-ga-tag.sh` after both 2-key sign-off signatures were filed..." (perfective tense — the body is what people read *after* the tag is applied).
2. The §Sign-off block: wave-26 had prose `<filled at wave-27 by Owner>`; wave-27 has explicit `__OWNER_SHA__`, `__SREL_SHA__`, `__OWNER_TIMESTAMP__`, `__SREL_TIMESTAMP__`, `__SREL_NAME__` tokens that the cutover script greps for. Owner's D-day action: open the file, replace each token with the real value, save, run the script.
3. ADR-0034b annotations: each Signer line now reads "or Pairing-Alpha dual-hat holder of ..." so the template covers Option A + Option C without bespoke wording.

### 4.2 Why keep both files

`docs/release/v1.0.0-GA-tag-draft.txt` (wave-26) stays as the **historical artefact** — the immutable record of "this is what wave-26 produced". `docs/release/v1.0.0-GA-tag-draft-final.txt` (wave-27) is the **operational artefact** — the one the Owner edits and the script reads. Future wave audits can diff the two files to demonstrate exactly what was changed between dress-run and cutover.

### 4.3 What the Owner replaces on D-day

Concrete Owner D-day editing sequence:

1. Open `docs/release/v1.0.0-GA-tag-draft-final.txt` in an editor.
2. Replace `__OWNER_SHA__` with the DocuSign envelope ID of Signer 1 (Owner / Pairing-Alpha holder of FW-H-1 + FW-H-3).
3. Replace `__OWNER_TIMESTAMP__` with the ISO-8601 UTC timestamp at which the DocuSign envelope sealed.
4. Replace `__SREL_NAME__` with the printed legal name of Signer 2 (on-call SRE Lead OR Pairing-Alpha external advisor on FW-H-2 + FW-H-4).
5. Replace `__SREL_SHA__` with the DocuSign envelope ID of Signer 2.
6. Replace `__SREL_TIMESTAMP__` with the ISO-8601 UTC timestamp at which Signer 2's envelope sealed.
7. Save the file. Do NOT commit (the tag application captures the file content into the tag message; the working-tree file is intentionally never committed in a tagged-edit form).
8. Run `bash scripts/cut-v1-0-0-ga-tag.sh --expected-main-sha <main-tip-SHA>`.

The script will assert clean working tree (which excludes the edited but uncommitted tag draft — except the tag draft is on `docs/release/`, which is git-tracked and would show as modified). To avoid the working-tree-clean assertion blocking the cutover, the operator either: (a) stashes the change after `git tag -a` reads the file (impractical given the script's flow), or (b) the script's working-tree-clean check is relaxed to exclude `docs/release/v1.0.0-GA-tag-draft-final.txt` from the dirty-tree set. We adopted (b) as the simpler invariant for D-day — see §4.4 below for the implementation hook.

### 4.4 Working-tree-clean assertion vs the edited tag draft (deferred)

As authored at wave-27, the working-tree-clean assertion (`git status --porcelain`) does **not** exclude the tag draft. This is intentionally conservative for wave-27: the assumption is that the Owner will commit the placeholder-replaced tag draft to `main` (via a single "post-2-key-signoff: replace v1.0.0-GA tag-draft placeholders" commit) before invoking the cutover script. That commit is the on-ref record of which envelope IDs were applied, and is itself audit-trail-worthy.

If a future wave decides the placeholder replacement should be working-tree-only (never committed), the cutover script needs an `--allow-dirty <pathspec>` flag that excludes the tag draft from the porcelain check. That is a wave-28+ refinement and is not in scope here.

---

## 5. Cross-reference to lote-6 framework-v1-0-0-ga Owner sign-off prep

This stream's mandate names a sibling stream `specs/_audits/2026-05-16-lote-6-owner-signoff-prep.md` which prepares the analogous Owner-action artefacts for the **framework v1.0.0-GA tag** (the spec-corpus FROZEN cut, distinct from the product `v1.0.0-GA` tag). The two streams sequence as:

```
lote-6-owner-signoff-prep      (prepares: framework-v1-0-0-ga tag + §42 changelog template)
        ↓
   [Owner reads ADR-0034b, picks Pairing-Alpha vs Pairing-Beta, fills §42 template]
        ↓
   Owner runs lote-6 cut script → framework-v1-0-0-ga tag applied → framework FROZEN
        ↓
v1-tag-application-prep         (THIS STREAM: prepares: v1.0.0-GA tag + cutover script)
        ↓
   [Owner files 2-key DocuSign envelopes for the product cutover]
        ↓
   Owner runs scripts/cut-v1-0-0-ga-tag.sh → v1.0.0-GA tag applied → release notes ACTIVE
```

Both tag operations are independent in their cryptographic content (one is on `specs/00_framework.md` v1.0.0 FROZEN; the other is on the full product corpus including `crates/` + `apps/` + `specs/`), but they share the Owner-action constraint: the same Owner signs both, and the freeze of the framework precedes the GA of the product.

The lote-6 sibling stream is expected to author the analogous `scripts/cut-framework-v1-0-0-ga-tag.sh` (or equivalent) following the pattern established here. When that stream lands, its audit doc should cross-ref §5 here and §5 of *its* doc should describe the symmetric unfreeze behaviour of its post-tag step (typically: §42 changelog entry seal + framework-v1-0-0-ga tag push; no release-notes thaw because the framework has no `RELEASE-NOTES-*.md` companion).

This stream consequently emits the following cross-ref obligation:

> When `specs/_audits/2026-05-16-lote-6-owner-signoff-prep.md` lands, that audit's §"post-cut" section MUST cross-reference this audit and note: "This script (the lote-6 cut script) unfreezes after `framework-v1-0-0-ga` is tagged; the analogous downstream unfreeze for the product corpus runs after `v1.0.0-GA` is tagged via `scripts/cut-v1-0-0-ga-tag.sh` (see `specs/_audits/2026-05-16-v1-tag-application-prep.md` §1)."

This is a forward-reference; the lote-6 stream is responsible for honouring it on its own landing.

---

## 6. Quality gates

| Gate | Command | Status | Notes |
|---|---|---|---|
| Cutover script dry-run | `bash scripts/cut-v1-0-0-ga-tag.sh --dry-run` | PASS (exit 0) | Performs § A–§ I assertions in permissive mode (allows non-main branch, dirty working tree, un-replaced placeholders); emits PLAN-prefixed lines for the four mutating commands. |
| Thaw script dry-run | `bash scripts/thaw-release-notes.sh --dry-run` | PASS (exit 0) | Detects DRAFT state; prints a 40-line preview diff (front matter doc_status DRAFT→ACTIVE, release_date TBD→2026-05-16, > **DRAFT.** banner block stripped); does NOT write the file. |
| `validate_specs.py` | `python3 scripts/validate_specs.py` | PASS (exit 0) | 446 schema-validated + 9 YAML-only = 455 total documents. This audit is in `_audits/` (SKIP_ALL). |
| `validate_references.py` | `python3 scripts/validate_references.py` | PASS (exit 0) | Zero dangling references. |
| Freeze: §3.b GA-blocker prep classification | (manual) | OK | All wave-27 changes are confined to `scripts/` + `docs/release/` + `specs/_audits/` — three non-frozen surfaces per `scripts/check-ga-freeze-allowed.py` SKIP_PATH_PATTERNS. No frozen-surface edit required. |

---

## 7. Pending Owner action (D-day handoff)

The wave-27 stream completes here. The next mutation against `v1.0.0-GA` happens on D-day under Owner authority. The handoff checklist:

1. ☐ Both 2-key DocuSign envelopes filed (Owner + on-call SRE Lead OR Pairing-Alpha/Beta dual-hat) per RB-GA-CUTOVER §8 + ADR-0034b.
2. ☐ Owner edits `docs/release/v1.0.0-GA-tag-draft-final.txt` and replaces the five `__*__` placeholder tokens with real DocuSign envelope IDs / signed-commit SHAs + timestamps + Signer 2 name. Commits this change to `main` with a "post-2-key-signoff: replace v1.0.0-GA tag-draft placeholders" message.
3. ☐ Owner runs `bash scripts/cut-v1-0-0-ga-tag.sh --expected-main-sha <new-main-tip-SHA>` (the new tip is the post-placeholder-replacement commit, not the wave-26 SEAL tip).
4. ☐ Script applies tag locally, pushes `main` (one commit ahead) + `v1.0.0-GA` tag to origin, invokes `scripts/thaw-release-notes.sh` which flips `RELEASE-NOTES-v1.0.0-GA.md` front matter to `doc_status: "ACTIVE"`.
5. ☐ Owner commits the thawed release notes ("post-GA-tag: thaw RELEASE-NOTES-v1.0.0-GA.md (DRAFT→ACTIVE)") + pushes to origin.
6. ☐ Owner writes the post-cutover attestation audit at `specs/_audits/2026-MM-DD-ga-cutover-execution-attestation.md` (referenced in the tag message body §Sign-off Method field for both signers).

Step 6 is the formal post-GA closing audit and is itself a deliverable for a future wave (likely wave-28 R-PREP / post-GA-attestation stream).

---

## 8. Caveats

1. **No `framework-v1-0-0-ga` tag check.** The cutover script does NOT verify the existence of the `framework-v1-0-0-ga` tag (sibling lote-6 prerequisite). The framework tag is the publication gate per `RELEASE-NOTES-v1.0.0-GA.md` front matter `publication_gate` field, but is not a strict precondition for *applying* the product GA tag — the two operations are crypto-independent. The Owner is responsible for sequencing them correctly (framework first, then product). A wave-28+ refinement could add an `--require-framework-tag` flag that asserts the framework tag's presence at run time; we did not add it at wave-27 to avoid coupling otherwise-independent ceremonies.

2. **`--expected-main-sha` is optional.** The flag is intentionally optional to avoid a hard-fail on Owner SHA-paste typos; passing the wrong SHA fails fast with a clear error, but omitting it falls through with an info-level warning. Production runs should always pass `--expected-main-sha`.

3. **Working-tree-clean does not exclude the tag draft.** See §4.4. The assumption is the Owner commits the placeholder-replaced tag draft to `main` before running the cutover script. A future wave can introduce a more nuanced `--allow-dirty <pathspec>` if uncommitted-but-edited tag drafts become the operational norm.

4. **Strip-banner regex is conservative.** `scripts/thaw-release-notes.sh` strips only the FIRST `> **DRAFT.** ...` banner block matched by an explicit regex anchored to the first `# CoreLink ...` heading. If the release notes are restructured (e.g., the banner moved or replaced with prose), the strip-banner pass becomes a no-op. The thaw script still flips `doc_status` and `release_date` correctly in that case; the banner residual is a cosmetic issue rather than a correctness one. Operator can pass `--no-strip-banner` to opt out.

5. **No `--rollback` flag.** The cutover script has no rollback path because annotated tag rollback at the public-remote level requires `git push --delete` which is destructive against any clones that already fetched. The recovery story is: post-bad-tag, push a `v1.0.1-GA` patch tag with a corrected message rather than try to rewrite history. Wave-28+ refinement could add a recovery audit template, but no automation is appropriate at the tag-deletion tier.

6. **Thaw script writes UTF-8 in-place.** `scripts/thaw-release-notes.sh` overwrites the release notes file with the rewritten content from a tmpfile copy. Filesystem permissions and atomicity are inherited from `cp`. If the operator runs on a network mount with non-atomic write semantics, a concurrent reader could observe an intermediate state. The release notes file is not read concurrently in the cutover flow, so this is theoretical.

7. **Validators run at baseline.** This audit's quality gates run `validate_specs.py` + `validate_references.py` at the wave-27 worktree tip, which is the wave-26 SEAL tip plus the wave-27 artefacts. Because all wave-27 changes are in `_audits/` + `scripts/` + `docs/release/` (all SKIP_ALL'd by `validate_specs.py`), the validators are mechanically identical to the wave-26 SEAL baseline.

---

## 9. SEAL declaration

This stream is wave-27 R-PREP / `wt/r-prep-v1-tag-application-prep`. It produces three artefacts (cutover script, thaw script, tag-draft-final) plus this audit, all of which exit-0 their dry-run quality gates and leave the wave-26 SEAL baseline validators green.

The stream is **SEALED for merge** at wave-27 against the merge-orchestrator's wave-27 closure pass, contingent on the following:

- `bash scripts/cut-v1-0-0-ga-tag.sh --dry-run` exits 0 with all § A–§ I pre-flight checks passed.
- `bash scripts/thaw-release-notes.sh --dry-run` exits 0 with DRAFT-state detection + diff preview emitted.
- `python3 scripts/validate_specs.py` exits 0.
- `python3 scripts/validate_references.py` exits 0.

No tag is created by this stream — that is Owner-action D-day, per the charter constraint at task dispatch.

---

**End of v1.0.0-GA tag application prep audit.**

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
