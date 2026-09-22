# Canonical staging target

`topology.json` is the issue #1700 desired state for the load and endurance
workflows. It is declarative and intentionally remains `unprovisioned` until
an owner records provider readback, protected secrets, readiness, and teardown
evidence. It contains names and contract inputs only; it contains no provider
IDs, tokens, or secret values.

## Owner controlled sequence

1. SRE Lead and Security Lead approve the budget, 24 hour lease, and resource
   names. Create only the listed staging resources in the dedicated account and
   `humangr.com` zone.
2. Validate provider returned names and IDs, render a complete isolated
   Wrangler environment, bind only the five `K6_*` names, and record redacted
   readback in `evidence/staging/readiness.json`.
3. Run a bounded readiness probe from `corelink`, then dispatch the five load
   scenarios and the 2 hour endurance run separately. A complete load run may
   seed a baseline only after owner review.
4. Keep the target within its lease and cost cap. Delete synthetic load data
   first, then queues, workers, bindings, and DNS. Teardown is manual and
   requires both listed approvers; missing evidence must stop the action.

The workflows fail closed on a missing secret, a non-canonical host, an
unsupported duration, or an absent baseline. `terraform apply`, Wrangler
deploy, secret installation, and deployment state changes are owner actions;
this repository does not claim they happened.
