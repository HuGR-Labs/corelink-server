# Staging provisioning boundary

`topology.json` is the canonical, unprovisioned desired state for B-029 and
B-070. It is intentionally separate from `wrangler.toml`: Wrangler environment
tables do not inherit resource bindings safely, and deploying a partial table
would bind the staging Worker to top-level development resources.

## Ordered provisioning plan

1. In the existing Cloudflare account and `humangr.com` zone, create exactly
   the D1 database, four R2 buckets, three KV namespaces, DSR queue and DSR DLQ
   named in `topology.json`. Do not substitute an existing dev/prod resource.
2. Capture the provider-returned D1 database ID and KV namespace IDs. Verify
   every returned resource name against the contract before rendering config.
   **Completed 2026-09-09:** the D1, R2, KV, queue and DLQ resources recorded in
   `topology.json` were created in the existing account and read back empty.
3. Render a complete `[env.staging]` in `wrangler.toml` in one reviewed change:
   explicit `name`, `workers_dev = false`, canonical custom-domain route,
   container, vars, D1, R2, KV, all six Durable Object bindings, the synthetic
   receiver service binding, observability, and an empty cron list. No binding
   may be inherited from the top-level dev configuration.
   Render every `root_worker_settings.vars` entry explicitly; provider-derived
   values replace their typed `provider_output` references only after readback.
4. Render the dedicated `corelink-signup-staging` environment with both D1
   aliases (`CONFIG_DB` and `BILLING_DB`) pointing to the same
   `corelink-config-staging` database, its `CORELINK_API_SVC`
   binding, staging-safe SLA gates, hourly trigger, and the staging-only DSR
   producer/consumer/DLQ names.
5. Bind every secret name listed in the contract to its named Worker, including
   both synthetic-receiver secrets (`PAGERDUTY_SYNTHETIC_ROUTING_KEY` and
   `PAGERDUTY_WEBHOOK_SECRET`). Values
   come from the existing secret stores and never enter git, command output,
   evidence, or Terraform state.
6. Deploy the synthetic receiver, root Worker/container, and signup Worker in
   dependency order. Apply D1 migrations to the staging database only after
   both writer configurations are frozen.
7. Add the proxied custom-domain route for exactly
   `staging.corelink.humangr.com`; verify DNS, TLS, `workers_dev = false`, Worker
   health, container health, D1 migration head, R2 isolation, queue consumer,
   DLQ consumer, and service-binding reachability.
8. Create/protect the GitHub `staging` environment and bind its five required
   `K6_*` secrets. `K6_TARGET_HOST` must be exactly
   `https://staging.corelink.humangr.com` (one trailing slash is normalized).
9. Dispatch the load suite once. Only a complete five-scenario green run may
   seed the baseline; then perform the separately approved cadence decision.
10. Change `deployment_state` only in the same reviewed commit that records the
    complete rendered configuration and provider readback evidence.

## Current blockers

- DNS/custom-domain routing and TLS for the canonical host are not evidenced.
  The current Wrangler OAuth token can administer Workers, D1, R2, KV and
  Queues, but provider reads of DNS records and zone TLS settings fail with
  Cloudflare codes `10000` and `9109`; no DNS mutation is authorized by it.
- The named Worker secrets and the five GitHub-environment K6 secrets are not
  evidenced as bound. Secret values are deliberately outside this repository.
- No staging image/deploy, migration readback, health proof, or baseline run is
  evidenced yet.

Until all three blockers are resolved, `deployment_state = "unprovisioned"`,
B-029 remains parked, and repository checks reject any partial `[env.staging]`.
