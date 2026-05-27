# Pre-Cutover Weekly Verification Framework — 2026-05-16 (wave-28 step-10)

> **Doc kind:** wave-28 step-10 framework audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-28 step-10 agent (Claude Opus 4.7) — branch `wt/r-prep-pre-cutover-weekly-verify`.
> **Base:** `main` @ `f5ff683` ("merge wt/r-prep-shadow-sink-consumer-adoption into main (wave-27)" — wave-27 SEAL tip).
> **Scope:** stand up a weekly cron that — between now and the GA D-day cutover — re-validates the canonical 8 GA-readiness DEFER items, re-runs the cheap subset of validators + cargo tests, and emits a markdown digest the Owner can scan in ≤ 90 s without re-running the full wave-25/26/27 audit chain.
>
> **Companion artifacts:**
> - `scripts/pre-cutover-weekly-verify.sh` — orchestrator.
> - `scripts/pre-cutover-state-diff.py` — per-item state delta over the last two digests.
> - `.github/workflows/pre-cutover-weekly-cron.yml` — Monday-09:00-Bahia cron + workflow_dispatch + push trigger.
> - `reports/pre-cutover-weekly/2026-05-16-digest.md` — first-run dry-run digest (this audit's evidence row).
>
> **Cross-ref:**
> - `specs/_audits/2026-05-16-ga-readiness-final.md` (wave-24 final audit; §11 DEFER counter source).
> - `specs/_audits/2026-05-16-final-cutover-readiness.md` (wave-27 closure; §1 enumerates the canonical 8 DEFER items).
> - `specs/_audits/2026-05-16-ga-final-checklist.md` (operator boolean checklist; §G mirror).
> - `specs/_audits/2026-05-16-pre-cutover-state-snapshot.md` (wave-27 metric snapshot; §L locks the count at 8).
> - `specs/_audits/2026-05-16-cutover-dependency-map.md` (wave-25 dependency map; the cron's regression signal feeds the dependency-map's slack budget).
> - `scripts/ga-readiness-defer-drift.py` (wave-25 detector; the verify script re-runs this every week).
> - `scripts/check-ga-freeze-allowed.py` (wave-26 freeze gate; the verify script re-runs `--self-test`).
> - `specs/_runbooks/RB-GA-CUTOVER.md` (the runbook whose §0 checklist this digest helps unblock).
> - `specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md` (the cadence-twin: that one closes SOC 2 CC4.2; this one closes the GA-readiness DEFER counter visibility gap).

---

## §1. Motivation

Wave-25 / wave-26 / wave-27 shipped the GA-readiness scaffolding:
- wave-25 §4 DEFER scrub + drift detector locked the §11 counter at the canonical 8 items (5 user-bound + 3 vendor-bound).
- wave-25 final audit + wave-26 dependency map + wave-26 dress-run + wave-27 state snapshot + wave-27 final-cutover-readiness consolidation gave the Owner a one-shot signoff package at `f5ff683`.
- **But the Owner now needs *ongoing* visibility:** "is the GO verdict still GO seven days from now?" without re-running the entire wave-audit chain weekly.

This cron-driven verification fills that gap. Every Monday 09:00 Bahia (UTC-3) the workflow:

1. Probes the canonical 8 DEFER items' source-of-truth artifacts and projects each to a state token + readiness class + last-touched timestamp.
2. Re-runs the cheap validator subset (`validate_specs.py`, `validate_references.py`, `validate_canonical_consistency.py`, `ga-readiness-defer-drift.py`, `check-ga-freeze-allowed.py --self-test`).
3. Re-runs the cheap cargo subset (`cargo check --workspace`, `cargo test -p corelink-audit-chain --lib`, `cargo test -p corelink-server --test audit_export`) — under live mode only; the dry-run / push-trigger / `--no-tests` branches skip this.
4. Emits a markdown digest at `reports/pre-cutover-weekly/<YYYY-MM-DD>-digest.md`.
5. Computes a regression signal by comparing the new digest against the immediately-prior one (per-item state ordinal; any backwards transition trips a regression).
6. On regression: exits the verify step with `rc=1`, opens an auto-PR with the `pre-cutover-regression` label, and triggers a PagerDuty event (severity `warning`, group `corelink-ga-cutover`).
7. Otherwise: opens an auto-PR with the `pre-cutover` label and exits 0.

The cron is **explicitly retired** at the post-cutover thaw step (`RB-POST-GA-CONTINUITY.md` §5.5 Owner thaw declaration) — once the product GA tag is cut and the thaw conditions of `specs/_audits/2026-05-16-ga-1-feature-freeze.md §6` are met, this workflow is either removed or rewritten as the long-running post-GA continuity monitor.

## §2. The canonical 8 DEFER items

The script orders them exactly as `specs/_audits/2026-05-16-final-cutover-readiness.md §1` does:

| # | Item | Source artifact | Probe strategy | Readiness class on success | D-day ETA |
|---|---|---|---|---|---|
| 1 | LFPDPPP MX attorney sign-off (DEBT-025) | `specs/_audits/2026-05-16-lfpdppp-mx-legal-review-package.md` + DEBT register change-log row | grep canonical change-log row matching `^\|\s*YYYY-MM-DD\s*\|\s*v[0-9.]+\s+—\s+\*\*DEBT-025 (CLOSED\|RESOLVED)` OR `reports/lfpdppp-mx-closure.json` presence | `LEGAL_BOUND` → `SIGNED` | wave-26 absorption |
| 2 | FW-H-1..4 role nominations | `specs/_governance/fw-h-nominations.md` (optional; if absent: `NOT_STARTED`) | grep `FW-H-[1-4].*\b(named\|nominated\|appointed)\b` | `OPERATOR_BOUND` → `SIGNED` | Pre-GA-Gate |
| 3 | External pentest vendor SOW countersign (DEBT-026) | `reports/pentest-rfp-tracker.json` | jq aggregate over `vendors[*].state` (NOT_CONTACTED → RFP_SENT → RESPONDED → SOW_DRAFT → SOW_COUNTERSIGNED) | `VENDOR_BOUND` → `SIGNED` | T-28d → T-0 |
| 4 | DEBT-003 AWS Artifact PDF + sha256 | `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md` | grep `TBD-on-receipt` (present = `DRAFT_READY`; absent = `CLOSED`) | `OPERATOR_BOUND` → `SIGNED` | Pre-GA-Gate (T+30d) |
| 5 | DEBT-016 Statuspage `status.corelink.humangr.com` go-live | `specs/_audits/2026-05-16-debt-016-statuspage-urls.md` + (advisory) `reports/statuspage-live-state.json` from the workflow's live probe | dress-run snapshot detection + live-state lookup | `ENGINEERING_CLOSED_OPERATOR_BOUND` → `SIGNED` | T-7d |
| 6 | Pilot signups ≥ 5 design-partners (G4) | `specs/_audits/2026-05-16-pilot-onboarding-e2e.md` + (optional) `reports/pilot-tenant-state.json` | jq count of `tenants[*].attestation_signed == true` ≥ 5 | `OPERATOR_BOUND` → `SIGNED` | Pre-T-24h (G4 snapshot) |
| 7 | Pentest retest letter zero HIGH/CRITICAL (DEBT-026 final gate) | `reports/pentest-rfp-tracker.json` | jq `.retest_letter_received == true` (else NOT_STARTED / IN_FLIGHT if `retest_delivery_date != null`) | `VENDOR_BOUND` → `SIGNED` | Earliest 2026-07-29 |
| 8 | Owner sign-off (ADR-0034b 2-key) | `specs/_audits/2026-05-16-final-cutover-readiness.md` §10 signature block | grep populated `^**Signature:** ...` line | `OPERATOR_BOUND` → `SIGNED` | T-0h |

**Why this enumeration is canonical.** It mirrors the 8-row §1 table of `specs/_audits/2026-05-16-final-cutover-readiness.md` (wave-27 consolidation) and the §L locked-count assertion of `specs/_audits/2026-05-16-pre-cutover-state-snapshot.md`. The wave-25 `ga-readiness-defer-drift.py` detector (scoped against `specs/_audits/2026-05-16-ga-readiness-final.md §11` — which is the wave-24 audit that the wave-25 scrub edited down 8→7 before the wave-26/27 re-count restored 8) is **re-run** by this script every week to keep that surface honest.

## §3. State ordinal scheme

The verify script + state-diff Python use a strict total order:

```
UNKNOWN(0) < NOT_STARTED(1) == NOT_CONTACTED(1) < IN_FLIGHT(2)
    < DRAFT_READY(3) < VENDOR_SELECTED(4) < SIGNED(5) < CLOSED(6)
```

A digest is **regression-free** iff every item is `>=` its prior-digest ordinal. The diff script labels per-item transitions as `PROGRESS` / `UNCHANGED` / `RELABEL` (ordinal unchanged but label differs, e.g. NOT_STARTED ↔ NOT_CONTACTED) / `REGRESSION` (ordinal dropped).

## §4. Cron timing rationale

`cron: '0 12 * * 1'` — Monday 12:00 UTC = Monday 09:00 America/Bahia (UTC-3, no DST).

- Monday morning Bahia: Owner's primary review window (per `feedback_style` memory; matches the weekly `compliance-weekly.yml` cadence, which is Monday 09:00 UTC — a separate cron 3 h earlier; this delay leaves the compliance digest available as input).
- After the SOC 2 / Drata 03:00 UTC sync windows.
- After overnight cargo-nightly CI cycles complete.
- Before the EU-West and US-East working days start, leaving room for follow-up triage.

## §5. Quality gates

| Gate | Command | Result |
|---|---|---|
| `bash scripts/pre-cutover-weekly-verify.sh --dry-run` exits 0 | dry-run on `f5ff683` | exit 0 (NO REGRESSION); digest written at `reports/pre-cutover-weekly/2026-05-16-digest.md` |
| `actionlint .github/workflows/pre-cutover-weekly-cron.yml` | actionlint v1.7.12 | exit 0 (clean) |
| `python3 scripts/validate_specs.py` | full corpus | exit 0 |
| `python3 scripts/validate_references.py` | full corpus | exit 0 |
| `python3 scripts/validate_canonical_consistency.py` | full corpus | exit 0 |
| `python3 scripts/ga-readiness-defer-drift.py` | drift detector | exit 0 |
| `python3 scripts/check-ga-freeze-allowed.py --self-test` | freeze gate self-test | exit 0 |
| `python3 scripts/pre-cutover-state-diff.py` smoke (synthetic regression) | synthetic SIGNED→DRAFT_READY input | exit 1 (regression detected) |
| `bash -n scripts/pre-cutover-weekly-verify.sh` | bash syntax | OK |
| `python3 -c "import ast; ast.parse(open('scripts/pre-cutover-state-diff.py').read())"` | python syntax | OK |

## §6. First-run dry-run digest (2026-05-16)

Saved to `reports/pre-cutover-weekly/2026-05-16-digest.md`. Per-item state at base `f5ff683`:

| # | Item | State | Readiness class |
|---|---|---|---|
| 1 | LFPDPPP MX attorney sign-off (DEBT-025) | `DRAFT_READY` | `LEGAL_BOUND` |
| 2 | FW-H-1..4 role nominations | `NOT_STARTED` | `OPERATOR_BOUND` |
| 3 | External pentest vendor SOW countersign (DEBT-026) | `NOT_CONTACTED` | `VENDOR_BOUND` |
| 4 | DEBT-003 AWS Artifact PDF + sha256 | `DRAFT_READY` | `OPERATOR_BOUND` |
| 5 | DEBT-016 Statuspage go-live | `DRAFT_READY` | `ENGINEERING_CLOSED_OPERATOR_BOUND` |
| 6 | Pilot signups ≥ 5 design-partners (G4) | `IN_FLIGHT` | `OPERATOR_BOUND` |
| 7 | Pentest retest letter zero HIGH/CRITICAL | `NOT_STARTED` | `VENDOR_BOUND` |
| 8 | Owner sign-off (ADR-0034b 2-key) | `NOT_STARTED` | `OPERATOR_BOUND` |

Aggregate: 1 LEGAL_BOUND · 4 OPERATOR_BOUND · 1 ENGINEERING_CLOSED_OPERATOR_BOUND · 2 VENDOR_BOUND · 0 SIGNED · 0 UNKNOWN. This matches the canonical wave-27 snapshot (`specs/_audits/2026-05-16-pre-cutover-state-snapshot.md` §L = 8 = 5 user-bound + 3 vendor-bound) once `LEGAL_BOUND` is rolled up under "user-bound" and `ENGINEERING_CLOSED_OPERATOR_BOUND` is rolled up under "user-bound (ops)".

## §7. PII / secrets hygiene

- The digest emits **no raw tenant_ids.** Probe 6 (pilot signups) reads `reports/pilot-tenant-state.json` only for an aggregate count — no tenant identifier is written into the markdown. If a future probe enrichment ever needs a per-tenant breakdown, the `pseudonymize()` helper at the top of `scripts/pre-cutover-weekly-verify.sh` returns a BLAKE3 16-hex-char prefix (SHA-256 fallback when `b3sum` is not available), and the digest must reference only the pseudonymized form.
- Secret allowlist is **2 secrets**, both optional:
  - `STATUSPAGE_API_KEY` — advisory probe of `api.statuspage.io/v1/pages` to write `reports/statuspage-live-state.json` (consumed by probe 5 on subsequent runs).
  - `PAGERDUTY_TOKEN` — regression-only PD event into the `corelink-ga-cutover` service.
- Both secrets degrade gracefully (`::warning::` log + skip) when not configured, so the cron is operable in the canonical-CI-is-local stance (`feedback_ci_local`) without GHA secret provisioning.
- No third-party action is used beyond `actions/checkout`, `actions/setup-python`, `actions/upload-artifact`, `dtolnay/rust-toolchain` — all SHA-pinned in the workflow.

## §8. Regression handling

If the digest detects a regression (per-item ordinal drop OR a previously-green validator now red OR a previously-green cargo test now red):

1. **Exit code 1** from `scripts/pre-cutover-weekly-verify.sh` and `scripts/pre-cutover-state-diff.py`.
2. **GitHub step summary** shows `regression: true`.
3. **Auto-PR label** `pre-cutover-regression` (in addition to base `pre-cutover` label).
4. **PagerDuty event** (severity `warning`, group `corelink-ga-cutover`, component `pre-cutover-weekly-verify`, dedup-key `pre-cutover-weekly-<digest-date>`) — only when `PAGERDUTY_TOKEN` is configured; warning-severity, not page-severity, so it lands on the SRE shift queue without disturbing on-call.
5. **Triage runbook:** the digest §6 watch-list + the audit doc §2 source table tell the SRE which probe to follow.

## §9. Forward-looking work

- **Wave-28 step-11..N:** if a probe surfaces a closure (e.g. DEBT-025 → CLOSED) but the canonical doc was not edited to reflect it, the script flags it on the watch-list rather than auto-amending — the closure must be sealed in `specs/_audits/2026-05-15-debt-register.md` change-log first.
- **Post-GA:** at the freeze thaw window, replace this cron with `RB-POST-GA-CONTINUITY.md` §3 continuity monitor; the post-GA monitor watches T+0..T+30d gates (SLO sustain, no SEV-0/SEV-1, pilot-to-GA conversion, Owner thaw declaration), not the pre-GA DEFER counter.
- **Diff script enrichments:** the current diff script summarises ordinals; a future enrichment may flag *expected* vs *actual* progress velocity (e.g. "item 3 has been DRAFT_READY for 3 consecutive digests; SOW countersign clock at risk of slipping past T-7d").

## §10. Sealed artifacts (this audit)

- `scripts/pre-cutover-weekly-verify.sh` (≈ 380 LOC bash; synchronous; pure stdlib + `git` + optional `jq` + optional `b3sum`).
- `scripts/pre-cutover-state-diff.py` (≈ 180 LOC python; pure stdlib).
- `.github/workflows/pre-cutover-weekly-cron.yml` (Monday 09:00 Bahia cron + workflow_dispatch + push-to-main; 4 SHA-pinned actions; 2 optional secrets).
- `reports/pre-cutover-weekly/2026-05-16-digest.md` (first-run dry-run output).
- `specs/_audits/2026-05-16-pre-cutover-weekly-verify.md` (this audit).

## §11. Snapshot record

- **Branch:** `wt/r-prep-pre-cutover-weekly-verify`
- **Base commit:** `f5ff683` (wave-27 SEAL tip)
- **Audit date:** 2026-05-16
- **Author:** Claude Opus 4.7 (wave-28 step-10 agent)
- **Sign-off:** Gustavo Schneiter (async at next review)
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>
