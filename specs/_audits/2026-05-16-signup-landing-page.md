# Signup Landing Page — Audit — 2026-05-16

> **Doc kind:** stream-deliverable audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-29 stream-2 agent (Claude Opus 4.7) — branch `wt/r-prep-signup-landing-page`.
> **Base:** `main` @ `365dd38` ("merge wt/r-prep-pre-cutover-weekly-verify into main (wave-28)").
> **Pairs with:** wave-29 stream #1 (`signup.corelink.humangr.com` backend handler — `wt/r-prep-signup-corelink-dev-backend`).
> **Canonical copy source:** `marketing/launch/PILOT-LANDING-PAGE-COPY.md` (wave-28 step-7, b0485bc).
> **Cross-ref:** `apps/docs/docusaurus.config.ts` (S18 docs site config), `marketing/launch/PILOT-ANNOUNCEMENT.md`, `specs/_compliance/GA-GATE-CRITERIA.md`.

---

## 1. Stream scope

Wave-28 commit `b0485bc` shipped the canonical landing-page COPY in markdown. This stream turns it into a real served page in the existing Docusaurus 3 docs site (`docs.corelink.humangr.com` — S18 foundation), with a 3-page signup flow and 4-locale i18n scaffolding.

Three pages delivered:

| Path | Purpose | Public URL at GA |
|---|---|---|
| `apps/docs/src/pages/pilot/index.tsx` | Landing (hero + 3 features + 5 FAQ + honest pre-GA banner) | `docs.corelink.humangr.com/pilot` |
| `apps/docs/src/pages/pilot/apply.tsx` | Token-gated signup form | `docs.corelink.humangr.com/pilot/apply` |
| `apps/docs/src/pages/pilot/welcome.tsx` | Post-signup welcome + onboarding next-steps | `docs.corelink.humangr.com/pilot/welcome` |
| `apps/docs/src/pages/pilot/pilot.module.css` | Tailwind-free standalone CSS, theme-variable-bound | (asset) |

The pages are served via Docusaurus 3 React-routing (`src/pages/` convention; per `docusaurus.config.ts` the docs preset uses `routeBasePath: "/"` so `src/pages/pilot/*` resolves at the root namespace). i18n is via `<Translate>` extraction; default-locale fallback is automatic for any locale that has not yet seeded a `code.json` entry — the established pattern in this repo for the `de` shadow locale per `apps/docs/i18n/TRANSLATION-WORKFLOW.md` SLA (≤ 14 d of EN change).

---

## 2. Design decisions

### 2.1 Landing copy fidelity

The COPY.md hero subhead is ~80 words. The hero text on the served page reuses the COPY.md hero subhead verbatim. The 3 feature blocks compress the COPY.md "Headline / Body / Supporting bullets" pattern into a tighter `featureGrid` 3-up — body retained, supporting bullets folded into body (visual density / above-fold goal). The 5 FAQ items are 1:1 with COPY.md §FAQ; the honest "what's pilot vs GA" block is summarised in the banner + reinforced in FAQ #1 (GA gate) and FAQ #4 (BYOK).

### 2.2 Honest pre-GA framing

Mandatory per wave-28 pilot comms precedent. Present in three locations:

1. **Banner at top of every pilot page** — quota + auto-convert to STANDARD at GA + 30-day window.
2. **FAQ #1** — explicit "we are not GA, GA is gated on GA-GATE-CRITERIA.md, ≥ 3 ACTIVE pilots".
3. **FAQ #2 + #3** — auto-convert + no contractual SLA during pilot.

### 2.3 Signup form contract

