---
id: "RB-PAT-SIGNING-KEY-ROTATION"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-09-01"
updated: "2026-09-01"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "pat", "signing-key", "rotation", "b-081", "blocked"]
---

# RB-PAT-SIGNING-KEY-ROTATION — PAT signing-key rotation

**Status: BLOCKED / DO NOT EXECUTE.** The code can forward overlap keys at a
container's next start, but the current control plane cannot enumerate, recycle,
or prove the boot generation of the active per-tenant **and shared `_oci`**
container population. Until those prerequisites exist, following this ritual
would create mixed key generations and can cause a PAT-auth outage. B-081
remains open.

## Invariant

The verifier accepts the current key plus the optional `PAT_SIGNING_KEY_PREV`
and `PAT_SIGNING_KEY_NEW` siblings. The minter uses only
`PAT_SIGNING_KEY`. Rotation must preserve `INV-KEY-OVERLAP` and
`INV-KEY-NO-SKIP`: no active container may skip directly from OLD-only to
NEW-only, and the 24-hour overlap clock starts only after every member of the
active population proves the promoted boot generation.

Never record secret values in tickets, logs, commands captured as evidence, or
this runbook.

## Population and anti-vacuity control

The population `P` is every PAT-authenticating `CoreLinkServer` instance in all
five Worker environments:

- `prod` (IAD)
- `prod-sam`
- `prod-lhr`
- `prod-nrt`
- `prod-syd`

For each environment `e`, `P_e` is the union of every active per-tenant instance
and the exactly-one shared `_oci` instance. `_oci` is persistent, not a tenant
alias: the Worker routes OCI `/token` (Basic PAT verification) and `/v2/*` to
`idFromName("_oci")`, so omitting it leaves a PAT-authenticating container on a
previous generation even if every tenant instance was recycled. Before each
phase, retain an independent active-tenant count, an enumerated tenant-instance
count, and an explicit `_oci` record for every environment. A proof of `0/0`, a
tenant-only result, or a five-region `_system` recycle result is not population
evidence. The existing `/_internal/admin/recycle-system` endpoint reaches only
the five regional `_system` instances and MUST NOT be used as evidence for `P`.

The adjacent contract is parsed by
`worker/tests/pat_rotation_forward.test.ts`; removing `_oci`, changing its
per-environment cardinality, or treating `_system` as population evidence makes
that test fail.

```json rotation-population-contract
{
  "all_pat_authenticating_instance_classes": [
    { "name": "active-per-tenant", "per_environment": "enumerated" },
    { "name": "_oci", "per_environment": "exactly-one", "routes": ["/token", "/v2/*"] }
  ],
  "not_population_evidence": ["_system"]
}
```

## Required controls before unblocking

All of the following must exist and be tested before this runbook is executable:

1. A complete, authorization-protected enumerator for `P`, with an independent
   active-tenant anti-vacuity count, per-environment results, and an explicit
   exactly-one `_oci` record in every environment (even when the tenant count is
   zero).
2. An idempotent recycle operation for exactly that enumerated generation, with
   a durable per-member success/failure ledger and bounded retries.
3. A non-secret boot-generation attestation that identifies which rotation
   phase a running container loaded.
4. Old-key and new-key PAT probes through both the public edge and the native
   container path in every environment, including OCI `/token` against each
   `_oci` instance.
5. A tested rollback that can restore the preceding overlap generation without
   allowing any member of `P` to skip a generation.

If any member is missing, unobservable, or fails a probe, stop. Do not advance
the key state and do not start the overlap clock.

## Safe three-generation ritual

### G-stage — make NEW acceptable, keep OLD minting

1. Generate NEW offline according to the secrets checklist.
2. Bind `PAT_SIGNING_KEY_NEW=NEW` while keeping `PAT_SIGNING_KEY=OLD` in each of
   the five environments. Do not assume secrets inherit across environments.
3. Enumerate `P`, record the anti-vacuity control, recycle that exact generation,
   and prove every member booted G-stage. The record MUST include one `_oci` in
   each environment; its OCI `/token` OLD/NEW results, recycle result, and boot
   attestation are mandatory.
4. Through edge and native paths in all five environments, prove OLD and NEW
   PATs both verify, including OCI `/token` on `_oci`, and prove newly minted
   PATs still use OLD.

### G-promote — mint NEW, keep OLD acceptable

1. In each environment bind `PAT_SIGNING_KEY=NEW`,
   `PAT_SIGNING_KEY_PREV=OLD`, and remove `PAT_SIGNING_KEY_NEW` only as part of
   the same controlled phase.
2. Re-enumerate `P`, record the control, recycle that exact generation, and prove
   every member booted G-promote, including exactly one `_oci` in every
   environment.
3. Prove newly minted PATs use NEW and both OLD and NEW verify through both
   paths in all five environments, including OCI `/token` on `_oci`.
4. Start the 24-hour overlap clock only after 100% of `P` has passed; that
   condition is false if any environment lacks its `_oci` recycle, boot-generation
   attestation, or OLD/NEW probe result.

### G-retire — reject OLD

1. After the full overlap and revocation window, keep
   `PAT_SIGNING_KEY=NEW` and remove both sibling bindings in every environment.
2. Re-enumerate `P`, record the control, recycle that exact generation, and prove
   every member booted G-retire, including exactly one `_oci` in every
   environment.
3. Prove NEW succeeds and OLD is rejected through both paths in all five
   environments, including OCI `/token` on `_oci`. Archive only generation IDs,
   counts, timestamps, and probe outcomes — never key material.

## Current blocker

The repository has only an `_system`-scoped recycle fan-out. It has no complete
per-tenant-plus-`_oci` population enumerator, no exact-population recycle ledger,
and no boot-generation attestation for PAT key state. Consequently no operator
can prove G-stage, G-promote, or G-retire across `P`; B-081 must remain open and
this runbook must remain non-executable until those controls are implemented and
validated in all five environments.
