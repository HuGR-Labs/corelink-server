---
title: "B-086 D1 residency read-only evidence"
status: "READ_ONLY_CAPTURE"
captured_at: "2026-09-09T06:10:15Z"
owner_decision: "UNRESOLVED"
---

# B-086 — D1 residency read-only evidence

This is an evidence capture, not a legal opinion and not a production change.
It records the distinction between Cloudflare API facts, repository bindings,
and the owner/counsel decision required by B-086. No D1 database, Worker
binding, DPA text, or signed instrument was mutated while collecting it.

## Result

The local Cloudflare OAuth profile is usable for read-only inspection. It is
also authorized for D1 writes, so the credential must not be used to create or
rebind a database without an explicit owner/provider change window. The
existing production D1 is one global database:

- Wrangler inventory: `corelink-prod-d1`,
  `d64742ea-e102-40b2-a844-ff02e3f94562`.
- `wrangler d1 info`: `running_in_region=ENAM`, `jurisdiction=null`,
  `num_tables=110`, `database_size=242 MB`, `read_replication.mode=auto`.
- Cloudflare D1 inventory reported the same UUID once and no jurisdiction for
  the database.
- The repository verifier found five production `CONFIG_DB` bindings and one
  distinct `database_id`; the DPA table still contains the active
  `Infrastructure: Workers, R2, D1, KV, Durable Objects, Custom Domains` /
  `Tenant-pinned (Section 7)` claim.
- A read-only aggregate over `tenant.primary_region` returned `apac=1`,
  `enam=155`, `wnam=108`; this is an application row distribution, not proof
  of physical or legal residency.

Therefore B-086 remains `open`/`unresolved`. The API can provision a new D1
with `--jurisdiction eu` (or `fedramp`), but a production alignment still
requires an owner/counsel choice between a jurisdictional D1 cutover and an
executed legal amendment. Neither choice is made by this evidence.

## Repository measurement

The five active production environments are `prod`, `prod-sam`, `prod-lhr`,
`prod-nrt`, and `prod-syd`. Each binds `CONFIG_DB` to the same UUID. The
jurisdiction keys in `wrangler.toml` are two R2 bindings (`prod-lhr`), not D1
bindings. The DPA amendment is explicitly `PENDING_LEGAL_REVIEW` and its
Cloudflare row still says `Tenant-pinned (Section 7)`.

Related legal surfaces are not interchangeable: the active registry in
`legal/sub-processors.md` explicitly discloses “D1 control-plane metadata
global under SCC/TIA safeguards”, while `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md`
and `legal/tia-template.md` describe Cloudflare’s service as tenant-pinned
without a D1-specific qualification. This evidence records those texts; it
does not select which instrument controls or silently amend any of them.

Focused checks run against the current tree:

```text
B-086 open: production_d1_bindings=5, distinct_database_ids=1,
active_dpa_d1_tenant_pinned_claims=1, jurisdiction_keys=2 (R2 only);
owner/counsel evidence remains pending
owner-action packet: PASS: 29 item(s), closed population verified
```

## Cloudflare/D1 read-only observations

The commands below were run with Wrangler `4.111.0` and the local OAuth
profile. They expose no token, email, tenant ID, or row-level personal data.
`wrangler d1 execute` was limited to `sqlite_master`, `PRAGMA table_info`, and
an aggregate `GROUP BY`; all returned `changed_db=false` and
`rows_written=0`.

```text
CI=1 pnpm exec wrangler whoami
Account ID: 6a1fc1c626fc2628823e60b9db01f5cd
Token D1 scope: write (credential capability; not exercised)

CI=1 pnpm exec wrangler d1 list
6011a014-718d-4901-928a-1c9ac3acb558  corelink-t9w1-authority-20260907-0047  jurisdiction=""
d7fe391f-9fe0-4544-a1fb-64747bcd2639  corelink-analytics-prod               jurisdiction=""
d64742ea-e102-40b2-a844-ff02e3f94562  corelink-prod-d1                       jurisdiction=""
...

CI=1 pnpm exec wrangler d1 info corelink-prod-d1
running_in_region     ENAM
jurisdiction          null
num_tables            110
database_size         242 MB
read_queries_24h      241
write_queries_24h     0
rows_written          0 (on the read-only request)
read_replication.mode auto

SELECT primary_region, COUNT(*) AS tenant_count
FROM tenant GROUP BY primary_region ORDER BY primary_region
apac  1
enam  155
wnam  108
```

The `sqlite_master` read showed control-plane tables including `tenant`,
`team_member`, `pat`, `tenant_billing`, `tenant_quota`, and `audit_outbox`.
This confirms that the D1 scope is material to the residency decision; it does
not establish where every row is physically stored.

## Feasibility and safe next action

The installed CLI exposes the following remote create options:

```text
wrangler d1 create <name> --location <weur|eeur|apac|oc|wnam|enam>
                       --jurisdiction <eu|fedramp>
```

Cloudflare states in the CLI help that a jurisdiction restricts where the D1
runs and stores data, and that a jurisdiction overrides the location hint.
The create command is remote and has no dry-run flag. It was **not** run.
The corresponding Cloudflare documentation says jurisdiction is set only at
database creation and cannot be added or updated later:
<https://developers.cloudflare.com/d1/configuration/data-location/>.

After counsel/owner selects `jurisdictional_d1`, the reversible preparation
sequence is:

1. Create a separately named, jurisdiction-scoped shadow D1 only in an
   approved provider change window (`--jurisdiction eu` for WEUR).
2. Apply and verify the reviewed migration set against the shadow database;
   do not point a production Worker at it yet.
3. Produce a row-count/schema/DSR and audit-integrity comparison without
   exporting row-level personal data.
4. Submit a reviewed binding/cutover change with an explicit rollback to the
   current UUID before any write path is switched. Record the effective date,
   provider case, and owner approval.

Creating the shadow database is an external mutation and therefore remains
pending. If counsel chooses `legal_amendment`, no D1 provisioning is needed;
the executed superseding instrument and its effective date must be captured
instead. A draft or this read-only receipt cannot close B-086.

## Source links and hashes

The evidence is bound to these source files at capture time:

```text
wrangler.toml                                      a7230342badb5dfedabc8bce6911a7c6926caa2730c27f4316f740a172f75ec2
legal/dpa-residency-amendment.md                   380667ad98b644fe9ed1e2057353c3ba11f687f9420ab9cdc496e705a83965fb
scripts/verify_b086_d1_residency.py                6b364b3453b3dd7efc2382929ee2b56f06470ae0f48353389954082fdf10a9c1
docs/handoff/2026-09-05-owner-action-packets-b008-b154.json
                                                     c4d47f039beef033564029ee32a785cddae73c3700b86ead6e7c1452fc0d0f8e
legal/sub-processors.md                             a3c5f278d8678b7a4aaa9bfad893760b2ec998374c5ce328458f0af34225e491
legal/dpa/SUB-PROCESSOR-COMMITMENTS.md               c0c7f3ddd648bf823c66fdf83bbada0f88e5072bb71c37c4b2c33497c8be7350
legal/tia-template.md                                f4966b0697fb63bc80cb79314da2f5a60f26a6507b3cdb2aff0552c3d27cb733
```

The corresponding structured receipt is
`evidence/owner-actions/B-086/d1-residency-resolution.json`.
