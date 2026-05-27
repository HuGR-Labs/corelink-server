# STRIDE deep dive — `corelink-clerk` (Clerk JWT session validation)

- **Crate:** `crates/corelink-clerk` (+ `corelink-clerk-cf` Cloudflare adapter)
- **Date:** 2026-05-15
- **Owner:** Security Lead + Identity
- **Pentest scope:** Yes — engagement 2026-06-15 (P0 surface — third-party identity boundary)
- **Coarse references:** matrix-stride-ctrl.csv THR-S-001/002 (AST-TOKEN×TB-1), THR-T-006 (supply chain on Clerk SDK), THR-E-006 (confused deputy)
- **Pentest doc cross-ref:** §3.1 Identity & Auth
- **SOC 2 cross-ref:** CC3.2 (vendor risk — Clerk sub-processor), CC6.1 (logical access), CC9.2 (vendor management)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-clerk-1** | Cloudflare edge receiving dashboard request with `Authorization: Bearer <Clerk JWT>` | `corelink-clerk::validate(jwt)` | TLS 1.3; JWT in header | `(user_id, tenant_id, scopes)` after RS256 verify + `iss` exact-match + `aud` + expiry + clock-skew |
| **TB-clerk-2** | This crate → Clerk JWKS endpoint (third-party) | Clerk JWKS HTTPS fetch | TLS 1.3 + cert chain; outbound allowlist | JWKS keys cached with kid; rotation tolerated |
| **TB-clerk-3** | Validated session → downstream handlers (admin-api, dual-approval, …) | Handler receives bound session context | Session-bound to UA+IP+PKCE (CTRL-AUTH-010) | Per-request authz state |
| **TB-clerk-4** | Webhooks from Clerk (user lifecycle events) | `corelink-clerk` webhook receiver | Svix-style HMAC sig + idempotency key | User state mutation in D1 |

## 2. STRIDE per boundary

### 2.1 TB-clerk-1 (JWT validate)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Attacker presents JWT with attacker-controlled `iss` (issuer confusion) | INV-AUTH-ISS-EXACT-MATCH — `iss` hardcoded compare to known Clerk URL per environment; no wildcards | `crates/corelink-clerk/tests/adversarial.rs` (10k iter) — FM-AUTH-001 |
| **T** | `alg=none` / HS-with-RSA-public-key attack (CVE-2015-9235, CVE-2018-0114) | INV-AUTH-JWT-VALIDATE-RS256-ONLY — RS256 hardcoded, no algorithm negotiation, deny `alg=none` at decoder | `crates/corelink-clerk/tests/adversarial.rs` — FM-AUTH-002 |
| **R** | Claim "I never authenticated" | INV-AUTH-AUDIT-PRE-POST-ORDERING — emit audit pre+post validate with user_id+kid | INV-AUDIT-APPEND-ONLY + adversarial test |
| **I** | Validation error reveals tenant/user existence | Sanitized error envelope (CTRL-NET-004); identical timing for unknown-user / bad-sig / expired | `crates/corelink-clerk/tests/prop_validate.rs` |
| **D** | JWT-flood requires RSA verify each (CPU heavy) | LRU cache of `(kid, signature) → verified` (bounded); per-IP rate limit; Clerk-issued tokens TTL ≥ 60s amortize | `crates/corelink-clerk/tests/prop_validate.rs` cache property |
| **E** | Token from one tenant accepted for another (confused deputy) | Tenant_id derived from `tid` claim AND cross-checked vs URL path; CTRL-AUTHZ-002 explicit assertion downstream | `specs/tla/tenant_isolation.tla` + property test |

