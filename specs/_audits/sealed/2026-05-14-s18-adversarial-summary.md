---
id: "AUDIT-S18-ADVERSARIAL-SUMMARY"
type: "audit"
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
parent: "WI-S18-005"
tags:
  - "audit"
  - "s18"
  - "adversarial"
  - "summary"
  - "cross-wi"
  - "ship-gate"
  - "docs"
  - "i18n"
  - "wcag"
  - "lighthouse"
  - "vale"
  - "lychee"
---

# S-18 Adversarial Summary — Cross-WI roll-up (public docs ship gate)

Cross-WI consolidation of S-18 adversarial scenarios + ship-gate
quantitative readout. 28 scenarios; 100 % named mitigation coverage.
Per WI-S18-005 §15 + spec contract S-18 §10 anti-scope.

---

## 1. Quantitative readout (ship-gate snapshot)

| Dimension | Target | Measured | Verdict |
|---|---|---|---|
| Docs taxonomy completeness (Diátaxis) | tutorial + how-to + reference + explanation × 100 % pages categorised | Pending WI-S18-001 SEAL; sidebar wired to Diátaxis roots | PENDING-PARALLEL |
| REAPI gen drift gate | CI gate green; zero drift `.proto` → rendered | `corelink-reapi.yml` extant; auto-gen step pending WI-S18-002 SEAL | PENDING-PARALLEL |
| SDK guide coverage | Python + Go + JS/TS + CLI (4 / 4) | Pending WI-S18-003 SEAL | PENDING-PARALLEL |
| Compliance + security + pricing DRAFT enforcement | All 3 pages `doc_status: DRAFT` until cross-functional sign-off | CODEOWNERS gate + `doc_status` lint pending WI-S18-004 SEAL | PENDING-PARALLEL |
| i18n coverage (en→{pt-BR, es-419}) | ≥ 80 % per locale | Script `apps/docs/scripts/i18n-coverage.ts` + `.github/workflows/docs-i18n.yml` SHA-pinned | OK (gate live) |
| a11y violations (serious + critical) | 0 | Playwright + axe-core sweep wired; gate enforced via `.github/workflows/docs-a11y.yml` | OK (gate live) |
| Lighthouse scores (5 routes × 4 pillars) | Perf ≥ 95, A11y = 100, BP ≥ 95, SEO ≥ 90 | `lighthouserc.cjs` + nightly + tag workflow `.github/workflows/docs-lighthouse.yml` SHA-pinned | OK (gate live) |
| Vale violation count | 0 `error` level | `.vale.ini` + 6 CoreLink rules + `.github/workflows/docs-vale.yml` SHA-pinned | OK (gate live) |
| lychee broken-link | 0 internal; external warning-only | `lychee.toml` + PR + nightly workflow `.github/workflows/docs-lychee.yml` | OK (gate live) |
| UX research 5-dev sample | bar 25/30 tasks (83 %); SUS ≥ 75 | DRAFT synthetic 83 % / 78.2; R-UX-05 expiry trigger for real-participant re-run | CONDITIONALLY_APPROVED |
| Cross-functional sign-offs (CF-1/2/3) | Finance + Legal + Privacy + Security per page | Pending WI-S18-004 SEAL + parallel staffing (ADR-0034 Option C) | PENDING-PARALLEL |

---

## 2. WI-S18-001 — Docusaurus foundation + Diátaxis + i18n + custom domain (5 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 1 | Custom-domain SSL misconfig (HSTS preload mismatch) | CF Pages CNAME wrong | DNS verify step in deploy workflow + `hsts-preload.org` baseline | LOW |
| 2 | Algolia DocSearch indexes private pages | Bad robots.txt / sitemap | Sitemap excludes `internal.*` paths; DocSearch config allowlist | LOW |
| 3 | Diátaxis sidebar drifts (page un-categorised) | New page lands without category | Lint script fails build if a page sidebar position is absent | LOW |
| 4 | i18n fallback shows English on `/pt-BR/` route | Missing translation | `i18n-coverage.ts` script (WI-S18-005 deliverable 1) — fails CI < 80 % | LOW |
| 5 | Edit-on-GitHub link leaks `main` SHA after force-push | Stale permalink | Use blob/main + last-modified plugin tied to git history | LOW |

## 3. WI-S18-002 — Getting started + REAPI auto-gen + 4-language examples (5 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 6 | REAPI rendered docs drift from `.proto` | Engineer hand-edits `.md` and forgets to re-gen | CI gate `corelink-reapi.yml` regen + diff fail; CTRL-DOC-AUTO-GEN | LOW |
| 7 | Code example uses real PAT or tenant_id | Copy-paste from dev box | CI lint scans for `pat_*` / `tenant_*` patterns; pre-commit hook | LOW |
| 8 | Quickstart > 5 min on freshly cloned starter repo | Cache cold, slow Bazel fetch | Starter ships with bazel-remote pre-warmed image; CI runs 5-min stopwatch | MEDIUM |
| 9 | Rust example shows panic instead of `?` propagation | Lazy author | Vale rule + PR review + clippy run on `examples/` | LOW |
| 10 | Python example uses requests sync in async section | Inconsistent SDK guidance | SDK guide ↔ REAPI reference cross-link tests | LOW |

