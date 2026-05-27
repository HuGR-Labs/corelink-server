---
id: "AUDIT-S18-PREFLIGHT"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Adversarial Pre-flight (Sonnet)"
tags: ["audit", "preflight", "s18", "public-docs"]
---

# S-18 Adversarial Pre-flight Review — Public Docs + API Reference + Pricing

Scope: review of `_spec_contract.md` + `WI-S18-001..005.md` only. Unmerged builder branches NOT inspected (per request). Working tree clean on `main @ bbbd99a`.

## 1. Pre-flight verdict

**NEEDS-SCOPE-FIX** (do not start build until P0s below resolved).

The sprint is correctly scoped LOW_RISK with cross-functional gate (CF-1/CF-2/CF-3) cleanly carved out as separate publish gates per anti-scope §10. The contract is internally rigorous on the cross-functional dimension. However, three load-bearing assumptions in builder-facing WI specs are factually wrong against the current repository state, and the waiver policy has unresolved internal contradictions between §19 of the contract and §6/§9.5 of WI-S18-005. These will cause builder thrash and/or silent drift. Not a FF-HR situation — risks are operational/spec-quality, not regulatory/financial liability promotion.

## 2. Spec gaps (P0 / P1)

### P0 (block build start)

**P0-1 — REAPI proto path is fictional in WI-S18-002.** Spec assumes `protos/reapi/v2/*.proto` (WI-S18-002 §1 codeblock line 58–65, §6.1 #2, Acceptance Criteria, Risk Register R-001). Repository actual layout:
- `crates/corelink-reapi/proto/build/bazel/remote/execution/v2/remote_execution.proto` (vendored Bazel REAPI v2 — canonical source)
- `crates/corelink-reapi/proto/google/...` (transitive deps)
- `apps/server/proto/health.proto`
- No `protos/` top-level directory exists.

Impact: `protoc-gen-doc` CI workflow `.github/workflows/docs-reapi-drift-check.yml` as specified will fail on first run (cannot resolve `-I=protos/`). The CTRL-DOC-AUTO-GEN canonical control — which §19 lists as **non-waivable** — is unbuildable as written.

Fix required before build: update WI-S18-002 §1 code excerpt, §6.1 #2 (source path + `--doc_out` invocation), §8 Acceptance Criteria, §13 Artifacts, and Risk R-001 to reference `crates/corelink-reapi/proto/` with explicit `-I` chain for google + build/bazel transitive includes. Decide whether REAPI v2 reference renders the upstream Bazel REAPI (vendored, externally maintained) or a CoreLink-authored extension proto (none exist yet). If the latter, S-18 has an undisclosed dependency on a proto-authoring WI that does not exist.

**P0-2 — Waiver policy internal contradiction.** `_spec_contract.md §19` (post Lote 10.18 codex P1) declares **"3 locales en-US/pt-BR/es-419 NÃO waivable"** and **"SBOM download path NÃO waivable"**. WI-S18-005 contradicts this in three places:
- §2 Narrative line 154–157: lists "3 locales → 2 locales GA" + "SBOM download → SBOM via support email com NDA" as typical CONDITIONALLY_APPROVED waivers.
- §6.1 #7 line 240–243: same list rendered as DoD-permissible waivers.
- §9.5 line 386–391: design decision explicitly justifies these as waivable.

Impact: builder of WI-005 will encode contradictory PRR gating logic; closing-ship-gate ADR template will accept waivers that the contract forbids. Single source of truth violated.

Fix required: align WI-S18-005 §2/§6.1/§9.5 with `_spec_contract.md §19` Lote 10.18 canonical — remove i18n and SBOM from the CONDITIONALLY_APPROVED list, retain only the 5-dev → 3-dev UX waiver (which §19 does mark waivable).

### P1 (fix during build, not blocking)

**P1-1 — Pricing tier $X/$Y/$Z are unresolved placeholders.** Contract §5.5 R-S18-9 and WI-S18-004 §6.1 #1 both reference `$X/mo` `$Y/mo` `$Z/mo` for Starter/Team/Pro with no canonical pricing model committed to spec corpus. Calculator stub in WI-S18-004 (lines 56–69) hard-codes `price_usd: 'X'`/'Y'/'Z'. The 10-scenario validation (Completeness 10.s18.5) cannot produce numeric expected values without a price model. Mitigation: contract correctly gates publish behind Finance sign-off (CF-1, non-skippable). But builder will ship stub-only calculator; Finance sign-off becomes the de-facto pricing-decision forum rather than ratifying a pre-existing model. Recommend: produce minimal `specs/02_models/pricing_model.md` (or similar) before WI-004 builder ships calculator, OR explicitly scope WI-004 deliverable as "calculator UI + 10 scenarios with TBD-pricing placeholder, Finance fills numbers at CF-1 review."

**P1-2 — Lighthouse Perf ≥ 95 on Algolia DocSearch + custom domain is realistic but tight.** Docusaurus 3 default build hits Perf 95+ on 5 simple routes when (a) Algolia search-modal is lazy-loaded (default in v3), (b) no client-side analytics injected, (c) preview deploys served via CF edge (HTTP/3, brotli). Pricing calculator (`PricingCalculator.tsx`) is the lone wildcard — interactive React component on `/pricing` route. WI-S18-005 line 124–132 correctly targets PR preview URL (Lote 10.18 codex P1 fix). Risk: if calculator pulls a chart lib (Chart.js / Recharts) Perf will drop ~10pts. WI-004 does not constrain calculator dependencies. Recommend P1 fix: WI-S18-004 §14 add "calculator must be zero-dep React (vanilla input + computed cost); no chart libs; SSR-friendly."

**P1-3 — i18n coverage 80% threshold not specified.** The user prompt asks about "i18n coverage 80%" — that threshold does NOT appear in `_spec_contract.md` or any WI. Contract §5.6 R-S18-12 + §19 require all 3 locales fully translated (NÃO waivable per Lote 10.18). WI-S18-005 §6.1 #1 says "missing translation = build fail." This is 100% coverage, not 80%. If a relaxed 80% bar is intended, contract must state it. As written, builder will configure Docusaurus `i18n.translateLocaleFallback` strictly and fail builds.

**P1-4 — Diátaxis quadrant assignments correct but `/security` `/compliance` `/pricing` outside taxonomy.** WI-S18-001 sidebar config (lines 70–80) places these as standalone navbar links; WI-S18-004 §6.1 #5 explicitly justifies them as "standalone, not Diátaxis category." This is defensible (Stripe pattern) but means Quality Standard 14.s18.2 "every page categorized" needs an exception clause for the 3 cross-functional pages. Contract §9 Quality Standards does not codify the exception. Risk: Docs-lead reviewer rejects PRs on taxonomy grounds. Recommend: contract §9 14.s18.2 footnote "pricing / security / compliance are standalone navbar links, not Diátaxis-bucketed."

**P1-5 — Vale + lychee CI thresholds not asserted.** WI-S18-005 §6.1 #4–#5 specify "PR fails se Vale violations" and "PR fails se broken-link" — strict zero-tolerance on both. This is risky for cosmetic Vale rules ("don't say 'simply'") on i18n content where pt-BR/es-419 native-speaker phrasing legitimately violates EN-tone heuristics. Recommend: WI-005 add "Vale runs on en-US sources only; pt-BR/es-419 exempt." For lychee, recommend `--max-redirects 5` + skip-cache 24h to avoid flaky external rate-limit failures (GitHub, Algolia, Docusaurus docs all have rate limits).

## 3. Cross-functional review enforcement check (pricing + security + compliance)

**Strong enforcement.** WI-S18-004 §8 Acceptance Criteria scenario "Pricing page Finance + Legal sign-off mandatory (cross-functional gate CF-1; Lote 10.18 codex P0 hard merge control)" requires the layered enforcement triad:
1. `.github/CODEOWNERS` mapping `apps/docs/docs/pricing.mdx` + `apps/docs/src/components/PricingCalculator.tsx` to `@finance-team` + `@legal-team`.
2. GitHub branch protection rule on `main` requiring CODEOWNERS approval (configured via `gh api`).
3. CI check `scripts/validate_cross_functional_signoffs.py` parsing `specs/04_sprints/S18/cross-functional-signoffs.md` and exiting 1 on missing entries.

§13 Artifacts explicitly lists CODEOWNERS, validator script, and branch protection rule. §15 Integration tests includes a dry-run hard-merge-control test.

**Does the WI enforce DRAFT + `cross_functional_review: TBD` on every pricing/security/compliance page?**

Partial gap. WI-S18-004 does NOT explicitly add `cross_functional_review:` frontmatter field to the page MDX schema. It enforces sign-off via the external log + CODEOWNERS rather than a per-page YAML field. The user prompt question implies a schema-field check that does not exist in spec. If the intent is to require `doc_status: DRAFT` + `cross_functional_review: TBD` on every MDX page until sign-off, that needs to be added as a P1 to WI-S18-004 §6.1 + §14 Quality Standards. Currently the gate is at the merge layer (CODEOWNERS + CI), not the document-content layer. Both are valid; the contract should pick one canonical pattern.

CF-1/CF-2/CF-3 reviewers correctly distributed (no over-coupling): CF-1 Finance+Legal; CF-2 Security+Privacy; CF-3 Legal+Privacy. Lote 10.18 codex P1 removed Security from pricing path correctly.

## 4. REAPI drift-protection assessment

**Drift-proof in design, broken in path.** Auto-gen mechanism (protoc-gen-doc + `git diff --exit-code`) is correctly drift-proof by construction — any `.proto` edit without regenerating rendered markdown fails CI. CTRL-DOC-AUTO-GEN is correctly non-waivable per §19.

But the CI workflow in WI-S18-002 §1 references the non-existent `protos/` path (see P0-1). Until that is fixed, the gate is unbuildable. Additional concerns:

- The 4-language manual examples (Rust/Python/Go/JS, R-S18-4) are NOT auto-generated and therefore NOT drift-protected. WI-S18-002 §15 Adversarial Scenario #3 acknowledges this ("4-language code example out of sync with REAPI v2 contract … quarterly review cadence") but quarterly review is too slow for a drift-prevention claim of "fresh from .proto." Recommend P1: snippet-test these examples against live SDK CI (S-15 SEALED reuse) so a SDK signature change breaks docs build, not a quarterly review.
- REAPI is the vendored Bazel Remote Execution API v2 (canonical Google/Bazel proto). CoreLink does not "own" this proto. The reference doc is essentially a re-render of upstream Bazel docs. Differentiator value vs. linking to `github.com/bazelbuild/remote-apis` directly is unclear and not justified in spec. Recommend P1: explicit narrative in WI-002 §9 on "why we render rather than link" (likely answer: hosting in our Diátaxis sidebar with our examples; acceptable but should be stated).

## 5. i18n coverage feasibility

**Math: 3 locales × (sidebar + ~30 doc pages estimated + 5 SDK guides + 7 tutorials placeholder + 3 cross-functional pages) ≈ 135–165 files needing pt-BR + es-419 translation.** User prompt's "4 SDK guide langs × 7 tutorials × 3 locales = 84 translation files" mis-counts: SDK guide languages (Python/Go/JS/CLI) are NOT a locale dimension — they are content. Locale dimension is 2 non-default × all pages.

Realistic at GA (2-week sprint)? **No, not at 100% coverage with native speaker review + Legal local review per §15 row 6.** Contract §19 forbids the 3→2 waiver (post Lote 10.18). This is a build-feasibility risk that the spec corpus has not reconciled. Either:
- (a) Reduce GA-day translation scope: only translate "shell" pages (homepage, getting-started, pricing, security, compliance, top-level sidebar labels) for pt-BR + es-419, with deep technical reference English-only at GA + machine-translated fallback. This is a contract amendment.
- (b) Stretch S-18 to 3 weeks for translation work.
- (c) Reinstate the 3→2 waiver (reverse Lote 10.18 codex P1 on this item).

WI-S18-005 PERT 14.7h is wildly insufficient for native-speaker + Legal-local review of 130+ files. Recommend P1: contract amendment specifying translation scope (shell-only vs. full corpus) before WI-005 builder starts.

**80% threshold (user prompt):** does not exist in spec. If desired, must be added — but conflicts with `missing translation = build fail` (WI-005 §6.1 #1).

## 6. Lighthouse Perf ≥ 95 attainability

**Attainable on 4 of 5 routes; tight on `/pricing`.** Routes assessed:
- `/` (homepage) — Docusaurus default, trivially ≥ 95.
- `/docs/getting-started` (MDX + code blocks) — ≥ 95 with proper syntax-highlight (Prism shipped lazy).
- `/docs/sdk/python` — same.
- `/security` (MDX + Cosign/Rekor code blocks) — ≥ 95.
- `/pricing` (interactive `PricingCalculator.tsx`) — risk zone. If calculator stays vanilla React with inline computation (per recommended P1-2 constraint), ≥ 95 holds. If it pulls Chart.js / D3 / Recharts for visualization, drops to ~85.

Algolia DocSearch v3 modal lazy-loads on user interaction (no LCP impact). Custom domain on CF Pages with HTTP/3 + brotli is best-in-class.

**Sustained 30d (Completeness 10.s18.7)** depends on nightly Lighthouse on live URL (correctly scoped separately in `.github/workflows/lighthouse-nightly.yml` per WI-005 lines 133–134). PR-preview gate at 95 + nightly drift detection is the right pattern.

## 7. Top 3 concerns (summary for closing report)

1. **P0-1 REAPI proto path is fictional** — builder cannot ship CTRL-DOC-AUTO-GEN as specified. Block build until WI-S18-002 updated to reference `crates/corelink-reapi/proto/build/bazel/remote/execution/v2/remote_execution.proto` with full `-I` include chain.
2. **P0-2 Waiver policy contradiction** — WI-S18-005 lists i18n + SBOM as waivable; contract §19 forbids both. Align WI-005 with Lote 10.18 canonical before builder encodes CONDITIONALLY_APPROVED logic.
3. **P1-1 / §5 feasibility** — i18n GA scope (130+ files × native + Legal review in 14.7h PERT) is not feasible. Either contract amends translation scope to shell-only, or sprint extends, or 3→2 waiver reinstated. Today the spec is internally consistent but operationally undeliverable.

---

**Fim audit S-18 pre-flight v1.0.0.**
