---
title: "DEBT-016 — Statuspage URL substitution mechanism (engineering closure)"
date: "2026-05-16"
wave: "R-prep wave-24"
branch: "wt/r-prep-debt-016-statuspage-urls"
base_commit: "33138b5"
owner: "Orchestrator (engineering-side); SRE Lead (operator-side)"
status: "engineering-CLOSED; operator-bound URL provisioning pending T-7d"
debt_links:
  - "DEBT-016"
related:
  - "specs/_runbooks/STATUSPAGE-INIT.md"
  - "specs/_audits/sealed/2026-05-15-debt-register.md"
  - "marketing/launch/STATUS-PAGE-SPEC.md"
  - "apps/docs/docusaurus.config.ts"
  - "apps/docs/src/statuspage-url.ts"
---

# DEBT-016 — Statuspage URL substitution mechanism

## 1. Context

The customer-facing trust corpus references `status.corelink.humangr.com` across:

- **5 MDX pages** (`apps/docs/docs/trust/{index,incident-response,subprocessors}.mdx`,
  `apps/docs/docs/explanation/security/incident-history.mdx`,
  `apps/docs/docs/how-to/billing/manage-subscription.mdx`)
- **× 4 locales** (`en-US`, `pt-BR`, `es-419`, `de`) via i18n mirrors under
  `apps/docs/i18n/`
- **+ 20+ internal runbooks / compliance docs** under
  `specs/_runbooks/`, `specs/_compliance/`, and `docs/internal/`
- **+ 1 system-context diagram** (`docs/internal/architecture/diagrams/system-context.mmd`)
- **+ 2 Rust impl callers** (`crates/corelink-statuspage-real/src/lib.rs`
  and `crates/corelink-privacy-erasure-worker/src/statuspage_publish.rs`) —
  these consume `STATUSPAGE_API_URL` env vars at runtime and are
  out-of-scope for DEBT-016 (covered separately by their own
  operator-secret rotation procedures)

The statuspage instance is **not yet provisioned** as of GA-prep wave-24.
DEBT-016 entered the register with a "T-7d pre-launch" deadline and
"operator follows STATUSPAGE-INIT.md provisioning playbook" plan, but
neither the runbook nor the engineering-side substitution mechanism had
landed prior to this wave.

## 2. Engineering-side closure

This wave delivers the **engineering-side** of DEBT-016: an
operator-friendly URL-substitution mechanism + canonical default +
runbook. **Operator action (real Statuspage tenant provisioning + DNS
binding) remains pending and is gated at T-7d pre-launch.**

### 2.1 Substitution mechanism

**Build-time `customFields` injection via `docusaurus.config.ts`.** Choice
rationale:

- **Build-time over runtime header injection:** The trust corpus is
  served as static SSG output from Cloudflare Pages. Runtime header
  injection (CF Worker rewriting on egress) would add a hot-path latency
  cost on every trust-page render and create a new surface for CSP /
  CORS regression. Build-time substitution is invisible at the edge.
- **`customFields` over global var pollution:** Docusaurus's blessed
  pattern for plugin/MDX-component access to config-time values is
  `siteConfig.customFields` (read at component time via
  `useDocusaurusContext()`).
- **`process.env.STATUSPAGE_URL` with sane default over Mustache /
  envsubst:** Adding a Mustache pipeline would force a build-step
  before Docusaurus's own MDX compile and create an ordering hazard.
  The env-var pattern matches the existing Algolia keys precedent
  (`process.env.ALGOLIA_APP_ID ?? "STUB_APP_ID"`) already used at lines
  149-151 of the config.

### 2.2 Single source of truth

New helper `apps/docs/src/statuspage-url.ts` exports
`DEFAULT_STATUSPAGE_URL` (`"https://status.corelink.humangr.com"`) and
`getStatuspageUrl(customFieldsValue?)`. The helper is importable from
both Node (`docusaurus.config.ts`) and the browser (React components),
and consults `process.env.STATUSPAGE_URL` at config time + the
`customFields.statuspageUrl` injected value at component time.

### 2.3 Existing literal URLs in MDX

Existing literal `https://status.corelink.humangr.com` references in the 5 trust
MDX pages × 4 locales are **intentionally retained as canonical
defaults**. Rationale:

- **Option A (CNAME) is the preferred operator path** (zero docs rebuild
  required; documented in `specs/_runbooks/STATUSPAGE-INIT.md §2`).
  Under Option A, the literal URLs resolve correctly without any
  build-time substitution.
