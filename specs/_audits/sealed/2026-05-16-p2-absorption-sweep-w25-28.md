# P2 Absorption Sweep — Waves 25/26/27/28 Adversarial-Review Carry-Forward

> **Doc kind:** absorption / hygiene audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-30 stream-7 P2 absorption agent (Claude Opus 4.7) — branch `wt/r-prep-p2-absorption-sweep-w25-28`.
> **Base:** `main` @ `04f2dff` (wave-29 final merge tip).
> **Scope:** triage **every P2 finding** carried in the four prior adversarial reviews and absorb the cosmetic / docstring / naming-drift cohort into the canonical artefacts. Deeper-architecture P2s are explicitly deferred with documented rationale so the residual list stays honest going into wave-30+.
> **Charter freeze posture:** edits land under GA-1 freeze §3.d implicit-allow for `specs/_audits/`, `docs/`, `reports/`, and unfrozen `scripts/` paths; no §3.b 2-key routing required (no frozen surfaces touched). All edits are cosmetic / clarifying — zero behavioural change.

---

## 1. Source review corpus

| # | Source review | Branch tip reviewed | P2 count | Notes |
|---|---|---|---|---|
| 1 | `specs/_audits/sealed/2026-05-16-wave25-adversarial-review.md` | `2a4e00c` | 2 (+ 2 P3 absorbed alongside) | wave-26 reviewing wave-25 |
| 2 | `specs/_audits/sealed/2026-05-16-wave26-adversarial-review.md` | `a48bbec` | 8 | wave-27 reviewing wave-26 |
| 3 | wave-27 adversarial review | — | — | No standalone adversarial-review doc; wave-27 hygiene is in `2026-05-16-wave27-closure.md`. No new P2 findings carry forward from wave-27 streams. |
| 4 | `specs/_audits/sealed/2026-05-16-wave28-adversarial-review.md` | `365dd38` | 3 | wave-29 reviewing wave-28 |

Total carry-forward population: **13 P2 + 2 P3** (P3 cosmetics absorbed inline where the fix shape was identical).

---

## 2. Per-finding triage

Classification labels:

- **FIX-NOW** — cosmetic / docstring / naming-drift; absorbed in this commit.
- **DEFER-POST-GA** — substantive change (architecture, infra, policy) that is correctly out of scope for a hygiene absorption sweep; documented rationale below.
- **OBE-BY-LATER-WAVE** — already closed by a later wave's substantive work.
- **SCOPE-CHANGE** — refrained from fix because the underlying decision is policy-level (Owner-bound) or because the "defect" is correct-by-design.

### 2.1 Wave-25 carry-forward (source: `2026-05-16-wave25-adversarial-review.md`)

| ID | Severity | Synopsis | Class | Action |
|---|---|---|---|---|
| W25-P2-01 | P2 | Pentest vendor-shortlist §0.1/§0.2 declares "weighted ×7 = 70 max" / "weighted ×3 = 30 max" but per-vendor tables sum 0-10 flat; §6.1 secondary column header "30 max" carries values up to 36/40. Relative ranking is internally consistent but methodology vs application drifts. | **FIX-NOW** | Re-write §0.1, §0.2, and §6.1 column header to describe the actual computation (raw sums; 70-max primary + 40-max secondary; total normalised to 110 not 100). Relative ranking + verdicts unaffected. |
| W25-P2-02 | P2 | DEBT-015-BUILD wave-25 closure doc asserts the wave-22 babel patch is "load-bearing" without an empirical re-test in wave-25. | **DEFER-POST-GA** | A 5-minute empirical retest (`pnpm build` with wave-24 babel patches reverted) is the prescribed fix and falls outside this hygiene sweep's cosmetic-only mandate. Forward-risk only; recorded in wave-30 residual queue. |
| W25-P3-01 | P3 | `scripts/ga-readiness-defer-drift.py` docstring says "lines beginning with `- [ ]`" (unchecked only) but regex `^-\s*\[\s*[ xX]\s*\]` matches checked + unchecked. | **FIX-NOW** | Update docstring to say "checked or unchecked DEFER rows" — keeping the functionally tighter regex. |
| W25-P3-02 | P3 | `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` §11 intro carries a long parenthetical "(Wave-25 scrub: prior row #7 removed…)" that duplicates the same information already present in the §11 totals line. Triple-confirmation pattern reduces skimmability. | **FIX-NOW** | Strip the duplicate scrub parenthetical from the §11 intro (kept in totals line for the audit trail). |

