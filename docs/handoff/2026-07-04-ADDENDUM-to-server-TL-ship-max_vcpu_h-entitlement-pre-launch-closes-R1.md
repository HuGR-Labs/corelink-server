# ADDENDUM → corelink-server TL — ship the `max_vcpu_h` entitlement value pre-launch (closes R1; the runners armed the ceiling but it reads 0/unlimited without your vector)

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-04

The runners TL ARMED the vCPU-hour ceiling on a durable pg ledger (#286: Neon, `FABRIC_RUNNER_VCPU=4`,
`FABRIC_LEDGER_BACKEND=pg`, proven live). But on the `corelink` auth path the **enforced ceiling value comes from
the introspect `max_vcpu_h` entitlement** — and until you ship that vector it reads **0/unlimited**, i.e. the
ceiling is armed but not really bounding anything. Under the no-waiver bar an unbounded ceiling is a loose end.

**Ask (pre-launch):** ship the `max_vcpu_h` entitlement in the runner introspect vector (alongside
`runners_entitlement` / `max_concurrency`), so the fabricd ceiling enforces a real per-tenant vCPU-hour bound at
launch. Small addition to the entitlement you're already wiring for cf-multitenant fairness. This closes R1.

Also confirming from the runners' WP5 sign-off: build `handleRunnerMint`'s WP5 with **`ac_output_name` optional**
(the autoscaler webhook has no output name → deny-DELETE + create-only fallback, which meets the poison/evict
guarantee). The exact-key restriction is a flagged follow-up (owner's call), not pre-launch.
— clw coordinator
