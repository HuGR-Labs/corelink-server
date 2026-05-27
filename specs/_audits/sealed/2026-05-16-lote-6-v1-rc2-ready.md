# Lote 6 v1.0.0-rc2 readiness summary — 2026-05-16 (wave-26)

> **Doc kind:** wave-closure readiness summary (companion to `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md`; no canonical front matter required — `_audits/` excluded from `validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-26 Lote 6 RC2 readiness agent (Claude Opus 4.7) — branch `wt/r-prep-lote-6-v1-rc2-ready`.
> **Base:** `main` @ `2a4e00c` ("merge wt/r-prep-tenant-config-cf-prod-wire into main (wave-25)" — wave-25 SEAL tip).
> **Mandate:** wave-26 conditional close of Lote 6 framework v1.0.0 GA — flip the wave-18 audit verdict from `DEFER-with-engineering-side-COMPLETE` (wave-24) to `DEFER-with-Owner-sign-off-only-remaining` (wave-26) by pre-authoring every artifact the Owner needs at FROZEN-cut time, including the §42 change-log entry shells and the §43.1 placeholder convention.
>
> **Cross-ref:** `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md` (predecessor; §11.8 wave-26 closure lands in v1.4.0 of that audit), `specs/_proposals/2026-05-16-framework-reviewer-roles.md`, `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md`, `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md`, `specs/00_framework.md` (RC1 → RC2 bump in this commit).

---

## 1. What landed in wave-26 Lote 6

Three coordinated edits to `specs/00_framework.md` + one new entry in the Lote 6 audit + this readiness summary:

| Artifact | Path | Wave-26 change |
|---|---|---|
| Framework version stamp | `specs/00_framework.md` frontmatter | `version: "1.0.0-rc1" → "1.0.0-rc2"`; `updated: "2026-04-24" → "2026-05-16"`. `doc_status: "DRAFT"` deliberately unchanged (FROZEN only flips at the v1.0.0 GA cut commit, gated on Owner action). |
| Framework header readiness blockquote | `specs/00_framework.md` § (top of doc, after frontmatter) | New "Wave-26 RC2 readiness note" blockquote summarising the wave-20 / wave-22 / wave-24 / wave-26 absorption arc and pointing at this readiness doc + the audit §11.8. Existing wave-24 GA-readiness unlock note preserved verbatim. |
| §43.1 placeholder convention | `specs/00_framework.md` §43.1 | Each FW-H-* slot now labeled with one of two placeholders that the Owner replaces at FROZEN cut: `PROPOSED-OWNER-DUAL-HAT` (Option C / ADR-0034b dual-hat path) or `(a nomear)` (Option A / 4-distinct path). Pairing-Alpha vs Pairing-Beta seat assignments documented inline per slot. The original "Revisor de Produto" framing is re-scoped to FW-H-4 Production Operations Lead per proposal §8 OQ-2. |
| §42 change-log template entry | `specs/00_framework.md` §42 | New row at the table head labeled `1.0.0` / `YYYY-MM-DD` / `TEMPLATE — v1.0.0 GA cut entry, pre-authored, awaiting Owner` containing both **Shape A** (Option A) and **Shape C** (Option C / ADR-0034b dual-hat) field-level shells. Owner deletes the unused shape and fills the remaining shape's placeholder fields at FROZEN cut. New RC2 row immediately below records the wave-26 readiness bump itself (no promotion). |
| Lote 6 audit §11.8 wave-26 closure | `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md` v1.3.0 → v1.4.0 | Adds §11.8 documenting the wave-26 absorption + verdict-text narrowing from `DEFER-with-engineering-side-COMPLETE` to `DEFER-with-Owner-sign-off-only-remaining`. §6 DEFER verdict itself unchanged. Adds `wave-24` + `wave-26` to tags. |
| This readiness summary | `specs/_audits/sealed/2026-05-16-lote-6-v1-rc2-ready.md` | Single-page consolidated summary the Owner reads alongside the audit §11.8 to execute the ~30-min sign-off. |

---

## 2. Why no promotion (RC2, not v1.0.0)

The framework's own §7 lifecycle rule is binding on the framework itself per PRINC-007 / PRINC-008 / PRINC-015:

> §7.5: `REVIEW → FROZEN` requires `Aprovador Final + ≥ N revisores requeridos ✅`.

That rule cannot be satisfied by agent action — reviewer signatures and the FROZEN flip are Owner-bound. Promoting the framework while violating its own promotion rule would be a self-referential gambiarra (the framework's first rule that it applies to itself would fail immediately and irreversibly). The wave-18 audit §5.1 and §6.1 / §6.2 establish this floor; wave-26 honors it.

Wave-26 RC2 therefore stops at the **last operation an agent can validly perform**: pre-author every field the Owner will fill, so the Owner's residual action is mechanical. The promotion itself — `doc_status: DRAFT → FROZEN`, `version: 1.0.0-rc2 → 1.0.0`, §43.1 signatures filled, §42 template entry filled in, tag `framework-v1-0-0-ga` — is a separate Owner-bound commit per the §6.3 Path A / Path C selection.

---

## 3. Owner-action surface (~30 min)

| Step | Action | Owner-bound? | Time estimate |
|---|---|---|---|
| 1 | Read `specs/_proposals/2026-05-16-framework-reviewer-roles.md` (wave-20 proposal — role definitions for FW-H-1..4) | yes | ~10 min |
| 2 | Read `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md` (wave-22 addendum — operating policy: dual-hat, quorum, SLA, training-pack budget, 90-day cadence, COI declaration; wave-25 RACI detail in §6) | yes | ~10 min |
| 3 | Read `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` (wave-24 ADR — small-org dual-hat authorisation: Eligibility / Pairings / Forbidden pairings / Cross-veto / Quorum / Sign-off mechanics / Auto-expiration) | yes | ~7 min |
| 4 | Read `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md §11.8` (wave-26 closure entry) + this readiness doc | yes | ~3 min |
| 5 | **Decide:** Option A (hire 4 distinct senior reviewers) OR Option C (invoke ADR-0034b Pairing-Alpha or Pairing-Beta) | yes (irreducible) | depends |
| 6 | Edit `specs/00_framework.md`: replace `(a nomear)` / `PROPOSED-OWNER-DUAL-HAT` placeholders in §43.1 with `<name> — <YYYY-MM-DD> — sha:<7-char>` (4 reviewer lines + Final Approver line); delete the unused Shape (A or C) from the §42 TEMPLATE row; fill the remaining Shape's placeholder fields | yes | ~5 min if Option C / depends on hiring if Option A |
| 7 | Flip `doc_status: "DRAFT" → "FROZEN"` and `version: "1.0.0-rc2" → "1.0.0"` and `updated: "2026-05-16" → "<YYYY-MM-DD>"` in the framework frontmatter | yes | ~30 sec |
| 8 | Commit with DCO sign-off + Co-Authored-By line for each reviewer present | yes | ~1 min |
| 9 | Tag `framework-v1-0-0-ga` and write the promotion audit at `specs/_audits/2026-MM-DD-framework-v1.0.0-promotion.md` | yes | ~5 min |

**Total non-decision time: ~40 min** (10+10+7+3+5+0.5+1+5). Decision time (step 5) depends on whether the Owner has already discussed Option A hiring with candidates or is willing to invoke Option C today.

---

## 4. Two FROZEN-cut shapes — what the Owner picks between

### 4.1 Shape A — Option A (4 distinct staffing)

Pre-condition: 4 named senior ICs (or equivalent retainers) have completed §3 acceptance deliverables (4 distinct comments docs at `specs/_audits/2026-MM-DD-fw-h-{1,2,3,4}-acceptance-comments.md`) per the addendum §4 Training Pack (10-hour floor each).

§43.1 fills with 4 distinct names. §42 template Shape A fills with 4 distinct names + dates + SHAs. `authorizing_adr: <none — Option A>`. No Owner conflict disclosure required.

Cost envelope: ~USD 1.2–1.8M/yr fully-loaded per ADR-0034b §Alternative 1, or per-engagement retainers if external advisors. Time to onboard: ~6–8 weeks minimum per nominee per ADR-0034b §Alternative 1.

**Use when:** the program has 12+ engineers OR has been contractually required to demonstrate SOC2 / ISO27001 separation-of-duty.

### 4.2 Shape C — Option C (ADR-0034b dual-hat fallback)

Pre-condition: all 5 of ADR-0034b §Eligibility conditions hold:
1. Org headcount < 12 engineers (hysteresis ceiling)
2. SOC2 / ISO27001 separation-of-duty not yet contractually required
3. ≥ 2 external advisors retained (per `_governance/reviewer_staffing_strategy.md §7.2`)
4. Owner self-attests competence in the two dual-hatted slots
5. ADR-0034b cited in the §42 entry

Two permitted pairings:

| Pairing | Owner takes | External advisors fill | When |
|---|---|---|---|
| **Pairing-Alpha** (recommended) | FW-H-1 Architecture + FW-H-3 Security | FW-H-2 Compliance/Privacy + FW-H-4 Production Ops | Founder authored architecture + threat model; externals attest regulatory + production-ops reality |
| **Pairing-Beta** | FW-H-2 Compliance/Privacy + FW-H-4 Production Ops | FW-H-1 Architecture + FW-H-3 Security | Founder stronger on compliance/ops than architecture/security |

§43.1 fills with the Owner's name on BOTH of their slots + 2 external advisor names on the other 2 slots + Owner as Final Approver. §42 template Shape C fills with `authorizing_adr: ADR-0034b` + `pairing: alpha | beta` + `owner_conflict_disclosed: dual-hat under ADR-0034b Pairing-<...>; Final Approver structural COI per ADR-0034b §Forbidden pairings rationale + addendum §7.4`.

Auto-expiration triggers (monitored at trimestral review per addendum §5.2): headcount ≥ 10 / SOC2 kickoff / 18-month ceiling / Owner role transition.

**Use when:** the program is at present-state small-org configuration (solo Owner + AI labor + 1–2 retained external advisors) AND no auto-expiration trigger has fired.

### 4.3 What's identical between Shape A and Shape C

- Final Approver is Gustavo Schneiter in both.
- 4 distinct §3 acceptance deliverable comments docs in both (under Shape C the Owner authors 2 of the 4 per ADR-0034b §Sign-off mechanics — PRINC-005 rastreabilidade preserved).
- Tag `framework-v1-0-0-ga` in both.
- `doc_status: FROZEN` + `version: 1.0.0` + §7 lifecycle semantics in both.
- The framework's own §41 evolution policy activates post-promotion in both.

---

## 5. Quality gates run pre-commit

- `python3 scripts/validate_specs.py` — green pre-edit, green post-edit (this commit changes only `00_framework.md` + one `_audits/` doc + adds one `_audits/` doc; `_audits/` excluded per `SKIP_ALL`; `00_framework.md` schema-bound and unchanged at the schema level since only prose + version stamp moved).
- `python3 scripts/validate_references.py` — green pre-edit, green post-edit (no new dangling references; the proposal / addendum / ADR cross-refs already existed in §43.1 from wave-20 / wave-24).

The post-edit verification is reported in the agent summary block at commit-time. Wave-26 quality-gate failure would block the commit per charter "synchronous bash only, never run_in_background"; on failure the agent SEALs as `BLOCKED` rather than commit a regression.

---

## 6. Verdict — wave-26

**Lote 6 framework v1.0.0 GA engineering side: CLOSED.** Wave-26 RC2 readiness lands the last agent-actionable preparation; the remaining work is exclusively Owner-bound per the framework's own §7 / §43.1 rule.

**Wave-18 audit verdict (`specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md §6.1`): DEFER — UNCHANGED.** Verdict text narrows to `DEFER-with-Owner-sign-off-only-remaining`. §6.3 Path C ("DEFER and continue") remains the active state until the Owner acts; Path A / Path C selection at Owner discretion.

**Pending user action (~30 min):** read the four cross-ref docs + this summary + the audit §11.8, decide Option A or Option C, fill the §42 TEMPLATE entry + §43.1 placeholders, commit, tag `framework-v1-0-0-ga`.

---

## 7. References

- `specs/00_framework.md` — RC1 → **RC2** in this commit (version stamp + §43.1 placeholder convention + §42 TEMPLATE entry + wave-26 readiness blockquote)
- `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md` v1.4.0 — predecessor audit (§11.8 wave-26 closure entry; verdict text narrows to `DEFER-with-Owner-sign-off-only-remaining`)
- `specs/_proposals/2026-05-16-framework-reviewer-roles.md` v0.1.0 — wave-20 proposal (FW-H-1..4 role profiles)
- `specs/_proposals/2026-05-16-framework-reviewer-roles-addendum.md` v0.1.2 — wave-22 + wave-25 addendum (dual-hat / quorum / SLA / training pack / 90-day cadence / COI / RACI detail)
- `specs/03_architecture/adrs/ADR-0034b-framework-reviewer-dual-hat-fallback.md` v0.1.1 — wave-24 + wave-25 ADR (small-org dual-hat authorisation)
- `specs/_audits/sealed/2026-05-15-ga-readiness-consolidation-wave-13-17.md` — wave-17 verdict READY-WITH-WAIVERS baseline (unaffected by this readiness flip)
- `specs/_governance/reviewer_staffing_strategy.md §7.2` — Founder-only-sign-off prior art
- `ROADMAP-TO-GA.md` — R-3..R-4 boundary snapshot (framework freeze gate is independent of the GA tag gate per audit §6.4)