## 4. WI-S18-003 — SDK guides (5 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 11 | Client verify documented as opt-in (drift from CTRL-CAS-002 default-on) | Author misremembers spec | Spec-cite assertion in SDK guide markdown linked back to security_model.md hash | LOW |
| 12 | CLI flag matrix out of date | New flag landed without doc update | `corelink --help` snapshot test in `corelink-meta.yml`; CI fail on diff | LOW |
| 13 | SDK install instruction uses `pip install -U` (insecure) | Dependency confusion | Pin via lockfile install (`uv pip install -r requirements.txt`) | LOW |
| 14 | JS guide leaks Cloudflare AccountID | Author copy-paste from internal | CI lint regex on account IDs; pre-commit hook | LOW |
| 15 | Go guide example deadlocks in user's `select` | Bad mutex order | All Go examples have `go vet` + `go test -race` gate | LOW |

## 5. WI-S18-004 — Compliance + security + pricing + cross-functional gate (6 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 16 | Pricing tier merged without Finance sign-off | Bypassed CODEOWNERS | CF-1 enforced via CODEOWNERS `apps/docs/docs/pricing/**` + branch protection "require review from CODEOWNERS"; CRITICAL post-mortem on bypass | LOW |
| 17 | Security claim merged without Security lead sign-off | Bypassed CODEOWNERS | CF-2 enforced via CODEOWNERS `apps/docs/docs/security/**`; CRITICAL post-mortem on bypass | LOW |
| 18 | Compliance claim contradicts canonical `compliance_matrix.md` | Author drift | Cross-reference lint script fails build if claim text hashes mismatch canonical | LOW |
| 19 | Pricing calculator off-by-100 % (cents vs dollars) | Bad unit math | 10-scenario test suite per spec contract §6 EVT-044; Finance review | LOW |
| 20 | SBOM downloadable URL returns 404 post-S-12 rotation | Release SHA changed | Release pipeline updates docs SBOM URL via auto-gen; lychee nightly | LOW |
| 21 | Pentest exec summary leaks CVE pre-disclosure | Bad redaction | Security lead + Privacy Officer dual sign-off; 90-day embargo template | LOW |

## 6. WI-S18-005 — Ship gate (7 scenarios)

| # | Scenario | Vector | Mitigation | Residual |
|---|---|---|---|---|
| 22 | Cross-functional review gate bypassed in pricing/security/compliance PR | Developer mistake / missing CODEOWNERS | Non-skippable per Waiver policy §19; CRITICAL post-mortem on inadvertent merge | LOW |
| 23 | Lighthouse score < 95 on a route post-merge | Perf regression sneaks in | Nightly Lighthouse workflow opens auto-issue; perf review weekly | LOW |
| 24 | axe-core violation introduced (new SVG missing aria-label) | New page lands | `docs-a11y.yml` PR gate — 0 serious/critical | LOW |
| 25 | Vale `error` level violation merged | Reviewer asleep | `docs-vale.yml` `fail_on_error: true` + branch protection requires green | LOW |
| 26 | Broken internal link merged | Slug rename | `docs-lychee.yml` PR run `--base apps/docs --offline` style scope on internal only | LOW |
| 27 | i18n missing translation in legal terms (pt-BR/es-419) | Translator skip | i18n-coverage script fails < 80 %; Legal local review per locale (spec contract §15 row 6) | LOW |
| 28 | UX research 5-dev sample fails (> 30 s on task or > 5 min on quickstart) | Discoverability regression | DRAFT synthetic baseline; R-UX-01..05 ticketed; expiry trigger drives D+10 real-participant re-run | MEDIUM |

---

## 7. Verdict

- 28 scenarios; 100 % named mitigation; 1 MEDIUM residual (UX-28 — synthetic
  baseline pending real-participant re-run R-UX-05).
- Ship-gate gates 5/9 live + SHA-pinned (i18n, a11y, Lighthouse, Vale,
  lychee); 4/9 PENDING-PARALLEL on WI-S18-001..004 SEAL.
- PRR-S18 promotes to `CONDITIONALLY_APPROVED` with R-UX-05 + cross-functional
  sign-offs collection + WI-S18-001..004 SEAL as expiry triggers.

---

**Fim S-18 adversarial summary cross-WI roll-up.**