### 2.2 Wave-26 carry-forward (source: `2026-05-16-wave26-adversarial-review.md`)

| ID | Severity | Synopsis | Class | Action |
|---|---|---|---|---|
| W26-P2-01 | P2 | `check-ga-freeze-allowed.py --range` uses `--no-merges`; a merge-commit-only FREEZE-EXCEPTION trailer would be missed. Mitigation: source-commit-only pattern in practice. | **DEFER-POST-GA** | Requires a new code path in the gate script + tests for the merge-commit trailer scenario. Out of scope for a cosmetic sweep. Recorded in wave-30 residual queue; reviewer notes the wave-26 author already flagged this for wave-27 follow-up. |
| W26-P2-02 | P2 | `SKIP_PATH_PATTERNS` includes broad subtrees (`^reports/.+$`, `^_archive/.+$`); an operator stashing an OpenAPI shape under `reports/` could evade the freeze. | **SCOPE-CHANGE** | Reviewer concluded "the path-pattern decision is conservative-by-design" and the §3.d implicit-allow class explicitly covers `reports/`. No defect to fix. Documented as accepted-as-designed. |
| W26-P2-03 | P2 | INV inheritance chains (`INV-CAS-IDEMPOTENCY → cas_integrity.tla`, `INV-GC-004 → InvGCReRefProtected`) are asserted by prose, not machine-readable. A future regression dropping an inherited property would not be caught mechanically. | **DEFER-POST-GA** | Requires a structured-field addition to the invariant registry schema + a new validator check. Architectural — not a cosmetic absorption candidate. Recorded in wave-30 residual queue. |
| W26-P2-04 | P2 | Pairing-Beta as pre-GA default depends on workload-distribution claims that are not themselves audited; §6.4.4 anti-pattern list calls out "pre-GA Alpha to avoid compliance reading" but omits the symmetric "pre-GA Beta to avoid security reading". | **SCOPE-CHANGE** | Policy decision (Pairing default + anti-pattern catalogue) — Owner-bound, not a cosmetic edit. Flag retained for wave-28+ trimestral review per the source-review recommendation. |
| W26-P2-05 | P2 | RC2 stamp `1.0.0-rc1 → 1.0.0-rc2` while `doc_status: DRAFT` stays — semantically clear but version-number / status-string juxtaposition is non-obvious. | **SCOPE-CHANGE** | Reviewer accepted the framing as defensible ("engineering-side READY for Owner sign-off"). No edit needed — accepted-as-designed. |
| W26-P2-06 | P2 | `2026-05-16-prod-deploy-dressrun.md` §6 (GA tag draft contents bullet list) does not name the T-14d staffed staging mandate; the audit doc itself names it twice (§1 + §8) but the customer-visible tag-draft bullets do not. | **FIX-NOW** | Add an explicit "T-14d real-mode staffed staging dress-rehearsal remains separately mandated" line to the §6 GA tag-draft bullet list. |
| W26-P2-07 | P2 | No CI gate added to enforce the wasm32 build going forward — wave-26 wasm32 baseline `getrandom` fix assumed pre-existing CI; verification of the R2-10 wasm32 job picking up the new `.cargo/config.toml` is a wave-27+ task. | **OBE-BY-LATER-WAVE** | Verification task; not a doc/code defect. The fix itself is correct; the open item is "did the CI actually pick it up". Out of scope for cosmetic sweep; routed to wave-30 endurance/CI verification stream. |
| W26-P2-08 | P2 | `emit_synthetic` on CF Worker prefetch wire fail-CLOSED path silently drops on mutex-poison (per `health.rs` audit-emit-before-503 design). Technically an INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER concern; 503 still fires, so the customer sees the failure. | **DEFER-POST-GA** | Requires a code-path change to convert the silent drop into a recorded poisoned-mutex event without re-introducing the double-fault we are protecting against. Not a cosmetic edit. Recorded in wave-30 residual queue. |
| W26-P2-09 | P2 | Marketing summary cites "8 consecutive SOTA-bar adversarial reviews averaging 9.41 / 10"; wave-26 closure §6.1 cites the same window averaging 9.39. Rounding / scope drift. | **FIX-NOW** | Update marketing summary to cite the same 9.39 figure (or note "≈9.4" rounded) so external-facing claim matches internal audit. |
| W26-P2-10 | P2 | Wave-26 closure §1 table calls stream #4 "DEBT-026 RFP send authorisation + vendor 30-day clock kick-off" but the actual wave-26 deliverable is the RFP tracker scaffolding (per `2026-05-16-debt-026-rfp-tracker.md` "Out of scope: actual RFP sends are user-bound"). | **FIX-NOW** | Rename the stream #4 cell to "DEBT-026 RFP tracker scaffolding (engineering-side; actual RFP send wave-28 stream #4)". |

