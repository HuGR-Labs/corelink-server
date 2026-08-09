# Container region & cost policy

**Decision (2026-08-09, owner): run ONE region — `corelink-prod` (iad) — for now.**
The four regional envs (`sam`, `lhr`, `nrt`, `syd`) stay at `max_instances: 0`
(off) until there is real customer traffic in that region to justify them.

## Why — the cost model that drives this

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
cost numbers below as an UPPER BOUND (assumes 24/7 awake), not a measurement.

## Cost table (upper bound: instances awake 24/7)

Rate: memory $0.0000025 / GiB-s, disk $0.00000007 / GB-s. 1 instance
(4 GiB / 8 GB) ≈ **$27/month** if never sleeping.

| config | hot instances | $/month (upper bound, zero traffic) |
|---|---|---|
| **1 region (iad), `max_instances: 2`** ← current policy | 2 | **~$54** |
| 5 regions, `max_instances: 2` | 10 | ~$270 |
| 5 regions, `max_instances: 20` (pre-2026-08-08) | 35 | ~$945 |
| everything off (`max_instances: 0`) | 0 | $0 |

## How to apply

Region app IDs (account `6a1fc1c626fc2628823e60b9db01f5cd`):

| region | app id | policy |
|---|---|---|
| prod (iad) | `a033572c-0803-4866-b3a3-61f4812843b1` | **ON, `max_instances: 2`** |
| sam | `a0337243-13cb-46ef-adb1-781294294404` | off (`0`) |
| lhr | `a03faa1b-70c4-40eb-aa38-9f70d43de992` | off (`0`) |
| nrt | `a033417c-db96-467b-a65c-83951d1fa5d1` | off (`0`) |
| syd | `a030dd8d-c24c-41bb-bf03-a1fb3542eac0` | off (`0`) |

```bash
. ./.env.local; A=6a1fc1c626fc2628823e60b9db01f5cd
# turn ON iad at max_instances=2:
curl -s -X PATCH -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" -H "Content-Type: application/json" \
  -d '{"max_instances":2}' \
  "https://api.cloudflare.com/client/v4/accounts/$A/containers/applications/a033572c-0803-4866-b3a3-61f4812843b1"
# the four regionals stay at 0 — no action needed while off.
```

⚠️ A `wrangler deploy` re-reads `wrangler.toml`, where `max_instances = 20` /
`10` is declared per env — **a deploy will reset these overrides**. After any
prod deploy, re-apply the API override above (or change the `wrangler.toml`
values if this policy becomes permanent).

## When to add a region

Add a region (flip its app to `max_instances: 2`) when that region shows real
sustained customer traffic — i.e. paying the ~$27+/instance/month there buys
lower latency for actual users. Until then, one region serves everyone
(higher latency for far users, but ~$54/mo instead of ~$945/mo).

See also: memory note `container-idle-burn-7-per-region`, and
`docs/operator/2026-07-08-go-live-runbook.md`.
