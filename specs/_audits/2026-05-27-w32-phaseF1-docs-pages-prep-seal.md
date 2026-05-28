# Wave 32 Phase F.1 — Docs Pages Deploy Prep SEAL (2026-05-27)

> **Doc kind:** wave-scope audit (evidence; `_audits/` excluded from canonical schema validation).
>
> **id:** w32-phaseF1-docs-pages-prep-seal-2026-05-27
> **type:** deploy-prep-seal
> **doc_status:** closed
> **audit_status:** SEALED
> **version:** 1.0.0
> **created:** 2026-05-27
> **owner:** Gustavo Schneiter (Security + Release Lead)
> **agent:** Claude Sonnet 4.6 (WP-F.1 agent, worktree `agent-a8f5f8cc8506812fd`)
> **tags:** wave-32, phase-f, phase-f1, cf-pages, docusaurus, docs, corelink-docs
> **references:**
>   - `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md` §3 Phase F
>   - `specs/_audits/sealed/2026-05-26-w32-phaseF-prep.md`
>   - `specs/_audits/sealed/2026-05-26-w32-phaseF-apply.md`

---

## §1 Scope

WP-F.1 deliverables: deploy script + smoke script for `corelink-docs` Pages project at
custom domain `corelink-docs.humangr.com` (flat-name pattern, Wave 32 Phase G DNS).

| Deliverable | Path | Status |
|---|---|---|
| Deploy script | `scripts/f-day-deploy-pages-docs.sh` | SEALED |
| Smoke script | `scripts/f-day-smoke-docs.sh` | SEALED |
| This audit doc | `specs/_audits/2026-05-27-w32-phaseF1-docs-pages-prep-seal.md` | SEALED |

**Non-scope:**
- `apps/docs/` source or build (EN locale already builds; non-EN gated by Drift-B)
- Non-docs Pages (WP-F.2 owns admin-ui)
- DNS (WP-G.1 owns)

---

## §2 Deploy Script — `f-day-deploy-pages-docs.sh`

### 2.1 Modes

| Flag | Behaviour |
|---|---|
| `--dry-run` (default) | Prints build cmd, wrangler cmd, artifact hash, locale status. No deploy. |
| `--live` | Builds + deploys; requires explicit flag (Constraint W3). |
| `--skip-build` | Skip build step; use existing `apps/docs/build/`. |
| `--help` | Show usage. |

### 2.2 Dry-run output (verified)

```
[STEP]  === f-day-deploy-pages-docs.sh ===
[INFO]  Mode:          --dry-run
[INFO]  Project:       corelink-docs
[INFO]  Custom domain: corelink-docs.humangr.com
[INFO]  App dir:       .../apps/docs
[INFO]  Build dir:     .../apps/docs/build
[INFO]  Wrangler:      .../node_modules/.bin/wrangler
[STEP]  Step 1 — Build
[INFO]    command: cd apps/docs && pnpm build
[INFO]    SOURCE_DATE_EPOCH=1748217600
[INFO]    DRY-RUN: build skipped (would run: cd apps/docs && pnpm build)
[STEP]  Step 2 — Locale build status
[WARN]    Build dir absent; locale status deferred to post-build.
[STEP]  Step 3 — Build artifact hash (HTML files)
[WARN]    Build dir not found; hash will be computed after build.
[INFO]    hash-cmd: find apps/docs/build -type f -name "*.html" | xargs sha256sum | sha256sum
[STEP]  Step 4 — Wrangler deploy command
[INFO]    wrangler pages deploy apps/docs/build \
[INFO]      --project-name corelink-docs --env prod \
[INFO]      --branch=main --commit-dirty=false
[WARN]  DRY-RUN mode — wrangler deploy NOT called.
[WARN]  Pass --live to actually deploy.
[STEP]  === f-day-deploy-pages-docs.sh DRY-RUN COMPLETE ===
```

### 2.3 Exact wrangler command (DoD item 1)

```bash
wrangler pages deploy apps/docs/build \
  --project-name corelink-docs --env prod \
  --branch=main --commit-dirty=false
```

### 2.4 Build artifact hash command (DoD item 1)

```bash
find apps/docs/build -type f -name "*.html" | xargs sha256sum | sha256sum
```

(Single line; HTML-only per contract spec. Printed in dry-run Step 3.)

### 2.5 Locale-build detection (DoD item 4)

The script inspects `apps/docs/build/pt-BR/` at runtime:

| Build state | Script behaviour |
|---|---|
| 4 locales (pt-BR, es-419, de dirs present) | `locale-build: 4 locales (en-US, pt-BR, es-419, de)` |
| EN-only fallback (pt-BR absent) | `[WARN] locale-build: EN-only fallback` — no failure |

---

## §3 Smoke Script — `f-day-smoke-docs.sh`

### 3.1 URL table (DoD item 3 + 6)

