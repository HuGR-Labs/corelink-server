# STRIDE deep dive — `corelink-tenant-path` (tenant prefix derivation — worker integration surface)

> **Scope distinction:** this deep dive covers the **integration surface** in `corelink-worker` and adjacent crates that *consume* `tenant_path::derive_prefix`. The primitive itself (HMAC algorithm, key handling, property tests) is covered separately in `STRIDE-tenant-path.md` (crate `crates/tenant-path`).

- **Crate(s):** `corelink-worker` (consumer) + every storage-call site that derives the per-tenant R2 prefix
- **Date:** 2026-05-15
- **Owner:** Security Lead + Architect
- **Pentest scope:** Yes — engagement 2026-06-15 (P0 surface — this is the *headline* multi-tenant isolation enforcement point)
- **Coarse references:** matrix-stride-ctrl.csv THR-I-001 (cross-tenant read), THR-I-002 (timing side-channel), THR-E-003 (path traversal), THR-E-006 (confused deputy), FM-253 (cross-tenant read), FM-303 (AC cross-tenant)
- **Pentest doc cross-ref:** §3.5 Multi-tenant isolation
- **SOC 2 cross-ref:** CC3.2, CC6.1, CC6.6 (data segregation)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-tp-1** | Worker handler (after auth resolves tenant_id) | `tenant_path::derive_prefix(tenant_id)` (called via `corelink-worker`) | tenant_id from validated PAT/JWT (never client-supplied); CTRL-AUTHZ-002 | 16-byte b64 prefix used as R2 key root |
| **TB-tp-2** | Worker → R2 storage driver | R2 PUT/GET/LIST with prefixed key | IAM bucket policy restricts to prefix pattern (CTRL-ISO-003) | Storage operation succeeds only on own prefix |
| **TB-tp-3** | Worker → D1/Neon metadata layer | D1 query with `WHERE tenant_id = $1` parameterized | RLS on Neon (`auth_schema` RLS default-on) | Row set scoped to tenant |
| **TB-tp-4** | Cross-tenant dedup (opt-in only, BYOE) | Dedup index lookup | ADR-signed cross-tenant policy; default OFF (CTRL-ISO-005) | Match or miss without revealing existence |

## 2. STRIDE per boundary

### 2.1 TB-tp-1 (derive)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Caller passes attacker-controlled tenant_id (URL or header injection) | tenant_id sourced from validated PAT/JWT `tid` claim only; URL/header path includes tenant only as cross-check (must match) | `specs/tla/tenant_isolation.tla` + `crates/tenant-path/tests/prop_tenant_path.rs` — FM-TENANT-001 |
| **T** | Tampered prefix substituted between derive and storage call | Prefix returned by value (owned bytes); type-system enforces flow; assertion at storage driver re-derives + compares | property test cross-tenant 30k cases (INV-TENANT-ISOLATION) |
| **R** | "We read another tenant's blob" claim | Audit emit includes derived prefix hash + tenant_id (pre+post) | INV-AUTH-AUDIT-PRE-POST-ORDERING |
| **I** | Existence oracle via 404 timing | CTRL-ISO-004 (Constant-time 404 MissReason parity per ADR-0023+ADR-0028); `TimingPaddingLayer`; pairwise Mann-Whitney + Šidák; \|Δmedian\| ≤ 1ms gate; alert SEV-2 > 5ms | `specs/_audits/sealed/2026-05-14-property-test-summary-s19.md` — FM-TENANT-004 |
| **D** | High prefix-derivation cost (HMAC) on hot path | HMAC-SHA256 ≤ 10 µs; result cached per request (`Arc<[u8;16]>`) | benches |
| **E** | Cross-tenant escalation by passing both tenants' IDs to a single call | INV-TENANT-ISOLATION 5-layer defense (`auth_model.md §8.1`); confused-deputy mitigated by single-tenant-per-call invariant | `specs/tla/tenant_isolation.tla` |

### 2.2 TB-tp-2 (R2 storage call)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Worker reaches different bucket than intended | Bucket ARN pinned in binding config; deploy-signed (CTRL-SUPPLY-002) | INV-SUPPLY-SIGNED-DEPLOY |
| **T** | Path traversal injects `..` / NUL / control chars to escape prefix | CTRL-INPUT-001 (path canonicalize + allowlist); rejects `..`, `\0`, invalid UTF-8, empty, > 4 KiB | `crates/corelink-worker/tests/` path-traversal cases — THR-E-003 |
| **R** | Repudiation on cross-tenant write | All R2 ops audit-emitted with tenant_id + derived prefix | INV-AUDIT-APPEND-ONLY |
| **I** | LIST operation enumerates beyond prefix | LIST scoped to `prefix=<derived_prefix>/`; pagination cursor cannot escape | property test + bucket policy review (CTRL-ISO-003) |
| **D** | Mass enumeration LIST storms (cost) | Per-tenant rate limit on LIST verb; quota on result-size | rate-limit tests |
| **E** | Pre-signed URL constructed for wrong path | Pre-signed URL contract: path is server-computed; client cannot inject path; URL TTL ≤ 5 min | adversarial regression |

