---
id: "RB-DPA-CHANGE"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "s19", "dpa", "legal-review", "consent", "wi-s19-002"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §10.

# RB-DPA-CHANGE — DPA template change runbook

> **Status:** ACTIVE. Owned by WI-S19-002. Covers the Legal-review
> hook + CODEOWNERS path triggered by any modification under
> `legal/dpa/**`.

## 1. Trigger

Any PR that touches one or more of:

- `legal/dpa/v*.<locale>.md`
- the frontmatter of any DPA template (especially `notice_version`,
  `effective_date`, `legal_basis`, `legal_review_status`)

This includes typo fixes — every diff is potentially material to
"informed consent" defensibility under GDPR Art. 7 / LGPD Art. 8.

## 2. Automated checks (no human action required)

The workflow `.github/workflows/dpa-legal-review.yml` runs on every PR
that touches `legal/dpa/**` and:

1. Applies the `legal-review-required` label.
2. Posts a sticky PR comment with the per-locale checklist.

CODEOWNERS in `.github/CODEOWNERS` requires
`@humangr-labs/legal` + `@humangr-labs/privacy` review on
`/legal/dpa/`.

Branch protection on `main` enforces:

- `legal-review-required` review approval from
  `@humangr-labs/legal`.
- All required status checks green.

## 3. PR author checklist

Before requesting review, the author MUST:

- [ ] Confirm the change is intentional and material (Legal review is
      expensive — bundle typo + content fixes when possible).
- [ ] Update `notice_version` (semver) if the change is material.
      Major bump forces re-acceptance for all existing tenants per
      WI-S19-003.
- [ ] Update `effective_date` to the planned merge date.
- [ ] Leave `legal_review_status: "draft"` — Legal flips it to
      `"approved"` at sign-off.
- [ ] Apply the change to **all three** locales in lockstep, or
      explicitly call out in the PR description which locales are
      excluded (single-locale fixes are allowed for translation
      errors but require Legal sign-off for that locale only).
- [ ] Confirm `notice_text_hash` ↔ rendered text is consistent by
      running `cargo test -p corelink-dpa-acceptance` locally.

## 4. Legal Counsel checklist (per locale)

For each locale touched, the assigned Legal Counsel:

- [ ] Reviews the diff against the LGPD / GDPR / CCPA control set
      referenced in `specs/03_architecture/canonical/privacy_model.md`.
- [ ] Confirms native-speaker review for non-English locales
      (separate native-speaker review report is acceptable; reuse
      across PRs is fine if the diff is purely structural).
- [ ] Adds a comment `LGTM — Legal — <locale>` on the PR.
- [ ] Advances the frontmatter `legal_review_status` to `approved`.
- [ ] Commits a per-version audit report at
      `specs/_audits/YYYY-MM-DD-legal-review-dpa-vX.Y.Z.md` with
      per-locale sign-offs (template inherits from
      `legal/quarterly-legal-review-template.md`).

## 5. Emergency-fix path

Genuine typo fixes that **cannot** change the legal meaning:

- The PR description MUST include the literal string
  `[dpa-typo-only]`.
- Legal sign-off is still required but may be expedited to a single
  reviewer (Legal Counsel of record) within 24h.
- The audit report still needs to be committed; reuse the previous
  version's audit and append a "typo-only addendum" section.

Anything else — including "small wording changes" — follows the full
path in §4.

## 6. Post-merge

- The `dpa-legal-review` workflow re-runs on `main` to attach the
  audit URL to the merged PR comment (informational).
- `corelink-dpa-acceptance` reloads the notice registry from the
  updated `legal/dpa/v*.<locale>.md` artefacts on the next deploy;
  no D1 migration required for content-only changes.
- If `notice_version` major-bumped: trigger the re-acceptance
  broadcast pipeline (WI-S19-003 owns; not part of this WI).

## 7. Audit artefacts

| Artefact | Path |
|---|---|
| Workflow | `.github/workflows/dpa-legal-review.yml` |
| CODEOWNERS rule | `.github/CODEOWNERS` (`/legal/dpa/` row) |
| Audit report (per version) | `specs/_audits/YYYY-MM-DD-legal-review-dpa-vX.Y.Z.md` |
| WI | `specs/04_sprints/S19/work_items/WI-S19-002-*.md` |
| Crate | `crates/corelink-dpa-acceptance/` |
| D1 mirror | `migrations/d1/0038_dpa_acceptances.sql` |

## 8. Related runbooks

- `RB-POSTMORTEM-PROCESS.md` — if a material legal change ships
  without sign-off, treat as SEV-2 + post-mortem.
- `RB-CHAOS-CATALOG.md` — DPA capture is exercised by the consent
  chaos suite.
