# Sentry setup runbook — Gustavo five-minute click-through

**Status:** ready to execute
**Owner:** Gustavo (Sentry-side account creation + secret provisioning)
**Pre-req:** code wiring has already landed (see commit `feat(observability): wire Sentry SDK …`)
**Time:** ≈ 5 minutes wall-clock + one CF Pages redeploy
**Cost:** $0 (Sentry Developer Free tier — 5 k errors/mo, 30 d retention)

---

## 0. Why now

Per `specs/_audits/2026-05-27-legal-ops-setup.md` §5 (analytics + monitoring stack), Sentry is one of the four free-tier observability pillars (Sentry / Axiom / Better Stack / Plausible). Code wiring is in place across:

- **admin-ui** — `@sentry/nextjs` with client + server + edge configs (PII filter, Authorization scrub, 10 % traces sample, errors 1.0, tunnelled through `/monitoring` to dodge ad-blockers).
- **docs** — `@sentry/browser` CDN loader injected via `headTags` in `docusaurus.config.ts` (no-op when DSN unset).
- **3 Workers** (get-corelink, analytics, signup) — `@sentry/cloudflare` wrapping the fetch handlers + cron via `Sentry.withSentry()`.

Wiring is **opt-in** — when the env vars below are unset, every surface degrades to a no-op (verified by green `pnpm test` runs across all 3 Workers and a clean admin-ui build). Nothing is shipped to Sentry until you complete this runbook.

---

## 1. Create the Sentry account (1 min)

1. Open https://sentry.io/signup/
2. Choose **Developer** plan (free, no credit card).
3. Org slug suggestion: `corelink` (one slug per HuGR product; this matches the `SENTRY_ORG` default baked into `apps/admin-ui/next.config.ts`).

---

## 2. Create the three projects (2 min)

In the Sentry dashboard create **three** projects (one per surface) so error volumes don't co-mingle and you can route alerts independently:

| Project slug             | Platform        | Purpose                              |
|--------------------------|-----------------|--------------------------------------|
| `corelink-admin-ui`      | Next.js         | client + server + edge runtimes      |
| `corelink-docs`          | Browser / JS    | docs.corelink.humangr.com            |
| `corelink-workers`       | JavaScript      | get / analytics / signup (shared)    |

Sharing one project across the three Workers is fine pre-PMF — they're all infrastructure and you want one alert pane. Split later if signal-to-noise drops.

After project creation Sentry shows the DSN — copy each one. They look like `https://abc123@o4500.ingest.sentry.io/4501`.

---

## 3. Provision secrets (2 min)

### 3.1 admin-ui (Cloudflare Pages)

```bash
# DSN — public because it's read by the browser bundle. Marked as a
# secret so it's not echoed in build logs.
wrangler pages secret put NEXT_PUBLIC_SENTRY_DSN --project-name=corelink-admin-ui
# Paste: <DSN for corelink-admin-ui project>

# Source-map upload token (Settings → Auth Tokens → Create New Token,
# scopes: project:read + project:releases + org:read).
wrangler pages secret put SENTRY_AUTH_TOKEN --project-name=corelink-admin-ui
# Paste: <token from sentry.io/settings/account/api/auth-tokens/>

# Optional override (defaults baked into next.config.ts already match)
wrangler pages secret put SENTRY_ORG --project-name=corelink-admin-ui
# Paste: corelink
wrangler pages secret put SENTRY_PROJECT --project-name=corelink-admin-ui
# Paste: corelink-admin-ui
```

Trigger a Pages redeploy (push any commit, or use the dashboard "Retry deployment" button).

### 3.2 docs (Cloudflare Pages)

```bash
wrangler pages secret put SENTRY_DSN_DOCS --project-name=corelink-docs
# Paste: <DSN for corelink-docs project>
```

Then redeploy the docs Pages project.

### 3.3 Workers (one secret per Worker)

```bash
# Run inside each worker directory or pass --name.

cd apps/get-corelink-worker
wrangler secret put SENTRY_DSN
# Paste: <DSN for corelink-workers project>

cd ../analytics-worker
wrangler secret put SENTRY_DSN
# Paste: <same DSN>

cd ../signup-worker
wrangler secret put SENTRY_DSN
# Paste: <same DSN>
```

For each Worker the next `wrangler deploy` (or any CI deploy that hits `--env prod`) picks up the secret. No code change needed.

---

## 4. Smoke-test (1 min)

After redeploys propagate:

```bash
# admin-ui — visit any page that triggers a known error path (e.g. force
# a 500 in dev tools by navigating to a malformed deep link). Then
# refresh the Sentry "Issues" tab for the corelink-admin-ui project.

# Workers — issue a malformed request to the dev/staging Worker:
curl -X POST https://get.corelink.io/  # 405 (does NOT throw, won't appear)
# Then to force a throw, temporarily add a `throw new Error("smoke")` in
# a non-prod env and curl it — revert after Sentry confirms receipt.
```

Sentry's free tier delivery latency is < 30 s typically.

---

## 5. Configure alerts (optional, recommended)

In each project: **Alerts → Create Alert → Issues → "Number of events in an issue is more than 10 in 1 hour" → Notify: email**. This is the lowest-noise default; tune as volume grows.

---

## 6. Upgrade trigger

When your error count crosses ≈ 4 k / month you'll see an "approaching quota" mail from Sentry. That's the cue to:

- Decide whether to upgrade to **Team @ $26/mo** (unlimited dashboards, 90-day retention, advanced alerting), or
- Reduce noise (raise `tracesSampleRate` floor, add `ignoreErrors`, fix the top-N issues).

Per the legal-ops audit projection (§9 "expected ramp"), Team plan is appropriate ≈ Month 4–6 post-launch.

---

## 7. Rollback

If a surface mis-behaves and you need to disable Sentry instantly:

| Surface           | Command                                                              |
|-------------------|----------------------------------------------------------------------|
| admin-ui          | `wrangler pages secret delete NEXT_PUBLIC_SENTRY_DSN …` + redeploy   |
| docs              | `wrangler pages secret delete SENTRY_DSN_DOCS …` + redeploy          |
| each Worker       | `wrangler secret delete SENTRY_DSN` + `wrangler deploy`              |

The Sentry SDK init checks for DSN presence; an absent DSN turns every surface into a no-op without code changes.

---

## 8. PII / compliance notes

All wirings ship with:

- `sendDefaultPii: false` (Sentry does not auto-attach IPs / cookies / user data)
- Authorization-style headers (`Authorization`, `Cookie`, `Set-Cookie`, `X-Api-Key`, `Proxy-Authorization`) scrubbed before transmit
- `signup-worker` additionally scrubs `svix-signature`, `svix-id`, `svix-timestamp` (Clerk webhook HMAC headers)
- 10 % trace sample (keeps free-tier comfortable headroom)
- No session-replay (would breach Sentry Free quota in days + capture PII)

Sentry is listed in `/legal/sub-processors` per the audit §7.3 sub-processor checklist.