### 2.3 TB-tp-3 (metadata layer)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | SQL parameter passed as raw string with attacker tenant_id | INV-INPUT-002 (`sqlx::query!` macros only; deny raw strings); clippy custom lint | clippy + CI gate |
| **T** | SQL injection via metadata field | CTRL-INPUT-002 parameterized queries + schema validation | adversarial regression |
| **R** | "Metadata says I wrote it but I didn't" | Audit emit pre+post metadata write | INV-AUDIT-APPEND-ONLY |
| **I** | Cross-tenant metadata leak via JOIN | Neon RLS default-on (INV-AUTH-SCHEMA-RLS-DEFAULT-ON); D1 query SCOPED with tenant_id literal | property test |
| **D** | Metadata query storm | Per-tenant query rate; connection pool bulkhead (PAT-BULKHEAD-001) | FM-058 |
| **E** | Privilege escalation via metadata column manipulation | INV-INPUT-002 + schema CHECK constraints; refcount manipulation prevented (INV-GC-003) | property test |

### 2.4 TB-tp-4 (cross-tenant dedup, opt-in)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Tenant injects forged dedup hint | Dedup lookup uses derived digest only; tenant_id verified post-match | property test |
| **T** | Dedup table tampered to point to attacker blob | CAS integrity check on read (CTRL-CAS-002 BLAKE3 verify); INV-CAS-INTEGRITY | `specs/tla/cas_integrity.tla` |
| **R** | "Dedup credited my tenant for unowned content" | Audit emit with both source + target tenant_id on cross-tenant match | INV-AUDIT-APPEND-ONLY |
| **I** | Existence oracle: "this blob is in another tenant" leaks | CTRL-ISO-005 — cross-tenant dedup OFF by default; opt-in requires customer ADR signing acknowledging existence-oracle risk | `specs/_audits/sealed/2026-05-14-pentest-s14-byok.md` |
| **D** | Dedup probe storm | Per-tenant dedup lookup rate | rate-limit |
| **E** | Cross-tenant dedup used to read another tenant's blob | Dedup returns digest match only; blob read still enforces tenant prefix; impossible to read across | INV-DEDUP-CONSISTENCY |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-TP-01 | Cross-tenant dedup (opt-in) exposes existence oracle | LOW (default OFF; ADR-gated) | CTRL-ISO-005 + customer-signed ADR |
| RR-TP-02 | 5-layer defense relies on consistent tenant_id threading through all call paths | LOW | Compile-time newtype + clippy lint; reviewed in `auth_model.md §8.1` |
| RR-TP-03 | Timing parity gate \|Δmedian\| ≤ 1ms — measured under load; CI variance can mask | LOW | Mann-Whitney + Šidák correction; alert SEV-2 if > 5ms 5min |

## 4. Adversarial test pointers

- `crates/tenant-path/tests/prop_tenant_path.rs` — derivation primitive property tests
- `crates/corelink-worker/tests/` — path-traversal + cross-tenant integration cases
- `specs/tla/tenant_isolation.tla` — formal 5-layer model
- `specs/_audits/sealed/2026-05-14-property-test-summary-s19.md` — timing-parity evidence (CTRL-ISO-004)
- `specs/_audits/sealed/2026-05-01-adversarial-s03.md` and later S-04/S-05 (tenant isolation focus sprints)

## 5. Cross-references

- Invariants: INV-TENANT-ISOLATION, INV-AC-TENANT-SCOPED, INV-DEDUP-CONSISTENCY, INV-INPUT-002 (via CTRL-INPUT-002), INV-CAS-INTEGRITY
- Controls: CTRL-AUTH-004 (path HMAC), CTRL-AUTHZ-002 (explicit tenant_id), CTRL-ISO-001..005, CTRL-INPUT-001/002, CTRL-CAS-001/002, CTRL-SUPPLY-002
- Failure modes: FM-TENANT-001..006 (pentest §3.5), FM-253 (cross-tenant read), FM-303 (AC cross-tenant)
- SOC 2: CC3.2, CC6.1, CC6.6
