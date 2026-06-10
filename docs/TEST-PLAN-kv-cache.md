# SOTA Test Plan — KV cache surface (R2KvStore / Turborepo / future sccache)

> **Provenance.** 307 candidate scenarios were enumerated by 7 parallel
> single-lens agents (functional · security · concurrency · edge/property ·
> resilience · observability · performance), then synthesized, deduplicated,
> **scope-triaged**, and prioritized here by the lead. Raw lens IDs are
> preserved for traceability (F-J-*, S-A-*, C-R-*, E-P-*, R-C-*, O-C-*, P-O-*).
> "Tudo tudo tudo" — but honestly partitioned: not every scenario is a unit
> test of *this* crate; some are route-layer, some are platform invariants.

---

## 0. Scope triage (the load-bearing judgment)

A SOTA suite is worthless if it pretends to test layers it can't reach. The 307
scenarios split into three rings:

| Ring | What | Where it's tested | Examples |
|---|---|---|---|
| **A — THIS suite (r2_kv + turbo route)** | KV store behavior + the turbo handler over both backings | `r2_kv` unit + `turbo_v8` integration tests, **InMemory AND R2-fake** | round-trip, tenant-prefix isolation, last-write-wins, key opacity, durability-across-rebuild, fail-closed audit, backend parity |
| **B — Route/auth layer (future cargo route + turbo route hardening)** | header-trust, cross-tenant 403, key validation, PROPFIND scoping | integration tests when the cargo route lands; some turbo-route now | S-A-13/14 teamId-vs-header, S-A-33 PROPFIND scoping, F-2/3 auth-header |
| **C — Platform invariants (Worker / DO / R2 / CF edge)** | things OUTSIDE this crate's reach | **documented as assumptions + asserted at the Worker/DO layer or in ops**, NOT faked here | S-A-04 CRLF header folding, S-A-34 request smuggling, C-R-29 DO hibernation, P-O-32 max_instances split-brain, P-O-35 region failover, R2 eventual consistency |

**Rule:** a Ring-C scenario is NOT silently dropped — it becomes a line in
`§6 Assumptions & cross-layer invariants` with its owner. Pretending a unit test
covers CF request-smuggling would be the exact dishonesty this plan refuses.

---

## 1. Ring A — the suite we build now (against BOTH backings)

Every behavioral row runs twice: `InMemoryKvStore` (contract source-of-truth) and
`R2KvStore` over a **persistent fake R2** (in-proc `HashMap` behind the
`R2S3Client` surface, survives "drop+rebuild" to prove durability without network).

### 1.1 Round-trip & opacity `[core]`
- **A-RT-1** GET∘PUT == identity, ∀ payloads: 0 B, 1 B, all-zero, all-0xFF, random binary, CRLF/`:`/null bytes, 5 MiB±1, 128 MiB. *(E-P-01..10, 43, 47, 48, 53)*
- **A-RT-2** Key opacity: `/`, `..`, `+`, `=`, `%2F` vs `/`, space, tab, newline, null, unicode/emoji/RTL, NFC≠NFD, case-sensitive, leading/trailing ws, numeric-leading-zeros — each a **distinct slot**, no normalization aliasing. *(E-P-15..31, 49; S-A-22/23 normalization)*
- **A-RT-3** Empty PUT overwrites to empty (not ignored); idempotent double-PUT; last-write-wins. *(E-P-32..34, 45, 46; F-J-22/23)*
- **A-RT-4** Miss returns NotFound (never fabricated empty/200). *(E-P-35; F-J-01)*
- **A-RT-5** `object_key` determinism + injectivity (already unit-tested; extend: same (tenant,key) → identical across rebuilds). *(E-P-50/51)*
- **A-RT-6** Backend parity: identical observable behavior InMemory vs R2-fake for every A-RT row. *(E-P-52)*