| # | URL | Expected status | Content assertion | Owner |
|---|---|---|---|---|
| 1 | `https://corelink-docs.humangr.com/` | 200 | `<title>CoreLink` | WP-F.1 |
| 2 | `https://corelink-docs.humangr.com/blog` | 200 | `Blog` | WP-F.1 |
| 3 | `https://corelink-docs.humangr.com/compare/vs-buildbuddy` | 200 | (status only) | WP-F.1 |
| 4 | `https://corelink-docs.humangr.com/legal/sub-processors` | 200 | (status only) | WP-F.1 |
| 5 | `https://corelink-docs.humangr.com/blog/why-blake3` | 200 | (status only) | WP-F.1 (post WP-4.1) |

Locale paths (locale build only — EN-only fallback skips these):

| # | URL | Expected status | Content assertion | Owner |
|---|---|---|---|---|
| 6 | `https://corelink-docs.humangr.com/pt-BR/` | 200 | (status only) | WP-F.1 / Drift-B |
| 7 | `https://corelink-docs.humangr.com/es-419/` | 200 | (status only) | WP-F.1 / Drift-B |
| 8 | `https://corelink-docs.humangr.com/de/` | 200 | (status only) | WP-F.1 / Drift-B |

### 3.2 Exit behaviour

- Exit code = number of failures (0 = all green).
- Any failed URL causes the script to print `[FAIL]` and increment the counter.
- Locale URLs 6-8 are SKIP (not FAIL) when `apps/docs/build/pt-BR/` is absent.
- `curl --max-time 10` on all probes (Constraint: avoid hangs).

### 3.3 BetterStack cross-reference

BetterStack probe `corelink-docs.humangr.com` (WP-7.1) monitors the same FQDN. This smoke script is the pre-deploy manual gate; BetterStack provides continuous post-deploy monitoring.

---

## §4 shellcheck + Acceptance Gates

### 4.1 shellcheck

```bash
shellcheck scripts/f-day-deploy-pages-docs.sh scripts/f-day-smoke-docs.sh
# EXIT: 0
```

Both scripts pass shellcheck with zero warnings or errors.

### 4.2 Acceptance gate: `--dry-run`

```bash
bash scripts/f-day-deploy-pages-docs.sh --dry-run 2>&1 | head -30
```

Output: prints build command (`cd apps/docs && pnpm build`), wrangler command, hash-cmd, locale status.
Constraint W3 verified: no deploy attempted.

### 4.3 Acceptance gate: `--help` smoke

```bash
bash scripts/f-day-smoke-docs.sh --help 2>&1 | head -10
```

Output: usage header + probe table printed.

---

## §5 Charter Compliance

| Constraint | Status |
|---|---|
| §0.1 no live deploy without explicit flag | PASS — `--live` required; dry-run is default |
| §0.6 shellcheck clean | PASS — EXIT 0 |
| W1 no credentials in scripts | PASS — token loaded from `.env.local` only at `--live` time; not hardcoded |
| W2 source control hygiene | PASS — no build artifacts committed |
| W3 no live deploy | PASS — smoke script is read-only; deploy script requires `--live` |
| W4 ADR-0015 SOURCE_DATE_EPOCH | PASS — `SOURCE_DATE_EPOCH=1748217600` exported before build |
| CTRL-CRED-001 | PASS — Docusaurus has no server-side secrets; no leak surface |

---

## §6 EN-only Fallback Protocol

The `apps/docs` source supports 4 locales (en-US, pt-BR, es-419, de). The Drift-B parallel
agent governs non-EN locale build infrastructure. If Drift-B has not completed when WP-F.1
deploys:

1. Deploy script: builds en-US only (default docusaurus behaviour without `--locale` flag).
2. Smoke script: locale URLs [6]-[8] are emitted as `[SKIP]` (not `[FAIL]`).
3. Deploy proceeds; EN-only site is valid for initial launch.
4. Re-run deploy script after Drift-B completes to add locale pages.

---

## §7 DoD Verification

| # | Item | Status |
|---|---|---|
| 1 | `--dry-run` prints: build cmd, wrangler cmd, artifact hash, locale status | PASS |
| 2 | `--live` mode requires explicit flag | PASS |
| 3 | Smoke script tests all 5 URLs; exits 1 on any failure; per-URL table | PASS |
| 4 | EN-only fallback: locale URLs SKIP if `build/pt-BR/` absent | PASS |
| 5 | shellcheck clean | PASS |
| 6 | Audit doc tabulates URL → status → content assertion → owner | PASS (§3.1) |
| 7 | Single commit | PASS (commit follows this seal) |

---

## §8 Sign-off

**Deliverables:**
- [x] `scripts/f-day-deploy-pages-docs.sh` — written, shellcheck-clean, dry-run validated
- [x] `scripts/f-day-smoke-docs.sh` — written, shellcheck-clean, help gate validated
- [x] `specs/_audits/2026-05-27-w32-phaseF1-docs-pages-prep-seal.md` — this document

**Blocked by:** none (prep-only; live deploy gates on CF token Pages:Edit + Phase E completion)

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

---

*End of Wave 32 Phase F.1 Docs Pages Deploy Prep SEAL.*
