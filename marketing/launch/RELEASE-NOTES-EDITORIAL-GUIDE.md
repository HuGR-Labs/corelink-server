---
id: "RELEASE-NOTES-EDITORIAL-GUIDE"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
parent: "ROADMAP-TO-GA.md §8 (R-8 Launch)"
tags:
  - "marketing"
  - "launch"
  - "release-notes"
  - "editorial-guide"
  - "r8"
  - "ga"
source_refs:
  - "scripts/generate-release-notes.py"
  - "releases/TEMPLATE.md"
  - ".github/workflows/release-notes.yml"
  - "CHANGELOG.md"
---

# Release-Notes Editorial Guide

> **Audience.** Operator (Marketing / DevRel / Engineering-on-call) who is
> turning the auto-generated `releases/RELEASE-<version>.md` into a published,
> customer-facing release note.
>
> **Rule of three.** Auto-generator drafts. Operator polishes. Final approver
> signs (`final_approver: Gustavo Schneiter`). No release note is published
> without all three.

---

## 1. Pipeline overview

```
  v* tag push                                      releases/<branch>     draft Release
        |                                                  ^                    ^
        v                                                  |                    |
  .github/workflows/release-notes.yml  ──►  scripts/generate-release-notes.py
                                                      |
                                                      |    (artifact)
                                                      v
                                          releases/RELEASE-<version>.md   ──►  editorial PR
                                                      |
                                                      |    (operator polish)
                                                      v
                                          merge to main + publish draft Release
```

Trigger surfaces:

- **Automatic.** Tag matching `v*` pushed to the default branch → CI runs
  `release-notes.yml`. The workflow opens a `releases/v<version>` branch
  with the auto-generated markdown and an editorial PR labeled
  `release-notes,customer-visible`. A **draft** GitHub Release is created
  with the markdown body — never auto-published.
- **Manual.** `workflow_dispatch` with `from_ref` + `to_ref` (operator
  regenerates after fixing a missing label or rewriting a commit).
- **Local.** `python3 scripts/generate-release-notes.py --from <ref> --to <ref> --dry-run`
  for previewing.

---

## 2. Per-section editorial responsibility

The auto-generator labels every section with its provenance. The table below
binds each section to a concrete pass.

| Section | Auto-source | Auto-OK? | Operator responsibility |
|---|---|---|---|
| **TL;DR** | `CHANGELOG.md` heading prose for matching version (first 3 lines) | No — almost always needs rewrite | Hand-write 3 customer-impact sentences. No version numbers in the prose; no internal IDs. |
| **What's new** | `feat(*)` commits + `(#NNN)` PR refs in commit messages | Yes (consolidate) | Group bullets into capability-level themes; drop duplicate variants of the same WI; keep parenthetical `sha` for traceability. |
| **Performance** | `perf(*)` commits with regex-extracted `%` deltas | Yes | Add a one-line headline if there is a category gain (e.g. "Audit-emit p99 down 25–35%"). Surface measured numbers verbatim — never round upward. |
| **Security** | `sec(*)` / `security(*)` / `fix(sec*)` commits + CVE refs | Yes | NEEDS-MANUAL for pentest closures, SOC2/ISO27001 control evidence, or coordinated disclosure timing. Cite report IDs explicitly. |
| **Compliance** | latest `specs/_audits/*debt-register*.md` rows with `CLOSED YYYY-MM-DD` inside the tag window | Yes | Map DEBT-NNN ids to customer-meaningful phrasing ("ISO27001 SoA published" not "DEBT-012 closed"). Add framework + auditor where applicable. |
| **Breaking changes** | `!:` Conventional-Commits + `BREAKING CHANGE:` footer + OpenAPI `deprecated: true` lines | No — always needs prose | For each item, write one-sentence customer impact + one-sentence why. Order by customer pain (high → low). |
| **Migration guide** | Link to `docs/customer/MIGRATION-<version>.md` if file exists | Yes (if doc exists) | NEEDS-MANUAL when a breaking change ships without a migration doc — inline the steps. |
| **Acknowledgments** | External author emails in git log (non-`@humangr.com`, non-`@anthropic.com`) | Yes | Confirm names, drop bot accounts, add consented lighthouse customers per Marketing opt-in list. |

---

## 3. Tone

**Factual. Customer-impact-first. No marketing fluff.**

Comparison reference: see the prose register of
[`marketing/launch/BLOG-POSTS/01-introducing-corelink.md`](BLOG-POSTS/01-introducing-corelink.md) —
analytic, specific, light on adjectives. Release notes are *narrower* than the
blog: only what shipped, only what the customer must know.