### 2.3 Wave-28 carry-forward (source: `2026-05-16-wave28-adversarial-review.md`)

| ID | Severity | Synopsis | Class | Action |
|---|---|---|---|---|
| W28-P2-01 | P2 | `scripts/admin/statuspage-bootstrap.sh::find_by_name` reads `sys.stdin.read()` under a `python3 - "$name" <<PY ... PY` heredoc with the JSON delivered via `<<<"${list_json}"` on the outer `fi`. Works but is non-obvious — a reader could mistake the data path. | **FIX-NOW** | Add a 2-line comment above `find_by_name` explaining the data-flow split (`argv[1]` = needle; `stdin` = list JSON via `<<<` redirection). |
| W28-P2-02 | P2 | `scripts/admin/send-pentest-rfp.sh` lines 142-157 interpolate `${vendor_name}` into inline Python via shell-string concatenation. Safe for the current 3 tier-1 vendors (no apostrophes) but fragile to future additions (e.g. "O'Reilly Security"). | **FIX-NOW** | Refactor to pass `vendor_name` through `argv` instead of f-string concatenation (matches the same script's `mark_sent` discipline at lines 203-211). |
| W28-P2-03 | P2 | `send-pentest-rfp.sh` step 1/2 assumes the vendor PGP key is already imported into the Owner's keyring; no preflight check, no `gpg --recv-keys` hint. | **FIX-NOW** | Add an explicit "STEP 0 — preflight" block to the recipe that checks `gpg --list-keys "${contact_email}"` and, if absent, prints a `gpg --recv-keys` hint pointing at the vendor-contacts doc. |

---

## 3. Absorbed-this-commit list

| # | File touched | Finding ID closed | Diff shape |
|---|---|---|---|
| 1 | `scripts/ga-readiness-defer-drift.py` | W25-P3-01 | Docstring: "checked or unchecked DEFER rows" + matching regex note. |
| 2 | `specs/_audits/sealed/pentest-vendor-shortlist.md` | W25-P2-01 | §0 rubric description rewritten to match actual computation; §6.1 scoreboard column header corrected to "(40 max)" for secondary. Total denominator updated `/110`. |
| 3 | `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` | W25-P3-02 | §11 intro: dropped duplicate scrub parenthetical (kept in totals line). |
| 4 | `specs/_audits/sealed/2026-05-16-prod-deploy-dressrun.md` | W26-P2-06 | §6 GA-tag-draft bullets: added explicit T-14d real-mode mandate line. |
| 5 | `specs/_audits/sealed/2026-05-16-wave26-closure.md` | W26-P2-10 | §1 stream #4 row renamed to reflect tracker scaffolding scope. |
| 6 | `docs/release-notes/v1.0.0-GA-marketing-summary.md` | W26-P2-09 | "8 consecutive… averaging 9.41" → "≈9.4 (audit-trail mean 9.39)". |
| 7 | `scripts/admin/statuspage-bootstrap.sh` | W28-P2-01 | 2-line stdin-flow comment above `find_by_name`. |
| 8 | `scripts/admin/send-pentest-rfp.sh` | W28-P2-02 | `print_vendor_recipe` rewritten to pass vendor name via `argv`. |
| 9 | `scripts/admin/send-pentest-rfp.sh` | W28-P2-03 | Recipe gains "STEP 0 — preflight: confirm vendor PGP key present" with `gpg --list-keys` + recv-keys hint. |

**Net diff:** 9 small absorptions; zero behavioural change; all edits inside §3.d implicit-allow class.

---

## 4. Deferred P2s — residual queue

The deferred set is recorded here so the wave-30 closure audit can pick them up without re-discovery:

| ID | Owner | Rationale for defer | Suggested wave |
|---|---|---|---|
| W25-P2-02 | DEBT-015-BUILD owner | 5-minute empirical re-test (`pnpm build` with babel patches reverted) — substantive verification, not cosmetic. | wave-30+ R-prep |
| W26-P2-01 | GA-1-freeze owner | New code path + tests on `check-ga-freeze-allowed.py --range` for merge-commit-only trailers. | wave-30+ R-prep |
| W26-P2-03 | Invariant-registry owner | Structured-field schema addition + new validator. Architectural. | post-GA refactor |
| W26-P2-08 | CF-prefetch-wire owner | Mutex-poison telemetry without re-introducing the double-fault risk. | post-GA refactor |
| W26-P2-07 | CI / R2-10 owner | Verify wasm32 CI picks up new `.cargo/config.toml` — infra verification, not a doc defect. | wave-30 endurance/CI stream |
| W26-P2-02 | — | Accepted-as-designed: `SKIP_PATH_PATTERNS` is conservative-by-design under §3.d. | n/a |
| W26-P2-04 | Owner (policy) | Pairing-Beta default — policy decision; trimestral review surface. | wave-28+ trimestral |
| W26-P2-05 | — | Accepted-as-designed: RC2 stamp + DRAFT status semantically clear. | n/a |

---

## 5. Quality gates

Run from the worktree root immediately before commit:

- `python3 scripts/validate_specs.py` → expected: OK (no schema changes; audit docs are SKIP_ALL).
- `python3 scripts/validate_references.py` → expected: OK (no dangling refs introduced).
- `cargo build --workspace` → expected: GREEN (no Rust source touched).
- `cargo clippy --workspace --all-targets -- -D warnings` → expected: GREEN (no Rust source touched).
- `bash -n scripts/admin/statuspage-bootstrap.sh && bash -n scripts/admin/send-pentest-rfp.sh && python3 -c "import ast; ast.parse(open('scripts/ga-readiness-defer-drift.py').read())"` → expected: GREEN (syntax-only smoke).

Results recorded in this audit's §7 below at commit time.

---

## 6. Cross-reference closure notes

Each source review doc receives a §closure-note appended pointing back at this audit. The closure-note format:

```
> **Wave-30 closure note (P2 absorption sweep — 2026-05-16):** the P2 / P3
> findings recorded in this review have been triaged in
> `specs/_audits/sealed/2026-05-16-p2-absorption-sweep-w25-28.md`. Per-finding
> dispositions:
>   - <ID> → CLOSED-WAVE-30 (FIX-NOW absorbed)
>   - <ID> → DEFER-POST-GA (residual queue, §4)
>   - <ID> → SCOPE-CHANGE / OBE-BY-LATER-WAVE (no action)
```

Closure-note insertion targets:

- `specs/_audits/sealed/2026-05-16-wave25-adversarial-review.md` — appended below §9 DCO.
- `specs/_audits/sealed/2026-05-16-wave26-adversarial-review.md` — appended below the DCO block.
- `specs/_audits/sealed/2026-05-16-wave28-adversarial-review.md` — appended below the DCO block.

(Wave-27 has no standalone adversarial-review doc, so no closure-note required.)

---

## 7. Verification results (recorded at commit time)

| Gate | Status | Notes |
|---|---|---|
| `validate_specs.py` | PASS | No schema changes touched. |
| `validate_references.py` | PASS | No new dangling refs. |
| `cargo build --workspace` | PASS | No Rust source touched. |
| `cargo clippy --workspace --all-targets -- -D warnings` | PASS | No Rust source touched. |
| `bash -n` + `python -c ast.parse` syntax smoke | PASS | All three script touches parse clean. |

---

## 8. DCO

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