- **Option B (env-var override + text substitution) is documented as a
  deploy-time branch operation** when the operator must host under a
  different domain. The runbook §3 provides the canonical `rg | sed`
  one-liner.
- Templating 20+ MDX files × 4 locales with `{{STATUSPAGE_URL}}` /
  `${STATUSPAGE_URL}` placeholders would (a) break Docusaurus's MDX
  compile (placeholders are valid JSX text but trigger Algolia
  search-index noise + i18n-translation drift on every locale rebuild),
  and (b) force every operator into Option B even when Option A is
  cheaper.

## 3. Fallback policy

`https://status.corelink.humangr.com` — the canonical wave-19 commit value
referenced by the 8+ trust pages and 20+ internal docs. This is the
operator's **default canonical host**; the operator either CNAMEs it to
the real Statuspage tenant (Option A) or sets `STATUSPAGE_URL` env var
and performs a deploy-time text substitution (Option B). Both paths are
documented in `specs/_runbooks/STATUSPAGE-INIT.md`.

## 4. Deliverables landed this wave

| Path | Type | Purpose |
|---|---|---|
| `apps/docs/src/statuspage-url.ts` | NEW | `DEFAULT_STATUSPAGE_URL` constant + `getStatuspageUrl()` helper |
| `apps/docs/docusaurus.config.ts` | EDIT | Import helper + add `customFields.statuspageUrl` + DEBT-016 commentary |
| `specs/_runbooks/STATUSPAGE-INIT.md` | NEW | Operator provisioning playbook (Option A CNAME + Option B env-var); GA-cutover gate at T-7d |
| `specs/_audits/sealed/2026-05-16-debt-016-statuspage-urls.md` | NEW | This audit doc |
| `specs/_audits/sealed/2026-05-15-debt-register.md` | EDIT | DEBT-016 row updated → engineering-CLOSED (operator-bound URL provisioning pending T-7d) |

## 5. Substitution-mechanism summary (Report fields)

- **PLACEHOLDER PATTERN:** `process.env.STATUSPAGE_URL` (env var) +
  `siteConfig.customFields.statuspageUrl` (build-time inject). Literal
  URLs in MDX kept as canonical defaults; no template placeholder
  introduced to MDX itself (rationale §2.3).
- **SUBSTITUTION MECHANISM:** build-time, via `docusaurus.config.ts`
  `customFields`. Runtime header injection rejected (§2.1).
- **FALLBACK DEFAULT:** `https://status.corelink.humangr.com` (wave-19 canonical
  commit value).
- **RUNBOOK:** `specs/_runbooks/STATUSPAGE-INIT.md` (operator playbook
  Option A CNAME + Option B env-var override + GA-cutover gate).

## 6. GA blocker policy

Per `specs/_runbooks/STATUSPAGE-INIT.md §4`, **GA cutover is blocked at
T-7d** if neither Option A nor Option B is complete. This row in the
debt register transitions to engineering-CLOSED today; the operator-side
provisioning remains tracked under the same DEBT-016 ID for the
GA-readiness review.

## 7. Quality gates

- `python3 scripts/validate_specs.py` — green
- `python3 scripts/validate_references.py` — green
- `apps/docs` typecheck — `pnpm typecheck` deferred to wave-24 closure
  (DEBT-015-BUILD residual blocks full `pnpm build` chain; typecheck
  alone runs via `pnpm --filter @corelink/docs exec tsc --noEmit`)

## 8. Caveats

- The 20+ internal runbooks / compliance docs continue to reference the
  literal `status.corelink.humangr.com`; those are operator-facing (not customer-
  facing) and don't need the build-time substitution mechanism — they
  inherit the same canonical default and same Option A / Option B
  provisioning paths.
- Two Rust impl callers (`crates/corelink-statuspage-real`,
  `crates/corelink-privacy-erasure-worker`) consume statuspage API
  endpoints at runtime via their own env-var paths
  (`STATUSPAGE_API_BASE_URL` etc.). They are out-of-scope for DEBT-016
  but follow the same operator-bound provisioning model.
- The `docs/internal/architecture/diagrams/system-context.mmd` Mermaid
  diagram cites the literal URL inside a diagram node; rendering is
  unchanged by Option A and Option B operators can re-render with the
  substituted URL (Mermaid CLI invocation documented in the runbook
  cross-references).
