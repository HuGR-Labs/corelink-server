# Issue 1656: regional capacity verification

The repository budget is five production container environments (`prod`,
`prod-sam`, `prod-lhr`, `prod-nrt`, and `prod-syd`). Each declares one
`CoreLinkServer` container with `instance_type = "basic"` and
`max_instances = 200`. The trusted mapping for `basic` is 0.25 vCPU, so the
cache reservation is 5 × 200 × 0.25 = 250 vCPU.

The runner reservation is an external contract: 250 `standard-4` instances at
4 vCPU each = 1,000 vCPU. The combined declared reservation is therefore
1,250 vCPU. The budget model records the documented account limit of 1,500
vCPU and 250 vCPU of arithmetic headroom. This is a reservation calculation,
not a provider quota grant or a live usage measurement.

## Verification contract

`python3 scripts/verify_regional_capacity.py` reads only `wrangler.toml` and
`config/capacity/regional-vcpu-budget.json`. It has no credentials, does not
invoke `wrangler`, and never changes provider state. It fails closed when a
region is missing, has more than one container, changes class, uses an unknown
instance type, or changes the reservation arithmetic.

Provider mode accepts only the redacted #2044 Cloudchamber receipt emitted from
`GET /accounts/{account}/cloudchamber/me` with `--provider-readback`. It
requires its exact schema, issue, endpoint, `read_only` marker, and finite
positive `quota.total_vcpu`, `quota.vcpu_per_deployment`, and
`quota.total_memory_mib` aggregates. The quota must match the declared runner
vCPU contract. The receipt must keep `usage` and `concurrency` explicitly
`unavailable`; the endpoint establishes neither measurement. A missing,
malformed, boolean, nonfinite, mismatched, or measurement-claiming readback is
a failure. Until a protected GET produces that receipt, the verifier reports
the repository declaration as `UNVERIFIED` and does not claim that Cloudflare's
live quota or usage agrees.

The verifier accepts no provider-defined extensions. Its allowed top-level
receipt keys are the contract fields plus only the #2044 probe metadata
`captured_at`, `account_id_redacted`, and `provider_api_version`; that metadata
is optional because this arithmetic verifier does not consume it. `quota` has
exactly `total_vcpu`, `vcpu_per_deployment`, `memory_mib_per_deployment`, and
`total_memory_mib`. Fields such as `tenant_concurrency` or `measured_usage`
are rejected at either level until a separately reviewed provider contract
defines their source and semantics.

## Done criteria

- all five regional blocks match the budget model;
- cache plus runner reservation equals 1,250 vCPU and stays within 1,500;
- credentialless tests run on a GitHub hosted runner;
- provider readback, when supplied, is read-only and matches the model;
- no quota mutation, deployment, secret, or campaign edit is part of this check.
