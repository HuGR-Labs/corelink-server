# Pilot Announcement Comms Package — Audit (2026-05-16)

> **Doc kind:** wave-28 step-7 deliverable audit (`_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-28 pilot-comms-package agent (Claude Opus 4.7) — branch `wt/r-prep-pilot-announcement-comms`.
> **Base:** `main` @ `f5ff683` ("merge wt/r-prep-shadow-sink-consumer-adoption into main (wave-27)" — wave-27 SEAL tip).
> **Scope:** author the 7 copy-paste-ready pilot-announcement comms artefacts requested by Owner for wave-28 step-7 + this audit doc.
> **Cross-ref:** `specs/_audits/sealed/2026-05-16-wave27-closure.md` (wave-27 closure — DEBT-027 ≥3 ACTIVE pilots called out as P1 GA-blocker), `docs/internal/customer-success-playbook.md` (wave-23), `marketing/launch/PRESS-RELEASE.md` (GA press; pre-existing), `marketing/launch/SOCIAL/TWITTER-THREAD.md` + `LINKEDIN-POST.md` + `HACKERNEWS-SHOW-HN.md` (GA-day social; pre-existing — this audit's artefacts are **pilot-day** counterparts; explicitly different content + different gate).

---

## 1. Charge

Owner-issued wave-28 step-7 charge: author the pilot announcement comms package as 8 deliverables:

| # | Deliverable | Path | Status |
|---|---|---|---|
| 1 | Exec-summary 1-pager | `marketing/launch/PILOT-ANNOUNCEMENT.md` | DELIVERED |
| 2 | 8-tweet thread | `marketing/launch/PILOT-TWEET-THREAD.md` | DELIVERED |
| 3 | Long-form LinkedIn post | `marketing/launch/PILOT-LINKEDIN-POST.md` | DELIVERED |
| 4 | Show-HN-style post | `marketing/launch/PILOT-HN-LAUNCH.md` | DELIVERED |
| 5 | Outbound email template (Mustache `{{lead_name}}` / `{{lead_company}}`) | `marketing/launch/PILOT-EMAIL-BLAST.md` | DELIVERED |
| 6 | Landing-page copy for `signup.corelink.humangr.com/pilot` | `marketing/launch/PILOT-LANDING-PAGE-COPY.md` | DELIVERED |
| 7 | 30-candidate direct-outreach target list | `docs/internal/pilot-target-list.md` | DELIVERED |
| 8 | This audit doc | `specs/_audits/sealed/2026-05-16-pilot-comms-package.md` | (this file) |

All 8 are present on branch `wt/r-prep-pilot-announcement-comms` at the SEAL commit.

---

## 2. Freeze posture

GA-1 feature-freeze is ACTIVE per `specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md`. This branch's changes are:

- **No `crates/` modifications.** No `apps/` modifications. No engineering surface modified.
- **No `specs/03_architecture/` modifications.** No INV registry modifications. No ADR modifications. No spec corpus modifications.
- **Marketing-only + internal-doc-only changes.**

**Freeze classification:**

- `marketing/launch/PILOT-*.md` (6 new files) → **§3.b P1-GA-blocker prep.** Direct support for DEBT-027 (≥3 ACTIVE pilots to GA, per `specs/_audits/sealed/2026-05-16-wave27-closure.md`). Without the pilot-comms package, the pilot-signup pipeline (wave-27) has no demand-generation surface and DEBT-027 cannot close.
- `docs/internal/pilot-target-list.md` (1 new file) → **§3.b P1-GA-blocker prep.** Same rationale; the target list is the operational counterpart to the pilot-comms-package and is required to actually source the ≥3 ACTIVE pilots.
- `specs/_audits/sealed/2026-05-16-pilot-comms-package.md` (this file) → **§3.d implicitly-allowed.** Audit docs are explicitly listed as freeze-permissible.

**Commit-message tokens (required by `scripts/check-ga-freeze-allowed.py` per the freeze decl):**

- Squash-merge commit subject: `marketing: add pilot-announcement comms package (wave-28 step-7)`
- Body MUST contain: `FREEZE-EXCEPTION: P1-ga-blocker` + 2-key authorisation per ADR-0034b §3 (Owner + on-call SRE). The audit doc itself (`§3.d implicitly-allowed`) is folded into the same commit; the freeze gate allows `P1-ga-blocker` to encompass adjacent `§3.d` changes per `specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md §3.d`.

---

## 3. Content design decisions (Owner-overridable)

1. **Honest pre-GA framing everywhere.** Every deliverable (1-pager, X thread, LinkedIn, HN, email, landing page) calls out explicitly what is **pilot** vs **GA-only**. The HN draft does this most aggressively (HN audience expects it); the email does this in 2 lines to keep the 150-word target; the landing page has a dedicated section. Rationale: HuGR pre-GA brand authority is contingent on never overselling.
2. **Trace to GA-GATE-CRITERIA.** The 1-pager + landing page link to GA-GATE-CRITERIA.md (in spirit — actual URL is `/ga-gate`); this ties marketing claims to a concrete, public engineering contract.
3. **Apply URL is `https://signup.corelink.humangr.com/pilot`** per Owner brief, matching wave-27 token-based slot-reservation pipeline.
4. **10 slots, ≥3 to GA.** The target-list funnel is sized so 30 candidates → 3–5 activated, leaving margin without forcing artificial scarcity.
5. **No fabricated quotes.** Every claim traces to either the public spec corpus (TLA+ isolation, Merkle audit chain, 4-region replication) or is labelled as pre-GA pilot scope (best-effort SLOs, no BYOK in pilot, no SOC 2 Type II yet).
6. **No emoji in copy.** Per Owner standing brief + observed style in `PRESS-RELEASE.md` + `LINKEDIN-POST.md` (the GA-day counterparts). Posting-note sections suggest no emoji even on X (corporate launch threads with thread-emoji read as marketing-noise to the build-infra audience).
7. **Target list omits personal contact data.** The list is `company / team / public signal / tier` only. Owner's outreach motion is "find a public-channel contact, send the email template." Explicit no-scrape posture protects against deliverability and GDPR/CCPA exposure.

---

## 4. Quality-gate checklist

| Gate | Result | Notes |
|---|---|---|
| `scripts/validate_specs.py` | GREEN (expected) | No `specs/` corpus changes that would trip front-matter / INV-coverage / orphan checks. Only `specs/_audits/` (excluded from validation per `SKIP_ALL`). |
| `scripts/validate_references.py` | GREEN (expected) | All cross-refs in this audit are to existing files (`wave27-closure.md`, `customer-success-playbook.md`, `PRESS-RELEASE.md`, freeze decl). No new spec-corpus references added. |
| Markdown lint | clean (best-effort manual pass) | Each file uses ATX headings, consistent fenced code, no broken tables, no unmatched emphasis. |
| Freeze: §3.b + §3.d | satisfied (see §2) | Commit message must carry `FREEZE-EXCEPTION: P1-ga-blocker` + 2-key per ADR-0034b §3. |
| Honest-claims discipline | satisfied (see §3.1, §3.5) | Pre-GA framing is explicit in all 6 marketing artefacts. No overclaim audit-trail entries. |
| DCO + Co-Authored-By | (commit step — Owner ratifies in merge) | Squash-merge commit MUST include `Signed-off-by:` + `Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>`. |

---

## 5. Risk register (small surface; enumerate anyway)

| ID | Risk | Severity | Mitigation |
|---|---|---|---|
| R1 | `signup.corelink.humangr.com/pilot` 404s at publication time → HN flagging risk + outbound email broken-link risk. | HIGH | **Owner pre-flight gate:** confirm landing page is live + signup form reachable BEFORE publishing X / LinkedIn / HN / sending emails. HN posting-notes section in `PILOT-HN-LAUNCH.md` explicitly calls this out. |
| R2 | Pilot demand exceeds 10 slots → Owner has no published "cohort 2" timeline. | MED | The 1-pager mentions "two cohorts" of headroom; cohort-2 timeline is an Owner-side product-marketing decision and not in scope for this comms package. Acceptable risk. |
| R3 | A pilot prospect cites GA-only capability (BYOK, SOC 2 Type II) as a deal-breaker mid-pilot. | MED | Landing-page FAQ + 1-pager + HN draft + LinkedIn post all front-load what's pilot-vs-GA. Mid-pilot deal-breakers should be screened OUT at the signup form's qualification stage. Acceptable. |
| R4 | Pilot signup token-gating (wave-27) misfires for an inbound applicant → bad first impression. | MED | wave-27 admin-scripts include token re-issuance flow. Owner-side ops risk; not addressable in comms package. |
| R5 | LinkedIn / X audience overlap creates double-impression fatigue. | LOW | Posting-notes recommend 60-minute X→LinkedIn cadence to amortise. Acceptable. |
| R6 | Pilot-target-list leak (internal doc) → competitive intel exposure. | LOW | Doc is `docs/internal/` and explicitly labelled INTERNAL with no-share + no-LLM-paste guardrails. Public-signal-only entries reduce blast radius. Acceptable. |

No HIGH risk is unmitigated; R1 is mitigated by an Owner-actioned pre-flight check that the deliverable itself describes.

---

## 6. SEAL declaration

This audit declares wave-28 step-7 (pilot-announcement comms package) **SEALED on branch `wt/r-prep-pilot-announcement-comms`** with:

- 7 deliverables present at the paths charged.
- 1 audit doc (this file).
- Freeze posture: §3.b P1-GA-blocker prep + §3.d cosmetic-doc; 2-key required on merge.
- Quality gates green-expected per §4.
- Honest-claims discipline satisfied per §3.

Owner merge is the SEAL-promotion event; this audit ratifies branch-side completion.

---

*HuGR Labs · CoreLink wave-28 step-7 · pilot-comms-package · 2026-05-16*
