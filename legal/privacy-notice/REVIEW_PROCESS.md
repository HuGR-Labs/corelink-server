# Privacy Notice Review Process SOP

**WI:** WI-S11-004
**Version:** 1.0.0

## 1. Roles

| Role | Responsibility |
|---|---|
| Privacy Officer (HuGR) | Drafts notice content; approves material vs minor classification per ADR-S11-007; signs final |
| Native Speaker Reviewer (PT-BR) | Reviews pt-BR.md for linguistic correctness + legal term accuracy |
| Native Speaker Reviewer (EN-US) | Reviews en-US.md |
| Native Speaker Reviewer (ES-MX) | Reviews es-MX.md |
| BR Attorney | Legal review of pt-BR.md (LGPD compliance); signs EVT-044 PDF |
| EU Attorney | Legal review of en-US.md (GDPR/CCPA compliance); signs EVT-044 PDF |
| MX Attorney | Legal review of es-MX.md (LFPDPPP ARCO compliance); signs EVT-044 PDF |

## 2. Process Steps

1. Privacy Officer drafts new notice version on git branch `notice/v{M.m.0}`.
2. **Native speaker review (parallel, ≤ 7 days target):**
   - Each reviewer reviews their locale file.
   - On approval: checkbox `native_speaker_reviewed: true` set in `metadata.yaml`.
3. **Legal local review (parallel, ≤ 14 days target):**
   - Each attorney reviews their locale.
   - On approval: attorney signs PDF (EVT-044) uploaded to R2 `evidence-legal/v{M.m.0}-{locale}.pdf`.
   - Path recorded in `metadata.yaml.locales.{locale}.legal_review_evt-044_path`.
4. **CI hook validation (pre-merge):** `scripts/validate_privacy_notice.py` checks:
   - (a) Semver bump valid.
   - (b) 3 locales sync (all 3 present).
   - (c) Legal Review EVT-044 PDFs exist in R2 (size > 0).
   - (d) `notice_text_hash` deterministic per locale.
   - (e) Major bump → CD flag for WI-S11-003 `stale_consent_check`.
5. **PR merge:** only after all CI checks green.
6. **CD pipeline:** deploy `/privacy/v{M.m.0}/{locale}.html` to Cloudflare Pages.

## 3. Material vs Minor Classification

See ADR-S11-007 (`specs/03_architecture/adrs/ADR-S11-007-privacy-notice-material-vs-minor.md`).

**Default rule:** when in doubt, Privacy Officer defaults to **minor** (defensible posture per ADR-S11-007 §4).

## 4. SLA Targets

| Step | Target | Owner |
|---|---|---|
| Native speaker review per locale | ≤ 7 days | Privacy Officer coordination |
| Legal local review per locale | ≤ 14 days | Privacy Officer + attorney |
| CI gate | Automated | CI pipeline |
| CD deploy | ≤ 1 hour post-merge | CI/CD |
| Diff publication | ≤ 24 hours post-merge | CI cron |
