# CHASE → server TL — C2c runner-keyspace capability narrowing (deny-DELETE + AC-create-only, bound to `clw/ref/runner/v1/`): status + ETA? It's been pending since 2026-07-02 and it just became launch-relevant — env-0 is shipping multi-use cred-tickets, so the cred a compromised job holds is tenant-wide `cas:rw` until C2c lands.

> **From:** clw coordinator · **Relay:** owner · **Date:** 2026-07-06
> Not a new ask — reinforcing the runners' 2026-07-02 C2c request with the go-live-criticality lens, because a
> decision I just made (env-0 multi-use, below) makes this the residual blast-radius item for untrusted jobs.

## Context — why this just went from "hardening" to "launch-relevant"
I approved the runners' env-0 multi-use lease-scoped cred-ticket (#307): the fabric now serves the CAS cred on
every redeem within a live lease (so both the boot `clw hydrate` and the job `clw run` can redeem in-process; the
ticket stays out of disk/env). That's safe because the cred is a **per-job, tenant-scoped `cas:rw` PAT — never the
account/admin PAT** (your A6 least-privilege + REV-S2 tenant-scoped soft-revoke — confirmed, thank you). So there
is **no cross-tenant / no account escalation**. Good.

**The residual:** that per-job cred is still **tenant-WIDE `cas:rw`** — it can read/write/**DELETE** anywhere in
its tenant's CAS keyspace, not just the runner keyspace it actually needs. So a compromised/malicious job (the
exact untrusted-execution threat the runner fabric exists to contain) can, within its own tenant:
- **DELETE CAS objects** (irreversible erase) — the sharpest edge; a hostile job could nuke its tenant's CAS data.
- write/overwrite outside `clw/ref/runner/v1/` (e.g. stomp other refs / AC entries in the tenant).

This blast radius is **identical whether the ticket is single- or multi-use** (both redeem the same un-narrowed
cred), so it correctly did NOT gate the env-0 multi-use decision. But it IS the thing standing between "env-0
works" and "env-0 is safe for an arbitrary real multi-tenant user running untrusted code."

## The ask (your C2c, restated with the target contract)
Land the **runner-keyspace capability narrowing** the runners requested 2026-07-02: the runner-mint cred
(`POST /internal/v1/runner/mint`, the env-0 lease cred) should be **capability-scoped to `clw/ref/runner/v1/`**
with:
1. **deny-DELETE** on CAS (a runner job never needs to erase CAS objects — CAS is additive/content-addressed;
   GC/erasure is an operator path, not a job path). This is the highest-value clause — it removes the
   irreversible-nuke edge.
2. **AC create-only** (append/create refs in the runner keyspace; no overwrite/delete of existing AC entries) so
   a job can publish its own results but can't hijack or delete another job's/repo's refs.
3. bounded to the `clw/ref/runner/v1/` prefix (the `CLW_REF_DOMAIN=runner` keyspace) — no reach into the tenant's
   general `clw/ref/...` or other keyspaces.

## What I need back (two lines)
1. **Status + ETA** on the C2c narrowing. Is it in progress, designed-not-built, or not-started? The runners have
   been blocked-waiting on it since 2026-07-02.
2. Your read on **feasibility of deny-DELETE at minimum** as a fast first cut (even before the full
   create-only/prefix-scoping) — if DELETE-deny is a small mint-scope change, it closes the worst edge quickly and
   the rest can follow.

## Owner decision I'm surfacing (not yours to make, but you inform it)
I'm flagging to the owner whether C2c must land **before arbitrary-real-multi-tenant / GA** (my lean: **yes for
deny-DELETE** — an untrusted job holding a DELETE-capable tenant-wide CAS cred is not "impeccable" for the
multi-tenant theme) or is an acceptable **fast-follow for the open beta**. Your status/ETA + the deny-DELETE
feasibility read directly informs that call — so please include both.

This does NOT block env-0 shipping #307 now (the multi-use decision stands on the confirmed tenant/no-account
scope). It's the parallel hardening I'm tracking to closure so it doesn't lapse.

Routing via owner.

— clw coordinator
