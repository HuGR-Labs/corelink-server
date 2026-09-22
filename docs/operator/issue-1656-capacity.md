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

Provider mode accepts an operator captured JSON readback with
`--provider-readback`. The evidence must be marked `read_only`, carry the
account `total_vcpu`, match the runner contract's `vcpu_per_deployment`, and
include a positive `total_memory_mib` field. A missing, malformed, boolean, or
mismatched readback is a failure. Until that readback exists, the verifier
reports the repository declaration as `UNVERIFIED` and does not claim that
Cloudflare's live quota or usage agrees.

## Done criteria

- all five regional blocks match the budget model;
- cache plus runner reservation equals 1,250 vCPU and stays within 1,500;
- credentialless tests run on a GitHub hosted runner;
- provider readback, when supplied, is read-only and matches the model;
- no quota mutation, deployment, secret, or campaign edit is part of this check.
