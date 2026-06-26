---
type: "ADR"
title: "ADR-S13-002 — Dual-approval enforcement + collusion-rotation defense"
description: "Why destructive admin ops require a second approver with a hard-fail 403 and a collusion-rotation oracle that blocks reciprocal A<->B approval."
source_files:
  - "specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s13", "admin-plane", "dual-approval", "nist-ac-2-7"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S13-002 — Dual-approval enforcement + collusion-rotation defense

The CoreLink admin plane executes destructive operations with global blast radius — tenant tombstone, retention reduction, secret-rotation start, config rollback. A single admin is a trivial insider threat, and naive two-admin approval is defeated by reciprocal collusion (A approves B, B approves A, repeat). This ADR records `PAT-DUAL-APPROVAL-001`: a hard-fail second-approver gate plus a collusion-rotation oracle that requires three distinct approvers in any rolling window of three destructive ops, satisfying NIST SP 800-53 AC-2(7).

# Context

Destructive admin ops have global blast radius; single-admin bypass is a trivial insider threat, and two-admin reciprocal collusion defeats naive dual-approval (`specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:24-29`). NIST SP 800-53 AC-2(7) requires privileged accounts be managed with special scrutiny, including rotation and unusual-activity monitoring (`specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:31-33`).

# Decision

- **Hard-fail 403**: every destructive op requires `X-Dual-Approver` + `X-Approver-Signature` headers; missing/invalid = 403 with no advisory mode, override, or env bypass (`specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:40-42`).
- **Separation of duties**: `caller != approver` is a D1 hard-check with no dev/staging exception (`specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:44-46`).
- **Collusion-rotation oracle**: the proposed approver must not be among the last 2 distinct approvers in the tenant's 24h window — equivalently, any rolling window of 3 destructive ops must have 3 distinct approvers; this is stronger than the prior `LIMIT 3 + count(distinct) < 3` oracle that let A→B/B→A/A→B succeed at op 3 (`specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:47-54`).
- **Defense-in-depth**: MFA freshness (`auth_time` ≤ 30 min else 401), constant-time HMAC-SHA256 over `op_payload || nonce || ts_ms_be` with a per-region key, a D1 UNIQUE nonce against replay, and ≤60s clock-skew tolerance (`specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:56-67`).
- **Audit fail-CLOSED**: the op + audit row are written as one D1 atomic batch; failure → 503 and the op does not execute (`specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:68-70`).

# Consequences

- Admin op latency ≤50ms p99 including the indexed collusion query; the `denied_collusion_rotation` error surfaces a "need a 3rd admin" message to the operator (`specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:105-107`).
- Rollout requires synchronous 2-engineer presence at GA (no async approval), and every op — approved or denied — is emitted as a chain-integrity CloudEvent (`specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:108-110`).

# Citations

1. `specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:24-33` — the insider-threat + reciprocal-collusion context and the NIST AC-2(7) driver.
2. `specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:40-54` — hard-fail 403, separation of duties, and the collusion-rotation oracle (3-distinct-in-3).
3. `specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:56-70` — MFA freshness, constant-time HMAC, nonce replay, and audit fail-closed.
4. `specs/03_architecture/adrs/ADR-S13-002-dual-approval-collusion-rotation.md:105-110` — latency, UX, and audit-trail consequences.
