# Container region & cost policy

**Historical decision (2026-08-09, owner): run ONE region — `corelink-prod` (iad).**
This is **not current deployed state**. Read-only Cloudflare application queries on
2026-09-13 found all five cache applications at `max_instances: 200`, with 15
reported instances and 0 `active` in each of `sam`, `lhr`, `nrt`, and `syd`;
`prod` had 16 reported instances and 1 `active`. The former four-region `0` override and
the `prod=2` override are no longer installed. Do not execute this old policy
as a production change without a new owner decision and a current cost readback.

## Why — the cost model that drives this

The following is the **2026-08-08 rationale for the historical decision**, not
a current price sheet or a direction to reconfigure production.

Cloudflare Containers does **not** bill like classic serverless. Memory + disk
are billed on **provisioned** instances, not on requests. When an app is turned
on (`max_instances > 0`), the platform pre-schedules a pool of "ready"
instances **per region, without any customer traffic**, and those provisioned
instances accrue memory + disk charges until they sleep. CPU is the only
request-proportional cost, and it is cheap.

Measured 2026-08-08 (single-region experiment on `syd`, via the CF Containers
per-application API + a `wrangler tail` control):
- Turning an app on with no requests immediately brought up **7 instances**
  (`inst=7 active=0 healthy=7`), with **zero** Worker/DO invocations in the tail
  (a control request confirmed the tail was live). ⇒ platform pre-warm pool, not
  our code.
- The pool size **scales with `max_instances`**: `max_instances=3` → 3
  instances; `max_instances=20` → 7. So `max_instances` is the cost lever.
- ⚠️ `max_instances=1` **takes the product down** (requests 503/timeout). Use ≥2.

## ⚠️ Open question — NOT yet confirmed

Whether the pre-warm pool **stays awake 24/7 (billing continuously)** or **sleeps
after inactivity (billing stops)** is **unconfirmed**. Our container uses the raw
container binding + a custom alarm reaper, **not** the `@cloudflare/containers`
SDK `sleepAfter` auto-sleep, and the platform pre-warm pool does not pass through
our Durable Object (so PR #1060's `setInactivityTimeout` does not reach it).

**Source of truth = the Cloudflare billing dashboard** (Billing/Usage →
Containers) for a period when prod was on. Until someone reads that, treat the
cost numbers below as a **historical scenario**, not a measurement or a current
upper bound. The current application configuration reports 1 GiB memory/4 GB
disk per cache instance, not the 4 GiB/8 GB assumed in the old table.

## Historical cost scenarios (2026-08-09 assumptions; not current prices/config)

Historical assumed rate: memory $0.0000025 / GiB-s, disk $0.00000007 / GB-s. 1 instance
(4 GiB / 8 GB) ≈ **$27/month** if never sleeping.

| historical config | hot instances | historical $/month (upper bound, zero traffic) |
|---|---|---|
| 1 region (iad), `max_instances: 2` | 2 | ~$54 |
| 5 regions, `max_instances: 2` | 10 | ~$270 |
| 5 regions, `max_instances: 20` (pre-2026-08-08) | 35 | ~$945 |
| everything off (`max_instances: 0`) | 0 | $0 |

## Current readback and decision boundary

Region app IDs (account `6a1fc1c626fc2628823e60b9db01f5cd`):

| region | app id | 2026-09-13 live `max_instances` / instances |
|---|---|---|
| prod (iad) | `a033572c-0803-4866-b3a3-61f4812843b1` | 200 / 16 |
| sam | `a0337243-13cb-46ef-adb1-781294294404` | 200 / 15 |
| lhr | `a03faa1b-70c4-40eb-aa38-9f70d43de992` | 200 / 15 |
| nrt | `a033417c-db96-467b-a65c-83951d1fa5d1` | 200 / 15 |
| syd | `a030dd8d-c24c-41bb-bf03-a1fb3542eac0` | 200 / 15 |

The 2026-09-13 account API confirmed `total_vcpu=1500`, `usage=null`. Five
cache apps reserve 250 vCPU by their declared ceilings. The main runner app
reserves 1000; runner dev, checkhost, and fabricd add 45. Across all eleven
listed account applications (two have `max_instances=0`), the declared ceiling
is **1295/1500**, not the 1250 cache+main-runner subtotal. A ceiling sum is
neither concurrent tenant occupancy nor measured/billable usage. See the dated
aggregate capture in `evidence/owner-actions/B-097/cloudflare-vcpu-quota-case.json`.

`wrangler deploy` can reset manual application overrides to `wrangler.toml`.
Re-read the live API after every deployment; do not assume historical overrides
remain installed. Any change to these ceilings requires a separate reviewed
capacity/cost decision, not an automatic application of this historical note.

## When to add a region

This historical rule cannot be applied as written: all five regional
applications are already configured with nonzero ceilings. Decide any regional
capacity or routing change from a fresh traffic, billing, and residency
readback, then document approval and rollback before changing production.

See also: memory note `container-idle-burn-7-per-region`, and
`docs/operator/2026-07-08-go-live-runbook.md`.