**Do**

- Lead with the customer outcome. "Cache hit latency p99 ... " not "Refactored ...".
- Quantify when possible. "−25–35 % audit-emit p99" beats "improved performance".
- Name external dependencies that customers act on. "Bazel ≥ 8.0", "Buck2 2026.04".
- Cite a sha or PR for every claim — that is the auditor's cross-reference.
- Surface BREAKING items at the top of their section, with a one-line "why".

**Don't**

- Don't use "we are excited to announce", "rock-solid", "world-class", "blazing-fast", "seamless", or other adjective stacking.
- Don't include internal jargon (`WI-S__-___`, `CAP-_____`, `INV-`, sprint numbers) in customer-facing prose. Those belong in `CHANGELOG.md` or `TECHNICAL-CHANGELOG.md`.
- Don't claim certification status that isn't externally confirmed (no "SOC 2 Type I attained" until the auditor letter is published).
- Don't include screenshots / GIFs / videos inline — release notes are markdown-first, mirrored to email + status page.
- Don't invent fixes. If `fix(*)` commits exist that customers cannot observe (internal-only), drop them silently.

---

## 4. Approval workflow

```
  Auto-PR opened by release-notes.yml
        ↓
  L1: Editorial pass (Marketing / DevRel)         ── 60–120 min
        ↓
  L2: Engineering accuracy review (Tech Lead)     ── 30–60 min
        ↓
  L3: Final approver sign-off (Gustavo)           ── 10–20 min
        ↓
  Merge to main + manually publish the draft GitHub Release
        ↓
  Cross-post: status page, email list (consented), CHANGELOG.md cross-link
```

**Sign-off requirements per layer:**

- **L1** confirms tone, no jargon, customer-impact prose written for every NEEDS-MANUAL section.
- **L2** confirms every claim cross-references an underlying sha / PR / debt-register row, every measured delta matches the underlying benchmark, no untrue invariant claim.
- **L3** confirms the release headline matches the GA trajectory in `ROADMAP-TO-GA.md` and `LAUNCH-CHECKLIST-V2.md` L24 (release-notes published) is unblocked.

**Hard stops.** Any of the following blocks publish:

- A security CVE is referenced but the corresponding fix is not in the tagged commit range.
- A breaking change is listed but no migration step or doc-link exists.
- An external contributor is named without consent (verify `@CONTRIBUTING.md` DCO sign-off or Marketing opt-in).
- A measured performance delta cannot be reproduced from the cited bench artifact.

---

## 5. Operator runbook (per release)

1. CI opens PR `docs(release): editorial polish for vX.Y.Z` against `main`.
2. Pull `releases/vX.Y.Z` branch locally.
3. Run `python3 scripts/generate-release-notes.py --help` to confirm tooling green.
4. Open `releases/RELEASE-X.Y.Z.md`; for each NEEDS-MANUAL section in
   `releases/TEMPLATE.md` apply the polish pass.
5. If a migration is required and the doc is missing, create
   `docs/customer/MIGRATION-X.Y.Z.md` first (the generator picks it up
   automatically on regenerate).
6. Push polished branch. Re-request review (Tech Lead, then Gustavo).
7. On merge: visit GitHub Releases → find the draft release for `vX.Y.Z`
   → paste polished markdown over the draft body → **Publish**.
8. Update [`CHANGELOG.md`](../../CHANGELOG.md) cross-link if the release adds a new dated section.
9. Update [`LAUNCH-CHECKLIST-V2.md`](LAUNCH-CHECKLIST-V2.md) row L24 (release-notes published) → tick.

---

## 6. Cross-references

- [`scripts/generate-release-notes.py`](../../scripts/generate-release-notes.py) — generator.
- [`releases/TEMPLATE.md`](../../releases/TEMPLATE.md) — polish-time skeleton.
- [`.github/workflows/release-notes.yml`](../../.github/workflows/release-notes.yml) — CI.
- [`CHANGELOG.md`](../../CHANGELOG.md) — authoritative engineering changelog.
- [`ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) §8 — R-8 launch wave.
- [`LAUNCH-CHECKLIST-V2.md`](LAUNCH-CHECKLIST-V2.md) — T-24h..T+72h moment-of-truth checklist.
- [`BLOG-POSTS/01-introducing-corelink.md`](BLOG-POSTS/01-introducing-corelink.md) — tone reference.
