---
type: "TenancyControl"
title: "Request-quota enforcement"
description: "The container-side mirror of the Worker's monthly request-count cap, closing the OCI pass-through bypass with a fail-OPEN per-tenant allowance limiter."
source_files:
  - "crates/corelink-container/src/request_count.rs"
  - "crates/corelink-eviction/src/tier.rs"
checkpoint_sha: "03c2ae27deb7094fea4009927b90959533dae21e"
provenance: "AUTHORED"
tags: ["tenancy", "quota", "request-count", "oci", "fail-open"]
timestamp: "2026-06-26T00:00:00Z"
---

# Request-quota enforcement

CoreLink enforces three orthogonal abuse axes — velocity (rate), cumulative dollars (the $-ceiling), and
cumulative request count (this control). A tenant can sit under the per-second limit AND under the
$-tripwire and still blow past the contracted monthly request allowance on its signed rate card. The
monthly request cap is normally enforced at the Worker edge, but the OCI surface is a two-leg
pass-through: the Worker forwards `/v2/*` + `/token` raw and returns BEFORE its quota block (it also
strips `x-corelink-tenant-id`), so OCI billable writes were never counted. This module is the
container-side mirror of `checkRequestQuota`, keyed on the same verified-HMAC-bearer tenant the OCI gate
resolves — never a request header.

# Role

This is the count axis of the tenancy abuse triad, complementary to the
[$-ceiling](/tenancy/dollar-ceiling.md) (dollar axis) and the
[storage-quota header](/tenancy/storage-quota-header.md) (bytes axis). Unlike the cost cap it is
deliberately fail-OPEN: it is an SLO-style allowance limiter, so on uncertainty it favours availability
and simply does not count, rather than rejecting a paid tenant during a partial outage. Its caps mirror
the Worker's `QUOTAS[tier].requestsPerMonthMax` byte-for-byte so the two enforcement points agree.

# How it works

- Per-tier caps mirror the Worker's rate card — free 500K up to max 80M requests/month
  (`crates/corelink-container/src/request_count.rs:67-77`).
- `cap_for_tier` resolves the cap from a tier SLUG (string), and — unlike the eviction `Tier` enum — it
  DOES cover the full sold paid ladder: `solo`/`starter`/`pro`(=`org`)/`max` each map to their distinct
  monthly cap; `team`/`enterprise` return `None` (uncapped → skip the counter write entirely) and any
  unknown slug maps to the most-restrictive `free` floor
  (`crates/corelink-container/src/request_count.rs:89-100`). Note the taxonomy seam: `team` is RETAINED
  here as an uncapped legacy slug even though ADR-S19-001 removed it from the sold ladder (and `business`
  never shipped, so it is absent → falls to the `free` floor). So the count axis covers Starter/Pro/Max
  cleanly — and eviction's legacy 5-arm `Tier` enum (see `ops/gc-eviction`) is no longer an open
  coverage gap either: CF-3's `Tier::from_slug` now bridges the sold `starter`/`pro`/`max` slugs onto
  it (`crates/corelink-eviction/src/tier.rs:114-136`).
- `check_and_increment` fail-OPENs (returns `None`, no count) when the wall clock is unavailable
  (`now_ms == 0`) — this is an availability limiter, not a cost cap
  (`crates/corelink-container/src/request_count.rs:253-259`).
- A tier-resolution D1 fault also fail-OPENs, because counting against a fallback `free` cap would
  false-positive a paid tenant during an outage
  (`crates/corelink-container/src/request_count.rs:262-270`).
- The counter is an atomic monthly `increment` keyed on a `YYYY-MM` UTC bucket; the op landing exactly ON
  the cap is still served (reject only `count > cap`)
  (`crates/corelink-container/src/request_count.rs:272-285`).
- Over the cap returns `429 Too Many Requests` with `Retry-After` = seconds until the next UTC month start
  (`crates/corelink-container/src/request_count.rs:287-296`).

# Invariants

- The cap is keyed on the verified-bearer tenant the OCI gate resolved, never a client-supplied header
  (`crates/corelink-container/src/request_count.rs:50-56`).
- Every uncertain path fail-OPENs (allow, no count) — the opposite of the $-ceiling's fail-CLOSED posture,
  and intentional (`crates/corelink-container/src/request_count.rs:31-48`).
- An uncapped tier skips the counter write entirely, so there is no D1 cost for tenants with nothing to
  enforce (`crates/corelink-container/src/request_count.rs:268-270`).

# Gotchas

- The unknown-slug → `free` branch is defence-in-depth and unreachable in practice, because the tier
  strings come from the introspection tier resolver which already filters to the canonical set
  (`crates/corelink-container/src/request_count.rs:79-99`).
- The month bucket is computed to match the Worker's `new Date().toISOString().slice(0, 7)` exactly, so
  the two enforcement points roll their cycles on the same UTC boundary
  (`crates/corelink-container/src/request_count.rs:102-111`).

# Citations

1. `crates/corelink-container/src/request_count.rs:31-48` — the fail-OPEN rationale (allowance limiter, not cost cap).
2. `crates/corelink-container/src/request_count.rs:50-56` — keyed on the verified-bearer tenant, not a header.
3. `crates/corelink-container/src/request_count.rs:67-77` — the per-tier monthly cap constants.
4. `crates/corelink-container/src/request_count.rs:79-99` — `cap_for_tier` defence-in-depth unknown-slug branch.
5. `crates/corelink-container/src/request_count.rs:89-100` — `cap_for_tier` (uncapped tiers → `None`).
6. `crates/corelink-container/src/request_count.rs:102-111` — `YYYY-MM` UTC bucket matching the Worker.
7. `crates/corelink-container/src/request_count.rs:253-259` — clock-unavailable fail-OPEN.
8. `crates/corelink-container/src/request_count.rs:262-270` — tier-fault fail-OPEN + uncapped-tier skip.
9. `crates/corelink-container/src/request_count.rs:272-285` — atomic increment + `count <= cap` allow.
10. `crates/corelink-container/src/request_count.rs:287-296` — over-cap `429` + `Retry-After`.
