---
type: "SecurityControl"
title: "Security posture overview"
description: "CoreLink's overall security posture: the four invariant guarantees and trust-boundary model from the architecture, plus the CAA-360 adversarial-assurance verdict (launch-safe, conditionally)."
source_files:
  - "docs/security/2026-06-13-CAA-360-audit-report.md"
  - "ARCHITECTURE.md"
  - "crates/corelink-container/src/storage/r2_s3.rs"
  - "worker/src/index.ts"
  - "crates/corelink-container/src/routes/dsr/audit.rs"
  - "apps/signup-worker/src/security-headers.ts"
checkpoint_sha: "7f62573f2be4f07352de830fe98f400bb1345adb"
provenance: "AUTHORED"
tags: ["security", "posture", "audit", "tenant-isolation", "compliance"]
timestamp: "2026-06-26T00:00:00Z"
---

# Security posture overview

CoreLink is a multi-tenant content-addressable cache where a single isolation or integrity failure
can flash a malicious binary into another tenant's production — so the whole architecture is built
around four MUST-level invariants and a layered trust-boundary model. This concept is the top-level
map of that posture: the invariant guarantees the system derives everything from, the plane and
trust-boundary structure, and the verdict of the CAA-360 adversarial-assurance audit (no Critical,
one documentation-grade High, launch-safe conditionally). It is the entry point above the more
focused [data-plane attack surface](/security/attack-surface-dataplane.md),
[credential handling](/security/credential-handling.md),
[money-path review](/security/money-path-review.md), and
[pentest learnings](/security/pentest-learnings.md), and ties to
[tenant isolation](/tenancy/isolation.md) and the [PAT moat](/auth/pat-moat.md).

# Role

It states the security thesis (the four invariants + the trust boundaries) and grounds it in an
evidence-anchored adversarial audit, so a reviewer can see both the intended model and the audited
reality (including the cluster of fail-open-instead-of-fail-closed gaps that the audit flagged as the
launch gate).

# How it works

- CoreLink commits to four RFC-2119 MUST invariants that everything derives from: CAS integrity,
  tenant isolation (TLA+-verified), confidentiality at rest and in flight, and an append-only
  Merkle-chained audit log (`ARCHITECTURE.md:27-52`). **Carve-out: the append-only Merkle-chained
  audit log is DESIGNED-not-yet-live** — the chain crates have no live producer, and the deployed
  audit trail is unsealed/unchained D1 `audit_outbox` rows (`emitted_at=NULL`, no `prev_hash`/
  `sequence_number`); chain-sealing is the unbuilt WI-S09-007 (`crates/corelink-container/src/routes/dsr/audit.rs:7-8`,
  `crates/corelink-container/src/routes/dsr/audit.rs:82`). See the
  [audit-analytics crate cluster](/crates/audit-analytics.md).
- The control plane (stateless Worker) and the data plane (in-region Rust container) communicate only
  over Cloudflare service bindings — no public endpoint exists for the container, so the attack
  surface is the WAF, not the gRPC server (`ARCHITECTURE.md:93-101`).
- Tenant isolation is enforced in two independent layers: a Worker authZ check that the PAT-resolved
  tenant equals the requested tenant, and an HMAC-derived per-tenant R2 prefix at the storage binding —
  both must agree for a request to land bytes. Both layers are real and live. The Worker layer resolves
  `{tenantId, scope}` from the D1 `pat` row (`worker/src/index.ts:1039-1044`), and the cache path
  rejects a path/PAT tenant mismatch with a 403 — `if (isRealTenant && urlTenant !== "_anonymous" &&
  urlTenant !== resolvedTenantId) { 403 "tenant mismatch" }` (`worker/src/index.ts:2141`). The
  storage layer builds every R2 object key only through `r2_key`, which derives the per-tenant prefix
  via `tenant_prefix(self.tdk, tenant)?` and fails CLOSED on its error
  (`crates/corelink-container/src/storage/r2_s3.rs:519`, `crates/corelink-container/src/storage/r2_s3.rs:466`;
  `ARCHITECTURE.md:170-199`).
- The trust-security model layers the Merkle-chained audit, TLA+-verified invariants, a STRIDE/LINDDUN
  boundary model (TB-0..TB-4), SLSA-L3 supply chain, and constant-time side-channel hygiene
  (`ARCHITECTURE.md:274-345`). **BYOK envelope encryption is a DESIGNED posture layer, not a live one:**
  ARCHITECTURE.md describes it as engineered intent, but the at-rest BYOK encryption path is
  pure-logic skeleton / unwired in the deployed container (see CLAUDE.md "Designed ≠ wired"); do not
  cite it as an active control (`ARCHITECTURE.md:274-345`).
- The CAA-360 audit was a PTES/OWASP/MITRE-method 360° sweep with adversarial verification: of 49 raw
  candidates, 37 confirmed and 12 were refuted as false-positives on re-reading the real code
  (`docs/security/2026-06-13-CAA-360-audit-report.md:1-5`).
- The verdict was launch-safe conditionally: no confirmed Critical and one confirmed High that is a
  documentation-only error (a wrong migration comment), with the steady-state production posture
  holding (`docs/security/2026-06-13-CAA-360-audit-report.md:13-27`).
- The honest top risks cluster on one root cause — secrets and invariants that fail OPEN and silently
  instead of fail-closed and loud: the TDK unset in prod, optional secrets that fail open, and an AC
  divergent-body overwrite (`docs/security/2026-06-13-CAA-360-audit-report.md:22-27`).
