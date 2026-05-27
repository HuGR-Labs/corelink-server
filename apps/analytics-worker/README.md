# `@corelink/analytics-worker`

CoreLink analytics ingest Worker + weekly digest cron. Phase 0.G of the
[execution plan](../../specs/_audits/2026-05-27-phase-0-execution-plan.md)
closes the measurement loop on the launch-readiness fixes.

## What it does

- `POST /v1/event` — accepts a single event or a batch (`{ events: [...] }`)
  and persists into D1 `analytics_events`. Schema in
  [`src/schema.sql`](src/schema.sql).
- `GET /healthz` — liveness probe; returns 200.
- Monday 08:00 UTC cron — runs
  [`src/views/weekly-three-numbers.sql`](src/views/weekly-three-numbers.sql),
  formats per
  [PLG framework §7.5](../../specs/_audits/2026-05-27-plg-onboarding-framework.md),
  emails Gustavo via Resend.

## Saved SQL views (`src/views/`)

1. `signup-funnel.sql` — landing → signup → CLI auth → first hit (30d)
2. `ttfv-cohort.sql` — median + p75 TTFV by cohort day (60d)
3. `activation-d1-d7-d30.sql` — three activation windows by cohort day
4. `mrr-by-week.sql` — Stripe-derived weekly MRR delta + cumulative
5. `plan-mix.sql` — free / pro / enterprise tenant counts
6. `regional-latency-p99.sql` — per-colo p50/p95/p99 cache-hit latency
7. `weekly-three-numbers.sql` — the Monday email payload

## Local boot

```bash
pnpm install
pnpm --filter @corelink/analytics-worker run schema:apply:dev
pnpm --filter @corelink/analytics-worker run dev
# In another shell:
curl -X POST http://localhost:8787/v1/event \
  -H 'Content-Type: application/json' \
  -H 'Origin: http://localhost:3000' \
  -d '{"id":"01HXTEST000000000000000001","event_name":"signup_started","session_id":"sess-a"}'
```

## Deploy

```bash
wrangler d1 create corelink-analytics-prod    # one-shot, copy ID into wrangler.toml
pnpm --filter @corelink/analytics-worker run schema:apply:prod
pnpm --filter @corelink/analytics-worker run deploy:prod
wrangler secret put RESEND_API_KEY --name corelink-analytics-prod
wrangler secret put INGEST_KEY     --name corelink-analytics-prod
```

## Acceptance (per Phase 0.G §7)

1. `wrangler d1 execute corelink-analytics-prod --command="SELECT name FROM sqlite_master WHERE type='table'"`
   shows `analytics_events`.
2. `curl -X POST https://corelink-analytics.humangr.com/v1/event ...` returns 200 + row in D1.
3. Every file in `src/views/` runs against D1 without error.
4. Plausible dashboard shows non-zero events on `corelink-docs.humangr.com`
   within 1 h of deploy.
5. Cron triggered manually emits an email to `gustavo@humangr.com` even if
   all three numbers are zero.

## Hard rules

- `DIGEST_RECIPIENT` is hard-pinned to `gustavo@humangr.com`. The send
  function refuses any other recipient at runtime (defence in depth on
  top of the wrangler env var) per Phase 0.G §11.
- Events must NOT contain `email`, `ip`, `ip_address`, or `remote_addr`
  fields in `properties` — the ingest handler rejects these at the gate
  (PLG §7.3 privacy rule).
