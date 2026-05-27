---
id: "AUDIT-S18-SPRINT-CLOSE-R1"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Adversarial Review Round 1 (Sonnet)"
tags: ["audit", "sprint-close", "s18", "adversarial"]
---

# S-18 Sprint-Close Adversarial Review — Round 1

Scope: post-merge sprint-close review of S-18 (Public Docs + API Reference + Pricing).
Repo state: `main @ 974797d` (clean, post-merge of 5 SEALED WIs + Docusaurus Node-20-pin remediation commit).

## 1. Score

**5.5 / 10**

Justification: Strong execution on tooling/scaffolding (SHA-pinned CI, hoisted-pnpm explanation, drift-checked REAPI auto-gen on the real proto path, 264 unit tests passing). However, two structural failures remain unaddressed post-merge:

- The pre-flight P0 about the WI-S18-005 waiver contradiction was **NOT fixed**. WI-S18-005 still lists "3 locales → 2", "5-dev UX → 3-dev", and "SBOM download → email/NDA" as CONDITIONALLY_APPROVED waivers in §2, §6.1, §9.5 (and the Gherkin scenario at line 348–351) while spec contract §19 explicitly declares two of those three items **non-waivable**.
- New P0: `pnpm i18n-coverage` returns **0.0 %** coverage on both pt-BR and es-419 (100 source files, 0 translated, 0 TODO markers) against a hard 80 % threshold. Per spec contract §19, locale coverage is non-waivable. The CI gate `docs-i18n.yml` will fail on the next run, and the script is not even registered in `apps/docs/package.json` scripts (the workflow runs `pnpm i18n-coverage`, which would also fail with "Command not found").

The Node-20 pin is real and well-engineered (`.nvmrc`, `.npmrc` with documented `hoisted` linker, `engines` field, all five workflows pinned), but local build still cannot be verified on this developer machine due to Node 22; CI on Node 20 is plausible but unverified.

## 2. Verdict

**FAIL — P0 fixes needed.**

The sprint cannot SEAL until either (a) the WI-S18-005 waiver text is reconciled with spec contract §19 and (b) the i18n-coverage gate is reconciled with the reality that no translations are checked in (either ship initial pt-BR + es-419 translations + `<!-- i18n:TODO -->` markers to clear the 80 % gate honestly, or register the script in `package.json` and raise/honour the gate per the contract — but §19 forbids waiving it).

## 3. Pre-flight P0 verification

| P0 | Status | Evidence |
|---|---|---|
| REAPI proto path fictional | **FIXED** | `apps/docs/scripts/gen-reapi-docs.ts` lines 38–40 + 79–80 walk the real path `crates/corelink-reapi/proto/`. File `remote_execution.proto` exists at the canonical Bazel-style path. `pnpm test` exercises `pnpm docs:gen:check` (reapi-gen.test.ts line `regenerates without drift` passes in 1.7 s). |
| Waiver contradiction (3-locales + SBOM-download non-waivable) | **NOT-ADDRESSED** | WI-S18-005 lines 154–157, 240–243, 348–351, and 386–391 still list both items as CONDITIONALLY_APPROVED waivers, directly contradicting `_spec_contract.md` §19 lines 301–303 ("**3 locales ... NÃO waivable**" / "**SBOM download path NÃO waivable**"). The WI document was SEALED in this state and merged. |

## 4. Docusaurus build status

Local `pnpm build` (Node 22.17.1) fails with:

```
Error [ERR_MODULE_NOT_FOUND]: Cannot find package '@theme/prism-include-languages'
  imported from .../node_modules/@docusaurus/theme-classic/lib/prism-include-languages.js
```

Remediation in `974797d`:

- `apps/docs/.nvmrc` → `20.19.0`
- `apps/docs/.npmrc` → `node-linker=hoisted` + `public-hoist-pattern[]=@docusaurus/*` / `@theme/*` / `@mdx-js/*`
- `apps/docs/package.json` → `"engines": { "node": ">=20.0.0 <22.0.0" }`
- All five docs workflows pin Node 20 (verified — see §6).

