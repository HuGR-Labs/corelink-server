### Removed

- # B-070 — remove the unshipped root-worker staging declaration

  The root `wrangler.toml` no longer declares `[env.staging]`. No root-worker staging deployment was wired or shipped, so the configuration no longer claims that canary/rollout artifacts move from staging to production. The production workflow continues to deploy the five existing production targets (`prod`, `prod-sam`, `prod-lhr`, `prod-nrt`, `prod-syd`).

  The declaration was removed together with the no automatic staging-to-production promotion claim.

  This does not remove staging support from unrelated Workers, test harnesses, or
  owner-provisioned external environments. Those surfaces remain separately
  scoped and are not a root-worker staging-to-production promotion path.

  The future root staging topology is now a fail-closed, unprovisioned contract
  at `infra/staging/topology.json`; this replaces the later partial receiver-only
  `[env.staging]` regression without pretending that provider resources exist.