### 2.2 TB-clerk-2 (JWKS fetch)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | DNS hijack of Clerk JWKS host | TLS 1.3 + chain validation; CAA pin on dashboard domain not on Clerk's; mitigated by Clerk's HSTS | `crates/corelink-clerk/tests/http_fetcher.rs` |
| **T** | JWKS response tampered (downgrade) | RS256 enforced regardless of JWKS `alg` hint; key thumbprint cross-check (kid pinned set + grace for rotation) | `crates/corelink-clerk/tests/rotation.rs` |
| **R** | Clerk denies serving a key (key-pin dispute) | Cache key with fetch timestamp; audit emit on rotation | audit emission test |
| **I** | JWKS leak — n/a (public by design) | n/a | n/a |
| **D** | Clerk JWKS endpoint outage blocks all auth | Cached JWKS w/ 1h TTL + 24h stale-while-revalidate; SEV-2 alert if stale > 24h; fail-CLOSED on no-key | `crates/corelink-clerk/tests/rotation.rs` |
| **E** | Compromised JWKS introduces attacker key | Vendor risk on Clerk; mitigation: pinned kid set updated via CTRL-SUPPLY-002 release; rotation requires CoreLink deploy | vendor risk doc `specs/_audits/sealed/2026-05-14-roadmap-r1-8-dependency-audit.md` |

### 2.3 TB-clerk-3 (session → handler)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Session token replay on different UA/IP | CTRL-AUTH-010 — session bound to UA+IP+PKCE; mismatch = 401 + force re-auth | adversarial test |
| **T** | Session context mutated between middleware and handler | Immutable session struct passed by value; signed at middleware exit | type-system enforcement (Rust borrow) |
| **R** | Claim "the handler ran without my consent" | Audit entry with session_id + handler name | INV-AUDIT-APPEND-ONLY |
| **I** | Handler sees scopes beyond session | Scope subset enforced at middleware; CTRL-AUTHZ-001 per-verb | property test |
| **D** | Session inflation (huge claims) | Max JWT size + claim count cap | `crates/corelink-clerk/tests/prop_validate.rs` |
| **E** | Privilege escalation via stale session reuse | INV-ADMIN-MFA-FRESHNESS 30 min for admin ops; non-admin TTL ≤ 24h | `crates/corelink-clerk/tests/adversarial.rs` |

### 2.4 TB-clerk-4 (webhooks)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Forged webhook | HMAC sig (Svix) constant-time verify + timestamp window ±5 min | webhook signature test |
| **T** | Replay of valid webhook | Idempotency key D1 dedup PRIMARY KEY | property test |
| **R** | "Clerk never sent this user-deleted event" | Reconcile vs Clerk API daily; audit chain entry | INV-AUDIT-APPEND-ONLY |
| **I** | Webhook contains PII | CTRL-PRIV-001 redact at log; user_id pseudonymized | LINDDUN review |
| **D** | Webhook flood | CF rate limit + Clerk-side throttle | k6 |
| **E** | Webhook used to mutate other tenants | tenant_id derived from validated user_id, never trusted from payload | property test |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-CLERK-01 | Clerk is a critical sub-processor — outage = no dashboard auth | MEDIUM | Documented in DPA; status page + comms plan (FM-101 analogue); future: optional self-hosted IdP path (R-7+) |
| RR-CLERK-02 | JWKS rotation grace can briefly accept legacy-kid tokens | LOW | Bounded by Clerk TTL ≤ 1h; rotation playbook |
| RR-CLERK-03 | Webhook signature relies on Svix lib — supply-chain | LOW | INV-SUPPLY-PROVENANCE-IN-REKOR + cargo-deny |

## 4. Adversarial test pointers

- `crates/corelink-clerk/tests/adversarial.rs` — alg=none, iss spoof, session-binding (10k iter)
- `crates/corelink-clerk/tests/prop_validate.rs` — validate proptest
- `crates/corelink-clerk/tests/rotation.rs` — JWKS rotation + stale handling
- `crates/corelink-clerk/tests/http_fetcher.rs` — fetcher hardening
- `crates/corelink-clerk/tests/mutation_kills.rs` — mutation baseline

## 5. Cross-references

- Invariants: INV-AUTH-ISS-EXACT-MATCH, INV-AUTH-JWT-VALIDATE-RS256-ONLY, INV-AUTH-AUDIT-PRE-POST-ORDERING, INV-ADMIN-MFA-FRESHNESS, INV-TENANT-ISOLATION, INV-CONF-IN-FLIGHT
- Controls: CTRL-AUTH-001, CTRL-AUTH-010, CTRL-AUTHZ-001/002, CTRL-NET-004, CTRL-CRYPTO-001, CTRL-PRIV-001
- Failure modes: FM-AUTH-001..005 (pentest §3.1), FM-101 (edge outage analogue for IdP)
- SOC 2: CC3.2, CC6.1, CC9.2
