---
title: "CoreLink deployment environments"
type: "Runbook"
status: "ACTIVE"
---

# CoreLink deployment environments

The root Worker configuration ships five production targets:

- `prod`
- `prod-sam`
- `prod-lhr`
- `prod-nrt`
- `prod-syd`

The root `wrangler.toml` intentionally has no `[env.staging]` declaration. The
production workflow (`.github/workflows/cf-deploy-prod.yml`) deploys only those
five targets; it has no staging leg and does not promote artifacts from staging
to production. There is no automatic staging-to-production promotion.

This is a configuration-truth statement, not a claim that no staging system can
ever exist. Test harnesses, other Workers, and owner-provisioned external
staging-equivalent infrastructure may retain their own staging controls. They
are outside the root Worker deployment path and must not be described as a
shipped staging-to-production promotion flow.

Before adding a root staging environment, provision its independent resources,
secrets, workflow wiring, and promotion contract in a separately reviewed
change. Until then, production deploys remain governed by the five-target
matrix and the gates in `cf-deploy-prod.yml`.

The exact unprovisioned desired state is tracked in
`infra/staging/topology.json`. It names a dedicated Worker/container, D1, R2,
KV, DSR queue/DLQ, and service bindings for
`https://staging.corelink.humangr.com`; none may inherit or reuse dev/prod
resources. The file intentionally contains no provider IDs or secret values.
Only after provider creation returns the IDs and every listed secret name is
bound may an `[env.staging]` block be rendered into `wrangler.toml`.