### 1.2 Tenant isolation `[sec][P0]` — the breach surface
- **A-ISO-1** PUT(A,k) invisible to GET(B,k) → miss. *(F-J-28, E-P-36, S-A-01)*
- **A-ISO-2** PUT(A,k,va) ∥ PUT(B,k,vb) → each GET returns its own. *(F-J-29, E-P-37, S-A-11)*
- **A-ISO-3** `object_key(A,k) ≠ object_key(B,k)` ∀ k (prefix injectivity; HMAC-derived). *(E-P-51, S-A-10 collision-resistance)*
- **A-ISO-4** **InMemory MUST key by full namespaced `(prefix,key)`, never bare key** — else cross-tenant via the InMemory path. *(S-A-48, S-A-31)* ← also an **impl assertion** on `InMemoryKvStore`.
- **A-ISO-5** Crafted traversal key (`../otherprefix/k`, absolute `/x`, double-encoded) stays inside caller namespace — stored as a literal opaque key, never resolves out. *(E-P-16, S-A-06/08/09)*
- **A-ISO-6** tenant-id edge shapes: valid UUID, non-UUID test form, 16-char pad/truncate boundary, very-long — never collide; reject tenant-id containing `/`,`\`,`.`,null. *(E-P-38..42, S-A-47)*

### 1.3 Concurrency & atomicity `[race]`
- **A-CC-1** N concurrent PUT same key SAME bytes → idempotent, final == bytes. *(C-R-01, E-P-32)*
- **A-CC-2** N concurrent PUT same key DIFFERENT bytes → GET returns **one complete writer's value**, never torn/interleaved. *(C-R-02, E-P-55, R-C-30)*
- **A-CC-3** PUT ∥ GET same key → GET sees full-old or full-new, never partial. *(C-R-03/04/07, R-C-31)*
- **A-CC-4** Concurrent first-write (two jobs, same new key) → exactly one logical object, both succeed. *(C-R-05, F-J-38)*
- **A-CC-5** Many tenants × many keys under load → **zero cross-tenant bleed** under all interleavings. *(C-R-22/12, P-O-21)* `[sec][race]`
- **A-CC-6** InMemory Mutex: held only for the map mutation, not across "I/O"; poisoned-lock recovery doesn't dark the surface. *(C-R-18/19)*
- **A-CC-7** Happens-before: a GET acquiring after a PUT's write is visible sees the new value (no stale read on InMemory). *(C-R-26)*
- Driven with `tokio::JoinSet`; assert whole-value integrity (hash the GET), never byte-equality-of-fragments.

### 1.4 Durability & resilience `[durability][fail-closed]`
- **A-DUR-1** **Write via R2-fake → drop handler → rebuild same bucket → still GET-able** (the entire reason R2KvStore exists vs InMemory). *(R-C-10/11, F-J-35)*
- **A-DUR-2** InMemory: object lost after rebuild — expected; status reflects ephemeral mode; no panic. *(F-J-34, R-C-38)*
- **A-RES-1** R2 GET transient 5xx/timeout/reset → Internal/503, **NEVER a false 404** (don't mask present data as absent). *(R-C-01/02/03, the dogfood discipline)*
- **A-RES-2** R2 PUT error → 5xx, no false-success, retry-safe; partial/aborted PUT leaves no readable half-object. *(R-C-05/06/07, C-R-08/28)*
- **A-RES-3** Build-time: creds absent / `R2_TURBO_BUCKET=""` / `R2_TDK_HEX=""` → clean InMemory fallback OR explicit fail, **never silent wrong-bucket** (the AC-500 lesson, now codified by `env_or`). *(R-C-12..16)*
- **A-RES-4** `block_in_place` from inside the runtime does not deadlock; timeout maps to 503, thread released. *(R-C-03/19/20/21)*
- **A-RES-5** R2 429 → propagate Retry-After verbatim (regression guard for the dogfood 429 bug); synthesize a safe default if header absent. *(R-C-08/09, P-O-10)*

### 1.5 Observability & audit `[audit][no-body-log]`
- **A-OBS-1** Audit row emitted BEFORE the effect: PUT→WRITE_ATTEMPT then WRITE_COMMITTED; GET→READ_ATTEMPT then READ_HIT/READ_MISS. *(O-C-01..04)*
- **A-OBS-2** Audit-sink down → fail-CLOSED: PUT 503 + zero bytes written + no COMMITTED row; GET 503 + zero bytes served. *(O-C-05/06, R-C-22..24)*
- **A-OBS-3** **No artifact bytes in ANY log/trace/audit/span — even on error mid-stream.** *(O-C-10/11/34/37)* `[sec]`
- **A-OBS-4** No PAT/token, no raw tenant PII (hashed only) in logs/audit/errors; error bodies leak no bucket/SQL/stack. *(O-C-12..16, S-A-42)*
- **A-OBS-5** Metric cardinality bounded: opaque key is NEVER a metric label; only fixed-cardinality `tenant_shard`. *(O-C-21/22)*
- **A-OBS-6** SLI emitted for EVERY op incl. 503/403 (avail counter + latency histogram); no silent omission. *(O-C-17..20)*

### 1.6 Functional journeys (integration, turbo route over R2KvStore)
- **A-FN-1** cold→warm: 10-task monorepo all-miss+PUT, then all-hit. *(F-J-01/02)*
- **A-FN-2** partial / incremental: subset miss, rest hit; old-hash artifact retained. *(F-J-03/07/08, F-J-25)*
- **A-FN-3** network-effect: dev A warms, dev B/C all-hit; team-shared. *(F-J-04/13/40)*
- **A-FN-4** branch switch / new branch identical inputs → all-hit. *(F-J-09/10)*
- **A-FN-5** status endpoint enabled/disabled; events POST accepted, store unaffected. *(F-J-30..33)*
- **A-FN-6** large monorepo 200 tasks cold then warm. *(F-J-26/27)*
- **A-FN-7** key >128 chars rejected (400 HashTooLong) — NOT silently stored. *(E-P-13, S-A-24/25)*

---

## 2. Ring B — route/auth layer (turbo now where possible; cargo route when it lands)
- **B-AUTH-1** missing `x-corelink-tenant-id` → fail-CLOSED (reject, never wildcard/first-tenant). *(S-A-02/14/31, O-C-03)* `[P0]`
- **B-AUTH-2** `teamId` query ≠ injected tenant → 403; teamId never used for namespace. *(S-A-13/14)* `[P0]`
- **B-AUTH-3** cross-tenant GET/PUT → 403 + DENY audit row. *(O-C-08/09, S-A-01/11)* `[P0]`
- **B-AUTH-4** fail-OPEN regression guard: every error path returns non-2xx; a panic never becomes a 200-empty (false hit). *(S-A-30/31)* `[P0]`
- **B-AUTH-5** range / conditional-GET / response-header metadata do not leak cross-tenant existence or size. *(S-A-36/37/38)*
- **B-AUTH-6** (cargo/sccache route) PROPFIND/listing scoped to own prefix only, or disabled; same auth middleware as turbo. *(S-A-32/33)* `[P0]`
- **B-DOS-1** oversized body → 413 before buffering; opaque bytes never decompressed (zip-bomb). *(S-A-20/21, R-C-28)*

---

## 3. Ring C — platform invariants (documented, owned elsewhere — NOT faked here)
Each is a cross-layer assumption with an owner; asserted at the Worker/DO/ops layer.
- Worker strips client-supplied tenant headers, injects exactly one; rejects duplicates/CRLF/`alg=none` PAT. *(S-A-04/05/40/41)* → **Worker auth, GAP-5 lane**
- Container not publicly routable / Worker→container shared-secret. *(S-A-15/34/35)* → **infra/wrangler**
- DO funnel: 429+Retry-After under burst, no corruption; hibernation/eviction journaling; no split-brain at max_instances. *(C-R-13/14/29/33, P-O-09/32)* → **DO layer + the campaign-#1 concurrency work (see FINDING-per-tenant-write-ceiling.md)**
- R2 eventual consistency / read-after-write; object-lock; lifecycle eviction. *(C-R-20, R-C-40/41)* → **R2 semantics, documented**
- Region failover, free-egress routing, region residency placement. *(P-O-35/38, O-C-25/26/39)* → **ops + residency config**
- GDPR erasure purges namespace; eviction/retention audited; audit chain tamper-evidence. *(O-C-27..31, P-O-27)* → **platform compliance**
- Tenant-id never recycled; revoked-PAT replay rejected. *(S-A-17/18)* → **identity/Clerk + PAT lifecycle**

---

## 4. Harness design
- **Two backings, one body.** Parametrize Ring-A behavioral tests over `InMemoryKvStore` and `R2KvStore`-over-`FakeR2`. `FakeR2` implements the `R2S3Client` surface against an `Arc<Mutex<HashMap<String,Vec<u8>>>>` that **persists across handler drop+rebuild** (durability without network).
- **Property/fuzz:** `proptest` for A-RT-1/2/3 invariants (round-trip, opacity, idempotence, LWW, size-preservation, binary-transparency, prefix injectivity). Container is on the proptest-density allowlist (CLAUDE.md) — fund the case count.
- **Races:** `tokio::JoinSet` over N tasks; assert whole-value integrity by hashing the GET result; never assert on byte fragments.
- **Live smoke (opt-in):** a `--features live-r2` lane (≤1 KiB, throwaway bucket) mirroring `clw-conformance` — never in the default CI lane.
- **Fault injection:** `FakeR2` modes — return 5xx, timeout, reset, 429(+/-Retry-After), truncated body, wrong-Content-Length — to drive Ring-A §1.4.

---

## 5. Priority & sequencing (what blocks merge)
1. **P0 / `[sec]` — merge-blockers:** all of §1.2 (isolation, esp. A-ISO-4 InMemory-namespacing), §1.5 A-OBS-2/3 (fail-closed, no-body-log), Ring-B B-AUTH-1..4/6. A red P0 blocks merge, no exceptions (rigor compact).
2. **Core correctness:** §1.1 round-trip/opacity, §1.3 atomicity (A-CC-2 torn-write), §1.4 A-DUR-1 durability, A-RES-1 no-false-404.
3. **Resilience/observability remainder:** §1.4 rest, §1.5 rest.
4. **Functional journeys:** §1.6.
5. **Perf/soak harness:** P-O-* as a separate bench lane (not unit CI) — latency bounds, burst ceiling, hit-rate KPI, soak; feeds the case-study numbers, gated off the fast lane.

---

## 6. Assumptions & cross-layer invariants (Ring C, owner-tracked)
Listed in §3 — each MUST be asserted by its owning layer. This suite **documents
the dependency** rather than faking coverage. Reviewer: a Ring-C item appearing
as a green "unit test" here is a RED flag (false confidence).

---

*Counts: 307 enumerated → Ring A (~70 consolidated tests, 2 backings) · Ring B
(~10) · Ring C (~15 documented invariants). Dedup ratio ≈ 4:1 across lenses
(torn-write, cross-tenant, fail-closed each surfaced from 3–4 angles — kept once,
strongest framing).*
