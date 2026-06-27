---
type: "TenancyControl"
title: "Storage-quota / byte-accounting header"
description: "The Worker-trusted storage-cap header and the atomic byte-accounting that makes the per-tier storage quota live, plus the `X-Rate-Limit-Type` over-quota response taxonomy."
source_files:
  - "crates/corelink-container/src/byte_accounting.rs"
  - "crates/corelink-rate-headers/src/headers.rs"
  - "crates/corelink-adapter-host/src/oci/auth.rs"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["tenancy", "quota", "storage", "byte-accounting", "rate-limit-type", "fail-closed"]
timestamp: "2026-06-26T00:00:00Z"
---

# Storage-quota / byte-accounting header

The `tenant_storage_state` row carries the authoritative `bytes_used` counter the eviction worker and the
storage-quota policy read — but on the container data plane NOTHING incremented it: a successful CAS/AC/
Turbo write committed bytes to R2 and returned success without touching `bytes_used`, so the storage cap
was structurally inert (a Free tenant could store unbounded TB at `$0`). This control closes that gap.
The Worker — the quota-resolution authority — sets a server-trusted header carrying the tenant's resolved
per-tier storage cap, and after a successful write the billable handler does an ATOMIC check-and-accrue
against `tenant_storage_state`. Over-cap or unaccountable writes fail CLOSED; the public over-quota
boundary is surfaced to clients through the `X-Rate-Limit-Type` response-header taxonomy (the sprint
contract §5 reason vocabulary — NOT "RFC 9331", which is the unrelated ECN/L4S RFC).

# Role

This is the bytes axis of the tenancy abuse triad, alongside the
[$-ceiling](/tenancy/dollar-ceiling.md) (dollar axis) and the
[request-quota](/tenancy/request-quota.md) (count axis). The header is the seam between the Worker's
quota resolution and the container's enforcement *on the native surfaces*; the accountant is the
enforcement; the rate-headers crate is the customer-facing signal that an over-plan boundary (not a bug)
caused the rejection.

**The header is NOT the only cap-delivery seam — OCI is the exception.** The Worker DELETES
`x-corelink-tenant-id` (and therefore never sets `x-corelink-storage-quota-bytes`) on the OCI
pass-through, so on the OCI surface the resolved per-tier cap travels INSIDE the verified OCI bearer
token instead: the cap is signed into the token at mint time (`encode_cap`,
`crates/corelink-adapter-host/src/oci/auth.rs:285-303`) as the `storage_cap_bytes` field of the
HMAC-verified `VerifiedToken` (`:200-220`), decoded fail-closed only after the signature check
(`decode_cap`, `:250-263`, `:350-359`). It uses the SAME sentinel vocabulary as the header — `Some(n)`
finite cap, `Some(0)` genuine-unlimited, `None`/`"-"` indeterminate → data-plane fail-closed — so the two
delivery paths agree. On OCI an ABSENT `x-corelink-storage-quota-bytes` header is therefore NORMAL, not an
attack: the cap simply rides the bearer.

# How it works

- `STORAGE_QUOTA_HEADER` (`x-corelink-storage-quota-bytes`) is set SOLELY by the Worker and stripped from
  any client-supplied value, exactly like `x-corelink-tenant-id`
  (`crates/corelink-container/src/byte_accounting.rs:85-94`).
- `storage_quota_from_headers` parses it fail-CLOSED: absent/empty/non-ASCII/non-`i64`/negative → `None`
  (indeterminate), `"0"` means genuine-unlimited
  (`crates/corelink-container/src/byte_accounting.rs:96-110`).
- After a successful write the handler calls `ByteAccountant::accrue`
  (`crates/corelink-container/src/byte_accounting.rs:217-230`), which runs the atomic DB-side
  check-and-accrue UPSERT (`bytes_used = bytes_used + n` gated by the cap in ONE statement), so concurrent
  over-cap writes cannot both read the same baseline and pass
  (`crates/corelink-container/src/byte_accounting.rs:347-372`).
- `AccrueOutcome::OverCap` means the caller must reject the write (bytes NOT counted) and `Indeterminate`
  means a missing row with no resolved cap — never seed an uncapped row from absence
  (`crates/corelink-container/src/byte_accounting.rs:112-130`).
- The over-quota rejection carries the `over_quota` arm of the `X-Rate-Limit-Type` taxonomy
  (storage/bandwidth 100% boundary) (`crates/corelink-rate-headers/src/headers.rs:30-37`).
- `counts_against_sli` classifies `over_quota` as legitimate over-plan (NOT counted against the SLO),
  distinct from a within-quota bug (`crates/corelink-rate-headers/src/headers.rs:101-105`).

