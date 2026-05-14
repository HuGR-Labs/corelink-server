---
id: "ADR-S14-001"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "s14", "region", "pinning", "enforcement", "schrems-ii", "lgpd", "gdpr", "property-test", "wi-s14-002"]
---

# ADR-S14-001 — Tenant Region Pinning Enforcement: Custom Domain Authoritative + DO region_enforcer + 30k Property Test + INV-REGION-NO-CROSS-LEAK

## Status

ACCEPTED — WI-S14-002 SEALED.

## Context

CoreLink S-14 expands to 4 production regions (WNAM/ENAM/WEUR/SAM). Without runtime enforcement, a tenant pinned to WEUR can be served by the ENAM endpoint — routing EU PII to US infrastructure = Schrems II (CJEU C-311/18) + LGPD Art. 33 §1º catastrophic legal exposure (€20M+ GDPR fines + breach notification).

Prior art: WI-S11-007 established the `ResidencyEnforcement` trait + `InMemoryResidencyEnforcement` + 20k property test (2 regions: weur/enam). S-14 extends to all 4 Phase-1 regions with a 30k coverage matrix.

**The key decision:** where is `request_region` derived from?

Option A: `X-Region` HTTP header (attacker-controlled; trivially spoofed).
Option B: Custom domain `{region}.api.corelink.dev` (TLS-terminated by Cloudflare; cannot be tampered in transit).

## Decision

**Custom domain is the authoritative source for `request_region`.** `X-Region` headers are ignored.

**Stack:**

1. **`region_from_host(host: &str) → Option<Region>`** — parses `{tenant_id}.{region}.corelink.dev` from the Host header (custom domain). Region must be in the 4-region canonical enum; unknown strings return `None` (fail-CLOSED).

2. **`ResidencyEnforcement::assert_request_residency`** — Tower layer enforcer. Called AFTER auth (TenantCtx propagated) + BEFORE any handler or backend access. Mismatch = `ResidencyViolation::RequestRegionMismatch` = HTTP 451 (or 403 per spec; implementation maps to 451 per PAT-ROUTING-PINNED-001 fail-CLOSED canonical).

3. **`ResidencyEnforcement::assert_write_residency`** — Insert check for every backend write (R2 CAS, AC, KV, D1, audit emit). Defense-in-depth: Tower layer fires first; this is the per-write backstop.

4. **D1 trigger `trg_tenant_primary_region_immutable`** — Database-level backstop: `primary_region` is immutable post-INSERT. Prevents admin API mutation (which would re-route existing tenant data cross-region).

5. **DO `region_enforcer`** — Durable Object per region caches `tenant_id → primary_region` (5min TTL) to reduce D1 RTT. D1 fallback on cache stale > 5min (never serve stale indefinitely).

6. **30k property test** — 4 regions × 4 ops × 4 tenant types = 64 coverage cells × ~469 iter avg. PR gate: 30k. Nightly: 100k. 0 leaks required.

7. **KV per-region namespace** — `corelink-session-{region}` naming enforced. TypeScript binding types prevent cross-region KV access at compile time (FM-054 prevention).

## Consequences

**Positive:**
- Spoofing via `X-Region` header is impossible: custom domain routing is TLS-terminated by Cloudflare.
- Defense-in-depth: 4 layers (Tower middleware → insert check → D1 trigger → DO cache invalidation).
- 30k property test provides ~99.999% confidence; nightly 100k sustained.
- INV-REGION-NO-CROSS-LEAK CRITICAL ratificada with runtime + test evidence.
- Schrems II TIA + GDPR Art. 46 + LGPD Art. 33 §1º attestation supported.

**Negative:**
- DO `region_enforcer` adds ~$60/mês (4 instances × $15 each).
- 5min TTL creates a 5min window for stale cache (acceptable per design §9.3; D1 fallback mitigates).
- D1 trigger `trg_tenant_primary_region_immutable` requires manual ticket for legitimate region migration (acceptable; region migration is rare + high-risk).

**Neutral:**
- `region_from_host` also accepts bare region strings for test/internal routing (fallback branch).
- `apac/afr` regions are valid in the 6-region enum but not yet provisioned (Phase 2/3 deferred).

## Alternatives Considered

**A. `X-Region` header:** Rejected. Trivially spoofed; attacker sets `X-Region: weur` for ENAM endpoint. No TLS guarantee.

**B. Geo-IP only:** Rejected. Geo-IP is probabilistic; wrong for VPN users + enterprise proxies. Also doesn't handle explicit tenant-ID cross-region injection.

**C. Skip middleware, enforce only at D1 trigger:** Rejected. D1 trigger fires per-INSERT but doesn't reject reads; attacker can read from wrong region bucket. Tower middleware enforces ALL request types (GET included).

**D. 10k property test:** Rejected per §9.4. 10k = ~99.99%; 30k = ~99.999%. INV-REGION-NO-CROSS-LEAK is CRITICAL severity; higher confidence required.

## References

- WI-S14-002 §9.1 (custom domain decision) + §9.3 (DO cache 5min TTL) + §9.4 (30k iter rationale).
- ADR-S11-010 (TLA+ region_residency deferred to S-14 WI-S14-009).
- ADR-S11-011 (region migration cooldown 30d).
- `privacy_model.md §7.1` (6-region canonical enum).
- `resilience_patterns.md §3.4` (PAT-ROUTING-PINNED-001 fail-CLOSED).
- CJEU C-311/18 (Schrems II) + LGPD Art. 33 §1º + GDPR Art. 44-46.

## Reuse Pattern

This pattern (custom domain authoritative + Tower enforcer + D1 backstop + property test) is the canonical template for APAC/AFR expansion and any future region additions.

Adding a new region requires: ADR + Privacy Officer + Compliance review (cardinality discipline per ADR-S11-012).

---

## Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | Criação ADR-S14-001 (WI-S14-002 SEALED). |