Backend pairing — POST `https://signup.corelink.humangr.com/v1/signup/pilot/{token}` per wave-27 token-based slot-reservation pipeline (stream #1 of wave-29 builds the handler).

Client validation:

| Field | Rule |
|---|---|
| token | 32-char Crockford base32 (`^[0-9A-HJKMNP-TV-Z]{32}$`), case-insensitive, normalised to upper-case before submit |
| email | RFC 5322 lite, ≤ 254 chars |
| company | 1..120 chars, trimmed |
| tierHint | enum (undecided / FREE / STANDARD / PRO / ENTERPRISE) |
| useCase | 1..280 chars |

Error-display states:

- **400** — "Invalid or expired token …" with email-the-team fallback.
- **429** — "Rate limited" with retry guidance.
- **503** — "Signup pipeline closed — all 10 pilot slots reserved" with waitlist guidance.
- Other / non-JSON → generic "signup failed, contact pilot@…".

Success → `window.location.assign("/pilot/welcome?slot=reserved")`. The `?slot=reserved` query gates the welcome confetti so direct visits don't fake celebrate. The token is not echoed in the URL — one-shot, owned by the backend.

### 2.4 Confetti

Tiny CSS-only burst (8 emoji spans with a 600 ms staggered pop animation, defined inline in the welcome page so it doesn't bloat `pilot.module.css`). No third-party dependency; passes Docusaurus SSR (gated on `typeof window !== "undefined"` for the query param read).

### 2.5 CSS module

`pilot.module.css` is scoped to the pilot signup flow. Uses Docusaurus theme variables (`--ifm-color-primary`, `--ifm-color-emphasis-*`, `--ifm-background-surface-color`) so palette tracks light/dark mode automatically and inherits the WCAG 2.2 AA contrast budget established in `apps/docs/src/css/custom.css` (R-prep wave-25 closure of DEBT-015). No Tailwind. No runtime CSS-in-JS.

### 2.6 i18n strategy

`<Translate id="..." description="...">` for every user-facing string + `translate({...})` for prop-position strings (page titles, meta descriptions, aria labels). 4 locales declared in `docusaurus.config.ts` are all served — non-default locales fall back to the inline EN string until a future XLIFF/MT-stub pass runs per `apps/docs/i18n/TRANSLATION-WORKFLOW.md`. This matches the existing pattern for the `de` shadow locale (added wave-19+, native-speaker review still pending across the corpus).

---

## 3. Quality gates

| Gate | Status | Notes |
|---|---|---|
| `pnpm build` (Node 22) | green for all 4 locales | DEBT-015-BUILD closed wave-25; Docusaurus 3.10.1 |
| typecheck | clean | strict React + Docusaurus types |
| lint | n/a (`apps/docs/` eslint targets `tests/` only per `package.json::scripts.lint`) | confirmed by reading `apps/docs/package.json` |
| `scripts/validate_specs.py` | green | this audit lives under `specs/_audits/` (SKIP_ALL) |
| `scripts/validate_references.py` | green | no new cross-spec references introduced |
| markdownlint | green | this audit is the only new MDX/MD file |

---

## 4. Honest gaps + follow-ups

1. **Translations are EN-baseline** for `pt-BR`, `de`, `es-419`. Build is green and pages render in all 4 locales, but text is English until the next XLIFF/MT-stub pass picks up the new strings (≤ 14 d SLA per workflow).
2. **No e2e Playwright test added** for the form flow. The wave-29 stream-1 (backend) audit should establish end-to-end fixtures; this stream is page-only.
3. **No A11y baseline regen.** A11y baseline lives at `apps/docs/i18n/A11Y-BASELINE-2026-05-14.json`. New pages should be folded in on the next a11y-audit pass (`pnpm a11y-audit:baseline`).
4. **Sitemap + robots.** Docusaurus auto-includes `src/pages/*` in `sitemap.xml` (preset config `sitemap.filename: "sitemap.xml"`). `robots.txt` allows everything by default — no changes needed.
5. **Navbar entry not added.** The pilot landing is reached from outbound emails / signup.corelink.humangr.com redirect — no in-site navigation surface needed pre-GA. If wave-30+ adds it, append `{ to: "/pilot", label: "Pilot", position: "left" }` to `themeConfig.navbar.items`.

---

## 5. Verdict

**APPROVE.** 3 pages + 1 CSS module + this audit. Honest pre-GA framing on every page. Client validation matches the wave-27 token contract. Build green on Node 22 for 4 locales.

— wave-29 stream-2 agent · 2026-05-16