# Invariants

- The cap header is server-trusted only — set by the Worker and stripped from client input
  (`crates/corelink-container/src/byte_accounting.rs:85-88`).
- A fresh tenant with an indeterminate cap is NEVER seeded uncapped; absence fails CLOSED rather than
  defaulting to unlimited — the executed arm is the `None`-seed UPDATE-only path that returns
  `Indeterminate` when no row exists (`crates/corelink-container/src/byte_accounting.rs:423-425`).
- The cap check is serialized with the increment in one SQL statement — the live UPSERT's cap predicate
  (`WHERE ?5 = 0 OR …bytes_used + ?3 <= ?5`) gates the DB-side add, so two concurrent over-cap writes
  cannot both pass (`crates/corelink-container/src/byte_accounting.rs:355-363`).
- `over_quota` 429s are excluded from the SLO denominator as legitimate over-plan traffic
  (`crates/corelink-rate-headers/src/headers.rs:101-105`).

# Gotchas

- When the row's `bytes_quota` is `0` (not yet synced from `tenant_quota` by the DO) the write is
  uncapped, but the counter STILL moves — the live UPSERT's `?5 = 0` unlimited arm + `COALESCE(NULLIF(?5,
  0), …)` lets the add through while never clobbering the stored cap — so the cap becomes live the instant
  the DO populates the quota, with no lost history
  (`crates/corelink-container/src/byte_accounting.rs:358-362`).
- Deletes call the inverse `release`, a SATURATING decrement clamped at `0` by a DB `CHECK` and a `MAX(0,
  …)` in SQL, so reclaimed bytes free headroom without ever underflowing
  (`crates/corelink-container/src/byte_accounting.rs:447-451`).
- The accountant is `None` (accounting simply not enforced) when the storage env is unset, mirroring the
  other D1 adapters' dev/CI gate (`byte_accountant_from_env` returns `None` on absent `StorageEnv`)
  (`crates/corelink-container/src/byte_accounting.rs:272-279`).

# Citations

1. `crates/corelink-container/src/byte_accounting.rs:217-230` — `ByteAccountant::accrue` (the executed accrue entry) → `crates/corelink-container/src/byte_accounting.rs:347-372` — the live atomic check-and-accrue UPSERT (DB-side add, serialized cap).
2. `crates/corelink-container/src/byte_accounting.rs:355-363` — the cap predicate serialized with the increment in the live UPSERT (`WHERE ?5 = 0 OR …<= ?5`).
3. `crates/corelink-container/src/byte_accounting.rs:358-362` — uncapped `bytes_quota = 0` arm + `COALESCE(NULLIF(?5,0),…)` still moves the counter without clobbering the stored cap.
4. `crates/corelink-container/src/byte_accounting.rs:447-451` — `D1ByteStore::release` saturating `MAX(0, …)` decrement on delete.
5. `crates/corelink-container/src/byte_accounting.rs:272-279` — `byte_accountant_from_env` returns `None` on absent `StorageEnv` (accounting not enforced in dev/CI).
6. `crates/corelink-container/src/byte_accounting.rs:85-88` — `STORAGE_QUOTA_HEADER` is server-trusted, stripped from client input.
7. `crates/corelink-container/src/byte_accounting.rs:85-94` — the header name + value semantics.
8. `crates/corelink-container/src/byte_accounting.rs:96-110` — fail-CLOSED header parse (`None` indeterminate, `"0"` unlimited).
9. `crates/corelink-container/src/byte_accounting.rs:112-130` — `AccrueOutcome` (`OverCap` / `Indeterminate` fail-closed).
10. `crates/corelink-container/src/byte_accounting.rs:423-425` — the executed `Indeterminate` arm: a fresh row is never seeded uncapped from absence (`None`-cap + missing row → fail-CLOSED).
11. `crates/corelink-rate-headers/src/headers.rs:30-37` — the `over_quota` arm of the `X-Rate-Limit-Type` taxonomy.
12. `crates/corelink-rate-headers/src/headers.rs:101-105` — `counts_against_sli` excludes `over_quota` from the SLO.
13. `crates/corelink-adapter-host/src/oci/auth.rs:200-220` — `VerifiedToken.storage_cap_bytes`: the per-tier cap carried INSIDE the verified OCI bearer (the 2nd cap-delivery seam; same `Some(n)`/`Some(0)`/`None` vocabulary as the header).
14. `crates/corelink-adapter-host/src/oci/auth.rs:250-263` — `decode_cap` fail-closed parse of the signed cap field (`"-"` → indeterminate, negative/non-numeric → rejected).