- Defense-in-depth at the public webhook edge: every response from the signup worker carries a hardened
  header set — a strict `Content-Security-Policy: default-src 'none'; frame-ancestors 'none'; base-uri
  'none'`, `Strict-Transport-Security: max-age=63072000; includeSubDomains; preload`, plus `nosniff` /
  `X-Frame-Options: DENY` / `Referrer-Policy: no-referrer` / a locked-down `Permissions-Policy`
  (`apps/signup-worker/src/security-headers.ts:14-24`); `withSecurityHeaders` merges them onto every
  response, gap-filling only so per-route values (e.g. `Content-Type`) survive
  (`apps/signup-worker/src/security-headers.ts:31-41`).

# Invariants

- The four MUST guarantees (CAS integrity, tenant isolation, confidentiality, audit append-only) are
  the trust root the rest of the system enforces — verified by TLA+, proptest, or runtime assertions
  (`ARCHITECTURE.md:33-52`). **Two of the four are not yet fully live and MUST NOT be cited as active
  controls: BYOK at-rest confidentiality (designed/unwired, above) and the audit append-only chain
  (chain code is real+tested but has no live producer — the deployed trail is unsealed `audit_outbox`
  rows; chain-sealing = unbuilt WI-S09-007).** The two isolation/integrity layers below ARE live.
- Tenant isolation is defense-in-depth: a logic bug in either the Worker authZ layer or the storage
  prefix layer still produces zero cross-tenant blast; the Worker half is the path-vs-PAT tenant
  mismatch 403 at `worker/src/index.ts:2141` (over the D1-resolved tenant at `:1039-1044`), and the
  storage-prefix half is enforced at `crates/corelink-container/src/storage/r2_s3.rs:519` (the only
  path that builds an R2 key) (`ARCHITECTURE.md:170-183`).
- The audited posture is gated on closing the fail-open-secrets class: set the secrets, make them
  mandatory, add the AC divergence guard, fix the migration triggers, then launch
  (`docs/security/2026-06-13-CAA-360-audit-report.md:22-27`).

# Gotchas

- ARCHITECTURE.md is the canonical-intent overview (it explicitly defers to the Level-3 specs); some
  of its claims describe the engineered target and the audits below record where the deployed reality
  differs (e.g. the secret TDK being unset in prod) — read it together with the CAA-360 findings, not
  alone (`docs/security/2026-06-13-CAA-360-audit-report.md:104-120`).
- The one CAA-360 High (F36) is a documentation defect: migration 0064's comment falsely claims
  triggers survive a `DROP TABLE`+`RENAME`, silently dropping the `primary_region` immutability
  backstop — flagged High for the privacy invariant it endangers, not for a remote-reachable breach
  (`docs/security/2026-06-13-CAA-360-audit-report.md:51-81`).

# Citations

1. `ARCHITECTURE.md:27-52` — the four MUST invariant guarantees.
2. `ARCHITECTURE.md:33-52` — invariant detail (integrity, isolation, confidentiality, audit).
3. `ARCHITECTURE.md:93-101` — control/data plane split; container is service-binding-only.
4. `ARCHITECTURE.md:170-199` — two-layer tenant isolation (Worker authZ + HMAC R2 prefix).
5. `ARCHITECTURE.md:274-345` — trust & security model (Merkle audit, TLA+, STRIDE/LINDDUN, SLSA-L3, constant-time); BYOK envelope encryption is listed here as DESIGNED intent, not a wired/live control.
11. `crates/corelink-container/src/storage/r2_s3.rs:519` / `:466` — the live storage-layer tenant-isolation enforcer: `r2_key` derives the HMAC per-tenant prefix and fails CLOSED; this is the second of the two isolation layers.
12. `worker/src/index.ts:1039-1044` / `:2141` — the live Worker authZ tenant-isolation enforcer (the first layer): `{tenantId, scope}` resolved from the D1 `pat` row, and a `urlTenant !== resolvedTenantId` → 403 "tenant mismatch" on the cache path.
6. `docs/security/2026-06-13-CAA-360-audit-report.md:1-5` — audit counts (49 raw / 37 confirmed / 12 refuted; 1 High).
7. `docs/security/2026-06-13-CAA-360-audit-report.md:13-27` — executive summary: launch-safe conditionally, no Critical.
8. `docs/security/2026-06-13-CAA-360-audit-report.md:22-27` — the fail-open-secrets root cause + the launch gate.
9. `docs/security/2026-06-13-CAA-360-audit-report.md:51-81` — the one High (F36): migration 0064 trigger-loss documentation defect.
10. `docs/security/2026-06-13-CAA-360-audit-report.md:104-120` — F1: cross-tenant co-residence when `R2_TDK_HEX` is unset in prod.
13. `crates/corelink-container/src/routes/dsr/audit.rs:7-8`, `crates/corelink-container/src/routes/dsr/audit.rs:82` — the audit append-only MUST is designed-not-yet-live: the live sink writes *unchained* CloudEvents to `audit_outbox` (`emitted_at=NULL`, no `prev_hash`/`sequence_number`); chain-sealing is the unbuilt WI-S09-007.
14. `apps/signup-worker/src/security-headers.ts:14-24` / `:31-41` — the edge security-header set (strict CSP `default-src 'none'`, HSTS 2y preload, nosniff / `X-Frame-Options: DENY` / no-referrer / `Permissions-Policy`) and the `withSecurityHeaders` gap-fill merge applied to every signup-worker response.