Viability judgment: **HIGH plausibility** the CI build passes on Node 20. The root cause documented in `.npmrc` (pnpm symlink layout + Node 22 ESM strict resolution defeats Docusaurus's `ssgNodeRequire` `@theme/*` alias re-resolution) is technically accurate, and `hoisted` is the canonical upstream-Docusaurus-compatible workaround. The pin is enforced at three layers (`.nvmrc`, `engines`, CI), which is defense-in-depth.

Caveat: the local repo-root `node_modules` was created by pnpm with the existing isolated layout from before the `.npmrc` landed; CI starts from a clean checkout and will use the `hoisted` layout from the first install. This bug has NOT been reproduced under Node 20 to fully verify the fix.

## 5. New P0 / P1 / P2 from implementation

### P0 (must fix before SEAL)

1. **WI-S18-005 waiver contradiction unresolved** (pre-flight P0 carried over). Either delete the "3-locales → 2", "SBOM-download → email/NDA" lines from §2 / §6.1 / §9.5 / Gherkin scenarios, or amend spec contract §19 — but the contract was tightened in v1.2.0 *explicitly* to make them non-waivable, so the WI must align.
2. **i18n-coverage hard-fails (0 % / threshold 80 %)**. `apps/docs/i18n/{pt-BR,es-419}/docusaurus-plugin-content-docs/current/` is empty. 100 source files, 0 translated, 0 TODO markers. Spec contract §19 line 301 declares the 3-locale gate non-waivable; PRR W-4 only DEFERS native-speaker review while still asserting "i18n coverage gate enforces 80 % minimum" — but the gate cannot pass at 0 %.
3. **`pnpm i18n-coverage` not registered as a script.** `apps/docs/package.json` `scripts` block lacks the `i18n-coverage` entry. `.github/workflows/docs-i18n.yml` line 56 invokes `pnpm i18n-coverage --threshold 0.8`, which will exit with `ERR_PNPM_RECURSIVE_EXEC_FIRST_FAIL  Command "i18n-coverage" not found` — confirmed reproducible locally.

### P1

4. **`docs-reapi-gen.yml` actions NOT SHA-pinned.** Three lines use floating `@v4` tags (`actions/checkout@v4`, `pnpm/action-setup@v4`, `actions/setup-node@v4`) while all six other docs workflows are properly SHA-pinned (commit-hash + `# v...` comment). Charter violation; supply-chain regression.
5. **Bundle-size target unverified.** Spec target ≤ 250 KB main bundle is documented but `pnpm build` cannot run locally and there is no CI step that asserts the bundle-size budget (`docs-ci.yml` and `docs-lighthouse.yml` do not include a size-budget assertion that would fail the build on regression).

### P2

6. PRR-S18 W-4 wording ("i18n coverage gate enforces 80 % minimum") implies the gate is currently green, but reality is 0 %. This is misleading sign-off context — either re-word as "i18n coverage gate currently blocked at 0 %; remediation in flight" or fix the coverage.
7. `apps/docs/CONTENT-REVIEW.md` is a tracker, not a CI-enforced gate; the contract is enforced by `tests/cross-functional.test.ts` (134 lines, 58 tests passing). Acceptable design but the README does not state CI-enforcement clearly — minor.

## 6. Charter constraints check

| Constraint | Status | Evidence |
|---|---|---|
| All docs workflows SHA-pinned | **FAIL (P1)** | 6/7 workflows pinned; `docs-reapi-gen.yml` uses `@v4` floating tags on three actions. |
| All docs workflows pin Node 20 | **PASS** | `grep -n "node-version\|NODE_VERSION" .github/workflows/docs-*.yml` → all five resolved Node-version refs are `"20"`. |
| Bundle size ≤ 250 KB enforced | **N/A (P1)** | Local build blocked by Node 22; no CI budget assertion found. |
| i18n coverage ≥ 80 % en→{pt-BR, es-419} | **FAIL (P0)** | Script reports `0.0 % / 0.0 %` (`100 missing` per locale). |
| No `dangerouslySetInnerHTML` in custom React | **PASS** | `grep -rn dangerouslySetInnerHTML apps/docs/src/` returns empty. |
| No specific `$` amounts in `pricing/` | **PASS** | `grep -nE '\$[0-9]+\|USD [0-9]+' apps/docs/docs/pricing/*.mdx` empty; enforced by `tests/cross-functional.test.ts:122` regex `/\$\d/`. |
| No merge-conflict markers | **PASS** | grep across `apps/docs/` and `specs/` returns empty. |
| `python3 scripts/validate_specs.py` (S-18 failures) | **PASS** | 0 S-18 failures; 14 pre-existing S-11/S-12/S-13 ADR schema failures (`type`/`doc_status`/etc. missing on ADRs) — out of S-18 scope. |
| `pnpm typecheck` / `pnpm lint` / `pnpm test` | **PASS** | typecheck + lint clean; **264 / 264** tests passing across 11 files (reapi-gen drift check inclusive). |

## 7. Positives

- REAPI auto-gen correctly walks the **real** proto path (`crates/corelink-reapi/proto/build/bazel/remote/execution/v2/remote_execution.proto`) and the drift-detection mode (`pnpm docs:gen:check`) is wired into `tests/reapi-gen.test.ts` — pre-flight P0 #1 is genuinely closed.
- 264 unit tests, including a 58-test cross-functional gate (TBD frontmatter + DRAFT banner + zero-`$\d` enforcement on pricing pages), all green.
- Node-20 remediation is well-engineered: documented root cause in `.npmrc`, defense-in-depth across `.nvmrc` + `engines` + CI matrix, scope correctly limited to `apps/docs/` per the `.npmrc` comment.
- 6 of 7 docs workflows are SHA-pinned with `# vX.Y.Z` comments — best-in-class supply-chain hygiene where applied.
- Spec contract §19 was tightened (v1.2.0) to make 3-locales and SBOM-download non-waivable — strong policy posture.
- Cross-functional review gate is enforced by CI tests (not just a tracker doc), with pricing-page no-specific-`$` regex enforced at unit-test time.
- Charter alignment: no `dangerouslySetInnerHTML`, no merge-conflict markers, no specific `$` amounts.

## 8. Per-WI sub-scores

| WI | Score | Notes |
|---|---|---|
| WI-S18-001 (foundation + Diátaxis + i18n + custom domain) | 7.0 / 10 | Solid Docusaurus 3.10.1 scaffold; i18n directory structure created but empty; custom-domain config not independently verified here. |
| WI-S18-002 (getting-started + REAPI auto-gen + 4-language examples) | 8.5 / 10 | REAPI auto-gen genuinely walks real protos, drift-checked in CI; pre-flight P0 closed. |
| WI-S18-003 (SDK guides + CLI per-command + client verify default-on) | 7.5 / 10 | 145 SDK-guide tests + 21 CLI-reference tests passing; well-covered. |
| WI-S18-004 (compliance + security + pricing + CF-1/2/3 gate) | 7.5 / 10 | CODEOWNERS gate + cross-functional unit-test gate + `$\d` regex are real. CONTENT-REVIEW.md tracker is documentation-only but acceptable as a complement. |
| WI-S18-005 (i18n + WCAG + Lighthouse + Vale + lychee + UX + PRR closing) | **3.5 / 10** | i18n-coverage script exists but reports 0 %; not registered in `package.json`; CI workflow will hard-fail; waiver section directly contradicts spec contract §19 (non-waivable items listed as typical waivers). UX synthetic-baseline + adversarial 28-scenario roll-up are positives. |

## 9. Recommended remediation order

1. Fix `apps/docs/package.json` scripts: add `"i18n-coverage": "tsx scripts/i18n-coverage.ts"`.
2. Ship minimal pt-BR + es-419 translations OR insert `<!-- i18n:TODO -->` markers in every English source file to push coverage above 80 % honestly (per the script's TODO-counts-as-translated convention).
3. Amend WI-S18-005 §2 / §6.1 / §9.5 / Gherkin scenarios to remove 3-locales and SBOM-download from the "typical CONDITIONALLY_APPROVED waivers" list; replace with reference to spec contract §19 non-waivable rule.
4. SHA-pin the three actions in `docs-reapi-gen.yml`.
5. Add a CI bundle-size budget assertion to `docs-ci.yml` (e.g., a step that diffs `apps/docs/build/assets/js/runtime~*.js` size against a stored baseline).
6. Re-run round-2 adversarial review after fixes land.

---

**End audit — Round 1, FAIL verdict, 3 P0s + 2 P1s outstanding.**
