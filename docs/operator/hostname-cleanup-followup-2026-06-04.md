# Hostname cleanup — post-launch follow-up

**Date:** 2026-06-04 · **Owner:** Gustavo · **Context:** GAP-2/3 fix (commit `7cd188b1`)

## Decision (ground truth)

The **live production hostnames are the flat `corelink-*.humangr.com` pattern.**
The dotted `*.corelink.humangr.com` form **does not resolve** (verified `dig`
2026-06-04 + Wave-32 sign-off). Owner-confirmed canonical customer-app host:
**`humangr.com`**; API: **`corelink-api.humangr.com`**.

`clerk.corelink.humangr.com` is the **one intentional exception** — Clerk's
required Frontend-API CNAME format; leave it dotted.

## Fixed in GAP-2/3 (launch-critical runtime — already done)

All repointed to live flat hosts in `7cd188b1`:

- `apps/admin-ui/src/lib/api-client.ts` + `dsr-client.ts` default base → `corelink-api`
- `apps/admin-ui/src/lib/csp.ts` `connect-src` → `corelink-api`
- `apps/admin-ui/src/app/api/csp-report/route.ts` fallback → `corelink-api`
- `apps/admin-ui/src/app/.../billing/PortalLauncher.tsx` return URL → `corelink-admin`
- `apps/admin-ui/src/app/api/newsletter/subscribe/route.ts` CORS fallback → `corelink-docs`
- `worker/src/index.ts` CORS allowlist → `corelink-admin`/`corelink-app`/`corelink-docs`
- `crates/corelink-container/src/routes/tier_select.rs` test fixtures → `corelink-admin`
- + the tests that assert these (`worker/tests/index.test.ts`, the miniflare
  integration test, `apps/admin-ui/tests/csp.test.ts`)

## Deferred (NOT on the launch path — safe to defer, but track)

| # | Area | Files (representative) | Why deferred | Priority |
|---|------|------------------------|--------------|----------|
| 1 | **WebAuthn RP origins** | `crates/corelink-auth/src/webauthn*.rs`, `examples/webauthn_*.rs`, `tests/webauthn_*.rs` | Passkeys are post-launch; launch auth = Clerk. Large self-consistent set; when passkeys ship the RP-ID must equal the serving host (`corelink-admin`). | Med (before passkey GA) |
| 2 | **Multi-region routing** | `crates/corelink-region/src/region.rs:87`, `crates/corelink-core/src/types/region.rs:18`, `crates/corelink-privacy/tests/residency_region_adversarial.rs`, `ADR-MULTI-REGION-V1.md` | Code generates `{region}.api.corelink.*`; wrangler routes are `{region}.corelink-api.*`. Launch is single-region (IAD). | Med (before region #2) |
| 3 | **Signed-deploy route attestation** | `.github/workflows/cosign-sign.yml:195`, `crates/corelink-ops/src/deploy/types.rs` + tests/examples | Attests route `api.corelink.humangr.com/*`; real route is `corelink-api.humangr.com/*`. Verify the attestation harness isn't gating prod deploy on the wrong pattern. | **Check before first signed deploy** |
| 4 | **e2e / playwright prod defaults** | `.github/workflows/e2e-prod.yml:52`, `apps/admin-ui/playwright.prod.config.ts:30`, `apps/admin-ui/e2e/signup-welcome.spec.ts:29` | Fallbacks default to dead `app.corelink`. If repo var `E2E_BASE_URL=https://humangr.com` is set, the gate passes; otherwise it fails. **Set the repo var** or align the fallbacks. | Med (gate hygiene) |
| 5 | **Rate-limit 429 doc link** | `crates/corelink-rate-headers/src/headers.rs:172,176` | `docs_url` in 429 responses points at dead `docs.corelink`. User-facing but non-blocking. | Med |
| 6 | **Pilot-signup activation URL** | `crates/corelink-container/src/routes/signup.rs:674`, OpenAPI signup examples, `dashboards/grafana/dash-pilot-tenants.yml:161` | `signup.corelink.humangr.com/pilot/activate` is dead (live = `corelink-signup`). **OWNER: is the pilot flow on the launch path, or superseded by self-serve Clerk?** If live, this is higher priority. | **Owner decision** |
| 7 | **OpenAPI JSON server URL** | `openapi/corelink-v1.json:21` | Dotted `api.corelink`; the YAML is already flat. Regenerate JSON from YAML. | Low |
| 8 | **CLI doc-comment examples** | `tools/cli/examples/quickstart_signup.rs:10`, `src/main.rs:258`, `src/commands/runbook_drill.rs:17` | Comments only; the CLI **default endpoint is already** `corelink-api` (correct). | Low |
| 9 | **Dead Pages middleware canonical host** | `apps/admin-ui/functions/_middleware.ts:30` (`CANONICAL_HOST = "app.corelink…"`) | Part of GAP-1 (dead Pages path). Confirm the live `middleware.ts` does NOT redirect to a dead canonical host. | **Check (overlaps GAP-1)** |

### Row 6 — RESOLVED 2026-08-22

The owner decision the row asked for was made: **the pilot flow is on the launch
path**, and it is not superseded by self-serve Clerk — it feeds into it. The
activation URL now points at the live Clerk sign-up surface
(`https://humangr.com/corelink/sign-up?pilot=<id>`) instead of the dead
`signup.corelink.humangr.com/pilot/activate`, which was dead twice over: the
hostname is NXDOMAIN, and no `/pilot/activate` page was ever built on any host.
Fixed in the same change: the Rust constant, both OpenAPI mirrors, this
dashboard tile, and the intake form's POST target (which was also aimed at the
NXDOMAIN host, so the advertised-open pilot could not accept one application).

## Owner launch-day hostname items

- Ensure **`CORELINK_API_URL`** (server-side, admin-ui Worker) **and**
  **`NEXT_PUBLIC_CORELINK_API_URL`** (build-time) are both set to
  `https://corelink-api.humangr.com`.
- Confirm the **Clerk Frontend-API CNAME** `clerk.corelink.humangr.com` is
  provisioned (Clerk dashboard) and listed in allowed origins.
- Confirm Clerk **redirect URLs** use `https://humangr.com`
  (see `docs/operator/clerk-redirect-urls-state.md`).
