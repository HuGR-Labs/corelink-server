---
title: "OKF Wiki doc-extraction wave (wave-4) — Remediation Worklist"
type: "Audit"
description: "Exact transcribe-only fixes for the 6 MAJOR + 11 MINOR from the 32-concept doc-extraction adversarial verification. All lead-spot-confirmed against code."
status: "CLOSED (2026-06-26) — 6 MAJOR + 11 MINOR applied; lead cold-checked the 3 TRUTH rewrites vs code; 32 doc-extraction concepts TRUTH-CERTIFIED; validator green at 146."
tags: ["okf", "audit", "remediation", "doc-extraction"]
---

# doc-extraction remediation worklist (apply EXACTLY; transcribe-only)

Targets are `docs/knowledge/{security,compliance,testing,launch,ops}/*.md`. Before writing any new
cite, OPEN the target line and confirm it shows the claim. Do NOT change code. Keep `checkpoint_sha`
unchanged. After all fixes: `python3 scripts/okf_index.py` then `python3 scripts/validate_okf.py` → GREEN.

## MAJOR (lead-spot-confirmed against code)

### compliance/dsr-erasure.md — 3 MAJOR + 2 MINOR
- [TRUTH] Remove the fabricated symbol `InProcessErasureSink` (0 hits in repo). Replace the self-serve account-delete claim with EXACTLY:
  > A self-serve account delete routes through `routes/customer.rs::handle_account_delete`
  > (`crates/corelink-container/src/routes/customer.rs:940`), which builds a `dsr.queued.v1` message and
  > `INSERT OR IGNORE`s a `dsr_requested` anchor row, then hands erasure to the `DsrErasureSink` trait —
  > production **enqueues to a queue (async)**, not a synchronous same-worker drive (the only in-tree
  > sink impl is a test `MockSink`).
- [SOURCE_FILES] Add `crates/corelink-container/src/routes/customer.rs` to `source_files` (cited above).
- [GROUNDING] Repoint the `dsr.rs` cites to the ENFORCING lines (currently point at the verify handler / test code / structs): live erasure intake/orchestrator `handle_erase` → `dsr.rs:369-410`; forged-tenant 422 reject → `dsr.rs:397-398`; attestation signed only on VerifiedComplete → `dsr.rs:443-446`; `build_state_from_env` 32-char floor → `dsr.rs:256-272`.
- [MINOR] MFA "rejects None/empty/expired" cites `mfa.rs:146-154` but that impl only rejects None/empty (`Some(_)=>Ok`); expiry is only in the trait contract `mfa.rs:80-102` — scope the claim to None/empty for :146-154, or cite :80-102 for expiry.
- [MINOR] tombstone-read-410 invariant cites `secreview:53-64` (which says the SHARED seam read → 404); cite `cas_erase.rs:1-8` for the native-route 410 instead.

### ops/admin-plane.md — 1 MAJOR + 2 MINOR
- [TRUTH] The dual-approval claim is BACKWARDS. Replace with EXACTLY:
  > A dual-approval pair must be set together: an **incomplete** pair (one of `approval_id`/`approver`
  > present, the other absent) is rejected at request construction (`crates/corelink-container/src/routes/admin.rs:592`);
  > a **fully-absent** pair yields `None` (no approval requested) and proceeds — the per-operation
  > approval *requirement* is enforced in the handler, not at construction.
- [MINOR] "pilot baseline is InMemoryPilotStore" cite `admin_pilot.rs:390` (trait def) → `admin_pilot.rs:1031-1032` (`build_handlers` returns `InMemoryPilotStore::new()`).
- [MINOR] "pilot mutations emit corelink.admin.pilot_*" cite `admin_pilot.rs:174` (const decl) → the handler emit call site.

### launch/tier-model.md — 1 MAJOR
- [TRUTH] Drop "durable". Replace the audit-seam claim with EXACTLY:
  > The fail-CLOSED audit seam emits a `tracing` audit log before the state mutation via
  > `TierSelectAudit::emit` (`crates/corelink-container/src/routes/tier_select_audit.rs:75`) — today a
  > `tracing::info!` that always returns `Ok` (the durable D1 audit-chain write is the tracked Wave-37
  > hardening). The emit-Err-aborts-before-mutation logic lives in `tier_select.rs:735-738` (vacuous
  > today since the adapter never returns `Err`).

### ops/audit-analytics-plane.md — 1 MAJOR (SOURCE_FILES)
- Add the ENFORCING submodules to `source_files` and re-cite the implementing lines: the fail-closed-503 export gate is `crates/corelink-container/src/audit_export/audit_sink.rs:100-105` (not the barrel re-export `audit_export.rs:145`). Add `audit_export/{audit_sink,stream,state,types}.rs` and `audit_analytics/{state,rate_limit}.rs` to source_files; repoint the export-gate/router/header cites into those files. (`audit_analytics.rs:118` pat_gate_reject is a real in-barrel impl — keep.)

## MINOR (other dirs)
- security/pentest-learnings.md: the "container tracing unreachable from CLI/API" clause is cited to `docs/security/2026-06-18-nuclear-cycle2-triage.md:1-10`, which only supports the rate-limit/PARTIAL-coverage half — drop the container-tracing clause (or re-cite to a source that states it).
- testing/real-user-suites.md: intro reproduces the stale 2026-06-22 "3 journeys remain GATED" headline, contradicting the concept's own body — reword to "journey 1 fully gated; journey 2 forgery-proven (flip-gated); journey 3 (quota hard-cap) validated un-gated".
- ops/gc-eviction.md: (a) "unprobeable degrade → GcPause fail-closed" cite `degrade.rs:59` (the abort half) → add `degrade.rs:120`/`:17` for the fail-closed-probe contract; (b) "enterprise TTL override hard-capped 730d, longer rejected" cite `tier.rs:41` (bare const) → `tier.rs:145-148` (`ttl_for_tier_with_override` enforces `ExceedsMaxTtl`).
- ops/runners-fabric.md: "FABRIC_INTROSPECT_AUTH_KEY dedicated + _HUGR additive" cite `auth_introspect.rs:567-569` (the OR-combine loop) → `:661` + `:691-700` (the dedicated-key read + _HUGR additive consumer in `build_state_from_env`).
- ops/secrets-lifecycle.md: the "echo trailing-newline → 401/403" mechanism is cited to `docs/operator/launch-pat-scope-fix-runbook.md:51-56` which only shows the `printf '%s'` example — soften to the cited discipline (use printf, not echo) without asserting the newline-pollution mechanism from that cite.
- launch/signup-onboarding.md: "rate limit is 5 requests / IP / hour" mis-reads `RateLimitConfig::with_overrides(1,5,720,…)` — restate as "burst 5, refill 1/s, 720s Retry-After floor" (`signup.rs:631-635`).
