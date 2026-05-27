# Agent R4 — Lote 10.1 S-01 WI Review (independent reviewer)

> Reviewer: Agent R4 (Claude Opus 4.7, 1M ctx) · Date: 2026-04-25 · Mode: independent SOTA review
> Inputs: WI-S01-002 .. WI-S01-007 (six new specs); WI-S01-001 (baseline reference, not scored).
> Cross-checked against: `specs/04_sprints/S01/sprint.md`, `specs/04_sprints/S01/_spec_contract.md`, `specs/03_architecture/storage_semantics_matrix.md`.

---

## Veredito Geral

The six NEW WIs are **GO-WITH-FIXES**: structurally complete, internally consistent on the high-level invariant story, and faithful to the WI-S01-001 baseline template. They will not, however, be implementable as-written without three classes of fixes: (1) the WI-S01-005 BatchUpdateBlobs handler does not specify the dual-write reconciliation between R2 and D1 (a known armadilha called out in `storage_semantics_matrix.md §3.2` — "se Container reinicia entre R2 PUT e D1 INSERT, blob existe em R2 mas não no index — inconsistência"); (2) WI-S01-002 conflates "single-blob ≤ 5 MiB" with the BLAKE3 verify scope but the canonical R2 simple-PUT limit is 5 GB and the 5 MiB number is a CoreLink Worker-memory budget choice that needs an ADR or explicit cite, not a side-note; (3) WI-S01-007 sets a TLC PR runtime budget (≤10 min) without naming a state-space bound or a TLC `-deadlock` / `-difftrace` policy, which means the gate is theatre until parameterized. None of these is a structural rejection — they are all sprint-window fixes. Average score 7.4/10. Net: ship after fixes below; do not seal S-01 until WI-005 reconciliation contract is named and WI-007 TLC bounds are committed in the spec.

---

## Per-WI Findings

### WI-S01-002 (BLAKE3 verify-at-write)
**Score: 8/10 — GOOD**

Strong overall. Type-driven `VerifiedBody` envelope is the right pattern; constant-time via `subtle::ConstantTimeEq` is correctly chosen and cited; SIMD-in-WASM perf claim (1.4 GB/s, 3.5ms for 5 MiB) is realistic for the `blake3` crate's portable-simd backend (true `simd128` WASM SIMD is gated by feature detection at runtime — flagged below). Gherkin coverage is solid (8 scenarios, including timing oracle and adversarial). STRIDE/LINDDUN delta is filled (not just inherited). Cost regression gate §14.10 wired.

Specific gaps:

- **Constant-time-of-what?** The §9.2 rationale says `subtle` "forces full-byte comparison sem branch"; that is correct for the *compare* step. But the spec never addresses **constant-time of the hash recompute itself** vs early-abort — i.e., the only constant-time path that matters is the compare; the BLAKE3 compute is necessarily O(n) and not constant-time per byte. The Gherkin scenario "verify time variance < 5%" measures *total verify* including compute. For very different blob sizes or for adversarially crafted small bodies, variance will trivially exceed 5%. Either restrict the SLO to fixed-size buckets or rephrase the scenario as "compare-step variance" (the actual security-relevant quantity).
- **`black_box` not enforced.** R-WI-004 ("Constant-time leak via compiler optim") names `black_box` in the *benchmark* but the spec doesn't require `criterion::black_box` or `core::hint::black_box` around the actual verify call in the benchmark, which the LLVM optimizer will otherwise hoist or constant-fold away. Move from "mitigation" wording into a DoD checkbox.
- **WASM SIMD detection is asserted, not measured.** `Scenario: WASM compile target works` says "BLAKE3 SIMD intrinsics are detected at runtime" — `blake3` 1.5+ gates `wasm32_simd` behind cfg flag, and Cloudflare Workers do support `simd128`, but Workerd's exact intrinsic coverage shifts. Add an explicit benchmark assertion that `Hasher::update` uses the `simd128` path (e.g., feature-detect via `is_wasm_simd128_supported()` or a startup log) — otherwise R-WI-002 is undetectable in production.
- **Dual-hash fallback decision is hand-wavy.** §6.2 says "Dupla-hash SHA-256 fallback: anti-scope GA; defense-in-depth opcional pós-GA Q1 se CVE em BLAKE3 emerge." But §28 R-WI-001 mitigation lists "dupla-hash fallback opcional" — these are inconsistent. Either commit to a "BLAKE3-only with documented CVE response RTO" or scope the fallback. As written, an auditor will reject the contradiction.
- **Audit emit on mismatch is not idempotent.** §6.1.5 emits `corelink.cas.poisoning_attempt` per mismatch. A burst of 10k retry-storms from a buggy client will flood the audit chain with identical events. Add per-(tenant, claimed_digest, request_id) dedup window or rate limit, otherwise this becomes a self-DoS amplifier (FM-amplification adjacent).
- **`Digest::from_hex` is the sole client input parse but has no fuzz target listed.** §13 lists `digest_parse.rs` fuzz target only in WI-S01-007 — the linkage is correct but DoD §11 of WI-002 should checkbox "fuzz target wired in WI-007".

### WI-S01-003 (R2 adapter single-blob)
**Score: 7/10 — GOOD with one technically wrong claim**

Path-construction discipline (`R2Writer::put` is the only code path) and `If-None-Match: *` for write-once are correctly chosen. Per-region buckets and trait `BlobStore` abstraction give the right escape hatch. STRIDE rows are filled. Property test 100k cross-tenant is sized appropriately.

Specific gaps:

- **5 MiB single-blob limit is unjustified vs canonical 5 GB.** `storage_semantics_matrix.md §3.1` documents the R2 simple-PUT cap at 5 GB / multipart at 5 TB. WI-S01-003 picks 5 MiB but never cites the constraint that drives it (Worker subrequest body limit ≈100 MiB paid; **Worker memory budget ≈128 MiB** as discussed in WI-002). The number 5 MiB is also where R2 multipart **becomes available**, not where simple-PUT must end. As-written, an implementer will reasonably ask "why not 25 MiB?" Decision needs an ADR or explicit cite to a source-of-truth (e.g., REAPI BatchUpdateBlobs typical inline blob size; CF Workers TTFB budget). Without it the gate is arbitrary.
- **`If-None-Match: *` is silently unsupported in some R2 SDK paths.** Cloudflare's R2 native binding (Workers JS) supports conditional headers; the Workers Rust binding (`worker` crate) coverage of conditional puts has been historically incomplete (re-check at impl time). R-002 says "Integration test verifies; cargo-audit upstream" — that's not enough. Add a DoD checkbox: "`If-None-Match: *` semantic verified end-to-end in CF dev (`wrangler dev`) AND production simulator, NOT just in unit-test mock." Otherwise the conditional is a paper guarantee.
- **Idempotent semantics map 412 → Ok(()) without verifying body equality.** Scenario "Idempotent duplicate PUT" is correct in spirit, but the parenthetical "(first writer wins; INV-CAS-IDEMPOTENCY ensures both bodies are byte-identical)" silently relies on WI-S01-002's hash check — i.e., if a buggy client sends correct digest with wrong body, the digest mismatch at WI-002 catches it; if it sends correct digest with correct body but the *first* writer wrote different content (possible only if INV-CAS-INTEGRITY was bypassed), the duplicate accept is wrong. In other words, this idempotency story is only safe iff verify-at-write is never bypassed. Should be an explicit dependency: "Pre: VerifiedBody envelope guards against poison; if VerifiedBody is bypassed, this idempotent path becomes a poisoning amplifier." Make the coupling visible in §12 invariants.
- **`SSE-S3` is asserted active but not enforced.** R2 enables SSE-S3 by default, but the spec says "verify ativo" with quarterly EVT-028 — that's slow. Add a startup-time check: `R2Writer::new` queries bucket policy and refuses to construct if SSE not enabled, OR fail-loud if the bucket admin API surfaces a "not encrypted" signal.
- **Cross-region binding misconfiguration is the main residual risk and only mitigated by `wrangler.toml` review.** R-004 ("Per-region binding misconfigured" prob=M) deserves more than `wrangler.toml CI verify`. Suggested addition: at deploy time, the Worker must call a sentinel object PUT to its own bucket and assert the PUT succeeded *and* the GET reads the canonical region marker (e.g., bucket name in metadata) — fail-fast on mismatch.
- **Path sharding factor 256×256 is fine but listing perf is undertested.** Chaos experiment §15 doesn't include a "list 100k objects in single prefix" test. S-06 GC will care, and the design decision should at least note "verified empirically in S-06 CHAOS-S06-001". Cross-link absent.
- **No backpressure mention.** Concurrent put storms can saturate the per-bucket request rate (R2 has internal limits). Spec doesn't reference circuit breaker or PAT-RETRY-IDEMPOTENT-001 with explicit RPS ceiling.

### WI-S01-004 (D1 schema blob-meta)
**Score: 7/10 — GOOD; one high-impact factual concern**

Composite `(tenant_id, digest)` PRIMARY KEY is correct and answers the cross-tenant collision question well — same digest in two tenants produces two distinct rows, no collision. Partial indexes on `deleted_at IS NULL / IS NOT NULL` is the right choice for D1 SQLite and avoids tombstone bloat. Atomic single-row UPDATE for refcount is correctly identified as race-safe in SQLite.

Specific gaps:

- **D1 transactions across multiple statements: BIG GAP.** The spec says (§2.2) `BEGIN; UPDATE ... ; COMMIT` is atomic, citing "D1 single-row update é atomic em SQLite engine". That's true for a *single statement* but `BEGIN/COMMIT` over Workers ↔ D1 RPC is a different story: as of 2026, D1 **does not support multi-statement client-driven transactions over the Worker binding** (transactions run inside a single `prepare()...batch()` or a single SQL string). If the implementation issues `BEGIN`, then `UPDATE`, then `COMMIT` as three separate `db.prepare()` calls, the engine will either error or run them in autocommit mode without the ACID grouping the spec assumes. Refcount race safety is then not actually guaranteed and the property test in §10.4.2 will be flaky. **Required fix**: rewrite §9 or §6.1.2 to either (a) use a single statement `UPDATE blob_meta SET refcount = refcount + 1 WHERE ... RETURNING refcount` (atomic-by-row, which is fine) OR (b) wrap multi-statement work in `db.batch([...])` which D1 executes as one transaction. The current text reads as if multi-statement RPC transactions are supported and that's a footgun.
- **`deleted_at` BIGINT vs INTEGER.** Spec uses `INTEGER` (epoch seconds, 32-bit "OK até 2106"). 32-bit signed actually rolls over in 2038, not 2106 (you're thinking of unsigned 32-bit). For epoch *seconds* in a SQLite INTEGER (which is 64-bit by default), this is moot — but the rationale text is incorrect and will get caught in code review. Either fix the rationale ("SQLite INTEGER is 64-bit native; Y2106 is the unsigned-32 horizon; we're safe to year ~292B") or pick `INTEGER` and stop justifying it with the wrong number.
- **Missing UNIQUE on (digest) cross-tenant — by design, but not stated.** §8 scenario 4 ("Cross-tenant same digest is allowed") is correct. But a reader expecting CAS dedup-across-tenants might miss this is *intentional* (per-tenant refcount; cost attribution; deletion isolation). Add a one-line design note: "We deliberately do not deduplicate physical R2 blobs across tenants in S-01; cross-tenant dedup is a Sprint S-XX consideration with privacy/billing trade-offs."
- **No CHECK constraint protecting refcount from going negative.** A bug in `decrement_refcount` should not silently set refcount to -1; spec should enforce `CHECK (refcount >= 0)` and return an error if violated, especially given §28 R-001 (refcount race) is rated MEDIUM residual.
- **No FOREIGN KEY to a `tenants` table is fine** (per-region D1 sharding; tenants table likely lives elsewhere) but worth stating explicitly so a reader doesn't ask.
- **Migration rollback policy is "additive-only" but no enforcement.** §25 says "additive-only policy + ADR per disruptive change" — fine. CI gate should *enforce* it (e.g., a script that diffs migrations vs `main` and fails on any `DROP`/`ALTER COLUMN`). Move from prose to WI-S01-007 §6.1.

### WI-S01-005 (REAPI BatchUpdateBlobs)
**Score: 6/10 — AVERAGE; review-fixable but one P0 reconciliation gap**

This is the WI with the most surface area and the largest gap. The handler design (auth → VerifiedBody → R2 → D1 → audit) is correct in sequence; per-blob result aggregation is the right Bazel ergonomic choice; size-guard via byte-counting (not Content-Length trust) is correct. REAPI v2 conformance via `bazelbuild/remote-apis` test suite is the right gate.

Specific gaps:

- **(P0) R2↔D1 dual-write reconciliation is unspecified.** The canonical source flagged this exact armadilha: "se Container reinicia entre R2 PUT e D1 INSERT, blob existe em R2 mas não no index — inconsistência. **DEVE** implementar reconciliação periódica" (storage_semantics_matrix.md §3.2). The WI describes the orchestration order but never says what happens when:
  - R2 PUT succeeds, D1 INSERT fails → orphan R2 blob (cost leak, GC must clean).
  - D1 INSERT succeeds, R2 PUT fails → orphan D1 row pointing at non-existent blob (read-side 404; AuthZ confusion).
  - R2 PUT and D1 INSERT both succeed but audit emit fails → §9.2 says "fail-closed write rolled back (R2 best-effort delete; D1 row not inserted)" — but D1 was already inserted in the orchestration order described! Order is contradictory.
  
  **Required fix**: pick an order (recommended: R2 PUT first, D1 INSERT second, audit emit *before* COMMIT-equivalent OR audit emit asynchronously with at-least-once retry tracked in `audit_outbox` table — outbox pattern). Document a reconciliation cadence (e.g., "GC sweep S-06 detects orphan R2 blobs older than 5 min and either ingests or deletes"). Without this, every retry storm produces a small leak.
- **(P0) "Audit emit fail-closed = R2 best-effort delete" is dangerous.** R2 `delete` on a blob that was successfully written by a *different concurrent writer* (idempotency) would delete a legitimate blob. The fail-closed handler must check "is this MY put I just made, or is this an existing blob from a prior put I'm mistakenly deleting?" — the answer depends on whether `If-None-Match: *` returned 200 or 412 on this call. Spec must distinguish: only delete on R2 if THIS call's PUT got 200 (we created it); never delete on 412 (someone else owned it). This nuance is missing.
- **gRPC status code mapping for `OUT_OF_RANGE` (size limit) is non-standard.** REAPI BatchUpdateBlobs blob-too-large is typically `INVALID_ARGUMENT` (3) or `RESOURCE_EXHAUSTED` (8), not `OUT_OF_RANGE` (11) which is for read-past-EOF semantics. Check the REAPI v2 protobuf definition before committing — `bazelbuild/remote-apis` conformance suite will be the arbiter.
- **PAT scope enforcement only mentions `cache:w`.** Real REAPI deployments need finer-grained scopes (cache:w:cas, cache:w:ac, cache:r, cache:r:cas, etc.). S-03 will introduce this but the handler signature should already accept a scope-set rather than a single scope string, otherwise a refactor is forced.
- **No handling of REAPI `compressor` field.** REAPI v2 added optional zstd compression on `WriteRequest.data`. Spec doesn't mention whether server accepts compressed bodies (and if so, hashes the *decompressed* content, since BLAKE3 is computed on logical content). At minimum, anti-scope this explicitly: "compressor=IDENTITY only at S-01; zstd in S-XX".
- **Per-blob result: what does `Status.code = OK` mean for a 412 idempotent dup?** Bazel client retry logic differs based on `OK` vs `ALREADY_EXISTS` (5). Pick the canonical mapping (REAPI says `OK` for already-uploaded is fine) and reference it.
- **Latency SLO p99 ≤ 1s for 50 small blobs is plausible but Worker CPU 50ms budget per request is the real constraint.** Each blob hash + R2 PUT + D1 INSERT in 50ms × 50 = 2.5s of work serialized; even parallelized via `try_join_all`, R2 round-trip dominates. This needs a concurrency budget statement (e.g., "process up to 16 blobs concurrently within Worker; queue rest"). Without it the SLO is asserted, not architected.
- **Risk register is thin (5 rows) for a HIGH_RISK API surface WI.** Compare to WI-S01-002 which has 9 rows. Missing risks: gRPC HTTP/2 trailer handling, malformed proto deserialization (covered by fuzz but not flagged here), client bug → retry storm amplification, audit rate-limit interaction with mismatch storm.

### WI-S01-006 (property tests 10k)
**Score: 8/10 — GOOD**

Right tool (proptest > quickcheck for shrinking + persistent regression DB); right scale (10k PR / 100k nightly is defensible — 10k gives ~3-sigma confidence per property; 100k extends to ~5-sigma). Adversarial generators (path collision, HMAC truncation, refcount race) are well-aimed at the actual threat model. Tokio-based concurrent property test for refcount is the correct approach.

Specific gaps:

- **"prop_path_collision_resistant 100k pairs → 0 collisions" is mathematically trivial.** With HMAC-SHA256 truncated to 16 bytes (128 bits), birthday-bound collision is 2^64; observing 0 collisions in 100k pairs is overwhelmingly the only outcome (P(collision) ≈ 5×10^-30). The test verifies *injectivity* per call, not collision *resistance* in the cryptographic sense. Either rename ("path injectivity sanity check"), reduce iter count (1k is enough), and add a **stronger** test: assert that paths derived from distinct (TDK, tenant_id) pairs differ statistically as expected (Hamming distance distribution). The current test is theatre; the stronger test catches a botched HMAC implementation that is subtly biased.
- **"prop_refcount_race_safe: 100 inc + 100 dec → final == initial" is necessary but not sufficient.** This catches drift, not torn updates. Add: assert that intermediate snapshots taken during the storm are always non-negative AND ≤ 100 (the running max). Without intermediate assertions, a bug that lets refcount transit through -50 → +50 (averaging to 0) passes the test.
- **proptest cases counts CI runtime budget conflicts with parallelism.** Spec says "PR ≤ 30s, nightly ≤ 5 min". 10k iter of `prop_tenant_isolation` doing 1000 ops each = 10M ops; even at 1µs/op = 10s, plausible. But `prop_refcount_race_safe` with Tokio and 100k iter (nightly) × 200 tasks each = 20M task spawns; that will not fit in 5 min. Either reduce nightly iter for the concurrent test, or split CI runtime budgets per property.
- **Regression DB versioning policy missing.** §6.1.5 says "Regression DB versioned em git" — but proptest's regression file format includes counter-example seeds. When a fix lands, do you delete the old seed (it now passes; future regressions of the *same shape* won't be caught) or keep it (slowly grows; eventually slow)? Pick one (recommendation: keep, prune annually).
- **No assertion that property tests cover all 4 CRITICAL invariants per §12.** Coverage matrix (invariant → property test) is implied but not tabulated. Add a §12.1 mapping table.
- **Adversarial generators should use seeded mutation, not pure random.** §9 doesn't mention `proptest::collection::vec` strategies vs derived `Arbitrary`. Spec should commit to either `derive(Arbitrary)` (less control) or hand-written `Strategy` for tenant_id (UUID v7 vs v4 distribution matters: spec elsewhere uses v7).

### WI-S01-007 (CI/TLC gate + SBOM)
**Score: 7/10 — GOOD with one P0 enforcement gap**

Workflow surface is right (TLC + clippy + SAST + cargo-audit + cargo-deny + cyclonedx + cosign keyless + nightly fuzz). Cosign keyless via OIDC + Rekor inclusion is the SLSA L3-aligned choice. `deny.toml` license allowlist (MIT/Apache-2.0/BSD/ISC/MPL-2.0/Unicode-DFS-2016) is correct and consistent with prior ADRs.

Specific gaps:

- **(P0) TLC budget without state-space bound.** §9.2 says "TLC explicit-state suficient para small bounds (3-5 tenants × 10 ops)". §1 says runtime ~5-15min; §15.1 says "Inject fake invariant violation em test branch: verify TLC catches". But §6.1 doesn't pin the `MC_*.cfg` constants. TLC explicit-state explores ALL reachable states; without a model bound (e.g., MaxTenants=3, MaxOps=10, MaxBlobSize=2), TLC will run for hours or OOM on 16 GB GitHub runners. Required: each `tenant_isolation.cfg`, `cas_integrity.cfg`, `gc_correctness.cfg`, `audit_immutability.cfg` MUST commit explicit constants AND a deadline (`-deadlock`, `-coverage`, max workers). Without this, the gate either doesn't run reliably (timeout flake) or doesn't actually check (silently truncates state space).
- **CycloneDX 1.5+ NTIA elements: format check is not full validation.** "NTIA minimum elements" is a real spec but cyclonedx-cli's `validate` only checks JSON Schema conformance, not NTIA-specific semantic completeness (supplier name, component name, version, unique id, dep relationships, author, timestamp). For SOC 2 evidence quality, run the NTIA-specific tool (e.g., `sbomqs` from interlynk) or document what subset of NTIA is covered. As written, "NTIA minimum check passes" is hand-wavy.
- **Cosign Fulcio outage mitigation is "grace period 24h" without mechanism.** R-002 says cache previous Rekor proof + grace period 24h. But the workflow as described will hard-fail on `cosign-sign` step if Fulcio is down (no signed cert → no signature). "Grace period" means what? Skip signing? Use a cached cert? (Fulcio certs are 10-min lived; can't cache.) Either accept "release blocked during Sigstore outage" (and write a runbook) or implement a fallback (e.g., long-lived KMS key as break-glass with an alert). Don't pretend it's solved.
- **No supply-chain hash of TLA+ tools.** TLC runs as a Java jar. The job pulls some version of `tla2tools.jar`. Without pinning + verifying its hash, a compromised mirror could substitute a TLC that "verifies" everything successfully. Pin the jar URL + SHA-256 + verify before exec.
- **`fuzz` job runtime 1h × 3 targets nightly — sequential or parallel?** Spec doesn't say. If sequential, that's 3h nightly minimum + overhead; if parallel, runner spec must allow 3 jobs concurrent. State the matrix.
- **`pull_request_target` anti-pattern is correctly avoided** but the spec doesn't address forks. Required: deny external-fork PRs from running secrets-bearing jobs (cosign, SBOM publish). Use `if: github.event.pull_request.head.repo.full_name == github.repository`.
- **No mention of action pinning.** GitHub Actions like `actions/checkout@v4` are usually pinned by tag, but tag-pinned actions can be rewritten. SLSA L3 demands SHA-pinning all third-party actions. Spec is silent.
- **CI runtime ≤ 20 min PR is aspirational without measurement.** TLC alone is 5-15 min; Rust build (release WASM) is 5-10 min cold; tests another 5 min; SAST 1-2 min. Cold cache: realistic 25-30 min. Warm cache: 12-18 min. Document cache strategy (sccache? cargo cache?) and budget separately.

---

## Cross-WI Consistency

- **Crate naming consistency: PASS overall, with one drift.** WI-S01-001 ships `corelink-tenant-path`. WI-S01-002 ships `corelink-hash`. WI-S01-003 references both correctly via `TenantPath::derive` and `VerifiedBody::new`. WI-S01-007 §13 lists `crates/corelink-tenant-path/fuzz/fuzz_targets/decode.rs` — consistent. **Drift**: WI-S01-001's namespace says `crates/tenant-path/` (without the `corelink-` prefix) in §13 path listing, but WI-S01-007 uses `crates/corelink-tenant-path/`. Pick one. (Recommendation: `crates/corelink-tenant-path/` for consistency with `corelink-hash` / `corelink-worker`.)
- **Dependency graph: PASS but linear.** WI-005 hard-blocks on WI-001/002/003/004; WI-006 hard-blocks on WI-001..005; WI-007 hard-blocks on WI-001..006. This means WI-007 cannot start until day ~14 of the sprint (S-01 is 2 weeks). For risk reduction, WI-007 should start in parallel with WI-001 (the `cas_foundation.yml` skeleton + TLA+ check job is independent of Rust crates) and grow as crates land. Update §18 of WI-007 to reflect "soft" vs "hard" blocking (TLA+ check is hard-blocked by spec existence; Rust test job is soft-blocked by crate existence).
- **INV-CAS-IMMUTABILITY enforcement story has a contradiction.** Sprint contract `_spec_contract.md` line 108 says "INV-CAS-IMMUTABILITY (CRITICAL): `INSERT IF NOT EXISTS` em D1 + R2 versioning." But WI-S01-003 §7 anti-scope says "❌ R2 versioning (blobs immutable post-write; INV-CAS-IMMUTABILITY enforced)" — i.e., the WI explicitly does NOT use R2 versioning, and instead uses `If-None-Match: *`. Either the sprint contract is stale ("R2 versioning" was an early design that got replaced) or WI-003 contradicts the contract. Reconcile (recommendation: update sprint contract to "INSERT OR IGNORE in D1 + If-None-Match in R2"). This is a P1.
- **Sprint sprint.md line 122 also says "INSERT IF NOT EXISTS"** but WI-S01-004 spec uses `INSERT OR IGNORE`. These are SQLite synonyms but pick one wording everywhere.
- **WI-S01-007 enforces CI gates for invariants but the TLA+ specs themselves are referenced via filename only.** No WI lists where `tenant_isolation.tla v3` actually lives or its ADR; cross-link to `specs/tla/` (referenced in storage_semantics_matrix.md §3.9). Make the path explicit in WI-007 §6.1.
- **Audit emission semantics differ between WI-002 and WI-005.** WI-002 §6.1.5 emits `corelink.cas.poisoning_attempt` directly from the hash check; WI-005 §1.6 emits "Audit emission" but doesn't say where in the orchestration. If both WI-002 and WI-005 emit audit events for the *same* mismatch, you get duplicates. Specify: WI-002 emits at the verify step; WI-005 emits success events only.
- **Reuse of `corelink-keys` crate** (mentioned in user prompt) is **not referenced anywhere in the six WIs.** WI-S01-001 alone owns key derivation (`TenantDerivationKey`). If `corelink-keys` is the future home of TDK (S-02+), document the migration. If not, scrub the name from the user prompt context.

---

## Technical Accuracy Issues

1. **WI-002 constant-time scope is mis-described.** Constant-time applies to the digest *compare*, not the *compute*. (See WI-002 review.)
2. **WI-002 SHA-256 dual-hash decision is internally contradictory** between §6.2 anti-scope and §28 R-WI-001 mitigation.
3. **WI-003 5 MiB single-blob limit lacks canonical citation.** R2 simple-PUT is 5 GB per `storage_semantics_matrix.md §2`. The 5 MiB choice is a CoreLink-specific Worker memory budget, not an R2 protocol limit. Spec should explicitly cite the binding constraint (Worker memory ≈128 MiB; concurrent N writes × 2 buffers = budget).
4. **WI-004 multi-statement transactions over D1 binding may not be supported as written** (BEGIN/UPDATE/COMMIT). Use single-statement `UPDATE ... RETURNING refcount` or `db.batch([...])`.
5. **WI-004 INTEGER timestamp Y2106 claim is off by 68 years** (32-bit signed rolls in 2038; SQLite INTEGER is 64-bit anyway). Cosmetic but wrong.
6. **WI-005 audit-fail-rollback ordering is contradictory** (D1 INSERT happens before audit emit per §1, but rollback says "D1 row not inserted").
7. **WI-005 OUT_OF_RANGE gRPC code for size limit may not match REAPI conformance suite.** Likely should be RESOURCE_EXHAUSTED or INVALID_ARGUMENT; defer to bazelbuild/remote-apis test.
8. **WI-006 prop_path_collision_resistant is statistically vacuous** (P(collision in 100k pairs) ≈ 0 by birthday bound; test catches nothing a 1k-pair test wouldn't).
9. **WI-007 TLC budget without state-space bound makes the gate non-deterministic** (timeout vs success depends on Java GC tuning, not invariant satisfaction).
10. **WI-007 CycloneDX NTIA validation conflates schema check with completeness check.**
11. **WI-007 cosign Fulcio outage "grace period 24h" lacks mechanism** (Fulcio certs are 10-min lived; nothing to cache).

---

## Missing Gaps for Production

- **Backpressure / circuit breaker discipline.** No WI commits to a concrete circuit breaker library, threshold, or open-state behavior. WI-005 SLO depends on R2 + D1 latency staying nominal; under degradation, the orchestration described will queue indefinitely or pile up Worker subrequests until CPU 50ms exhausts (FM-403). Add CB pattern reference in WI-003 + WI-004 + WI-005.
- **Hot-spot D1 scenario for `last_accessed_at` updates.** Read path (S-02) will update `last_accessed_at` on every GET; storage_semantics_matrix.md §3.3 explicitly warns "cria hot-spot em D1 — DEVE batch via buffer em DO". This isn't yet S-01 scope but the schema (WI-004) commits `last_accessed_at INTEGER NOT NULL` with no index — fine for now, but flag as "S-02 must batch updates via DO; otherwise this column becomes a write hot-spot at scale."
- **Cold-start Worker behavior.** First request after isolate cold-start incurs ~50-300ms of Worker init overhead. Spec asserts p99 ≤ 1s for batch of 50 blobs; need a separate metric/SLO for cold-start vs warm. None of the WIs mention this.
- **Retry storm amplification on hash mismatch.** A buggy client retrying a hash-mismatched body without recomputing will hammer the audit chain (WI-002) and pollute metrics. Add per-client rate limit on `COR_CAS_DIGEST_MISMATCH` after N consecutive failures (e.g., 10 in 1 min → return 429 with Retry-After).
- **R2 region failover.** §15.3 chaos says "R2 region failover (CF region offline): verify graceful degrade vs failover (S-14 future)" — but degrade behavior at GA must be stated. If wnam goes down, do EU tenants pinned to wnam fail, or do they read from weur? S-14 will solve; S-01 must declare current behavior (probably: tenant-pinned region; outage = unavailable; documented in SLO degradation matrix).
- **No PII discipline on audit events.** `corelink.cas.poisoning_attempt` carries `claimed_digest, computed_digest, request_id`. If body bytes leak via stack-trace or panic message, that's a privacy issue. WI-002 should commit "no body bytes in audit; only digest hex".
- **Cost monitoring per-tenant for poisoning attempts.** A tenant could be the source of a poisoning storm (intentional or buggy CI). Cost (R2 ops + audit chain) should be billable to that tenant; otherwise CoreLink eats the cost. Not S-01 scope but flag for S-10 billing.
- **TLA+ refinement mapping is mentioned but not concretized.** WI-S01-001 §12.2 says "Property test é a 'ponte' entre TLA+ abstract e Rust concreto." This is hand-wavy; a real refinement mapping (TLA+ state vars → Rust types → property assertions) is missing. Acceptable for sprint window; flag for S-12 formal-verification hardening.
- **No deploy gate for D1 schema sync across regions.** WI-004 §6.1.5 says "CI gate: schema validation cross-region (`scripts/check_d1_schema.py` planned forward)" — "planned forward" is not a sprint commitment. Either include in WI-007 or split into WI-S01-008.

---

## Comparison vs WI-S01-001 Template

The WI-001 baseline is genuinely high quality (≥300 word narrative, full STRIDE+LINDDUN, 12 sub-tasks PERT-weighted, type-driven security pattern explained, 32 sections all populated, ADR-0015 spawned for HMAC algo choice). The six new WIs **mostly** maintain this bar with one degradation pattern:

- **Quality maintained or matched**: WI-S01-002 (BLAKE3) and WI-S01-006 (proptest) are at parity with the baseline. WI-S01-002 actually goes further than WI-001 in §28 (9 risks vs 4) and adds the `VerifiedBody` type-driven envelope which is a genuine SOTA pattern.
- **Quality slightly degraded**: WI-S01-003 (R2) and WI-S01-004 (D1) are noticeably terser in §26 (Security/Privacy), §27 (KT), and §28 (Risk Register) — 6 risks for D1 vs WI-001's 4 plus expanded text. They use shortened table formats ("STRIDE: tampering — atomic transactions enforce") that lose specificity. Acceptable for medium-criticality items but these are HIGH_RISK lane, so the bar should be the WI-001 expanded-prose form.
- **Quality measurably degraded**: WI-S01-005 (REAPI handler) is the largest WI by surface area but has only 5 risks in §28 and a one-line §14 ("Standard 14.5.1..10") that just delegates to the sprint contract instead of customizing. Given this is the external API surface (highest adversarial pressure), it deserves the most expanded treatment, not the most compressed. Specifically: §26 Security & Privacy says "STRIDE + LINDDUN per S-01 sprint contract §12" — a HIGH_RISK WI must have its own delta beyond the inherited baseline.
- **Quality streamlined acceptably**: WI-S01-007 (CI) is appropriately shorter on customer-impact narrative because it's internal-team-facing; the §1 "Intent" YAML is the right form for a CI WI. No degradation.

Net assessment: the new WIs are 5-15% below the WI-001 baseline on average, with WI-S01-005 the worst offender. The pattern is "compressed for batch authoring", not "missing structural discipline" — fixable in one revision pass.

---

## Recommendations

### P0 (block S-01 seal)

1. **WI-S01-005**: Specify the R2↔D1 reconciliation contract explicitly. Pick: outbox pattern with `audit_outbox` table, OR R2-first with GC orphan sweep, OR (least preferred) accept transient inconsistency with named SLO. Current text is silently inconsistent.
2. **WI-S01-005**: Fix the audit-fail rollback ordering contradiction (D1 INSERT cannot be both "before audit" and "rolled back" if audit fails).
3. **WI-S01-007**: Pin TLC state-space bounds in each `.cfg` file with explicit constants AND a CI deadline. Without this, the gate flakes or silently truncates.
4. **WI-S01-004**: Replace any multi-statement `BEGIN/UPDATE/COMMIT` text with single-statement `UPDATE ... RETURNING` or `db.batch([...])` — D1 binding does not support multi-RPC transactions.
5. **Sprint contract**: Reconcile "R2 versioning" wording (sprint.md line 122 + _spec_contract.md line 108) with WI-S01-003's `If-None-Match: *` choice.

### P1 (fix during sprint window)

6. **WI-S01-002**: Resolve the BLAKE3 dual-hash anti-scope vs §28 R-WI-001 mitigation contradiction. Commit one position.
7. **WI-S01-002**: Add `black_box` requirement to constant-time benchmark DoD; rename "verify time variance" to "compare-step variance" or restrict to fixed blob sizes.
8. **WI-S01-002**: Add audit emission rate limit on poisoning storms (per-tenant per-digest dedup window).
9. **WI-S01-003**: Cite the binding constraint for 5 MiB (Worker memory budget) or replace with an ADR.
10. **WI-S01-003**: Add startup-time SSE-S3 self-check + sentinel-object region verification.
11. **WI-S01-003**: Add `If-None-Match: *` end-to-end verification in `wrangler dev` to DoD.
12. **WI-S01-004**: Add `CHECK (refcount >= 0)` to schema; fix the Y2106/2038 rationale.
13. **WI-S01-005**: Add concurrency budget for batch processing (e.g., bounded concurrent puts).
14. **WI-S01-005**: Verify gRPC status code mapping against REAPI v2 conformance (OUT_OF_RANGE may be wrong).
15. **WI-S01-005**: Expand §26 (Security/Privacy) and §28 (Risk Register) to baseline parity.
16. **WI-S01-006**: Replace `prop_path_collision_resistant` with a Hamming-distance-distribution test; the 100k-pair injectivity is statistically trivial.
17. **WI-S01-006**: Add intermediate-snapshot assertions to `prop_refcount_race_safe`.
18. **WI-S01-007**: Pin the `tla2tools.jar` SHA + verify before exec.
19. **WI-S01-007**: Add fork-PR secret-protection guard (`if: github.event.pull_request.head.repo.full_name == github.repository`).
20. **WI-S01-007**: Document cache strategy (sccache / cargo cache) to make ≤20 min PR budget realistic.
21. **WI-S01-007**: Resolve cosign Fulcio outage mitigation (drop the "24h grace" wording or implement a real break-glass).
22. **Cross-WI**: Standardize crate path naming `crates/corelink-tenant-path/` everywhere.
23. **Cross-WI**: Document audit-emission division of responsibility between WI-002 (poisoning attempts) and WI-005 (success events).

### P2 (next sprint)

24. **WI-S01-007**: Migrate to SLSA L3 generator action (planned in WI-S12-001; reference now).
25. **WI-S01-006**: Add invariant-coverage matrix table (invariant ID → property test name).
26. **WI-S01-005**: Plan REAPI scope-set evolution (cache:w → cache:w:cas etc.) to avoid forced refactor in S-03.
27. **WI-S01-002**: Plan client-side verify (CTRL-CAS-002) interface contract now even though WI ships in S-02.
28. **All**: Tabulate cross-region D1 schema sync gate concretely (script + CI integration; promote `check_d1_schema.py` from "planned" to a sub-task).
29. **All**: PII redaction policy for audit events (no body bytes ever; only digest + tenant + request_id).

---

## Final Verdict

**GO-WITH-FIXES.**

The six new WIs are structurally complete, internally coherent on the high-level invariant story, and faithful enough to the WI-S01-001 baseline that they could be implemented in the sprint window. Three concerns prevent a clean GO: (1) WI-S01-005's silent R2↔D1 reconciliation gap is a known production armadilha already flagged in canonical sources and must be specified before code starts; (2) WI-S01-004's implicit assumption of multi-statement D1 transactions is a footgun that will surface as flaky tests under load; (3) WI-S01-007's TLC gate is non-functional without state-space bounds. None of these are rejection-worthy — they are all sprint-window fixes — but the sprint cannot be sealed for implementation hand-off until at minimum the five P0 items above are resolved and the sprint contract `_spec_contract.md` is reconciled with WI-003's actual immutability mechanism. With those fixes in place, this is a credible foundation sprint and the cross-WI consistency is good enough that S-02 + S-04 will inherit a solid base.

Score summary: WI-002 (8/10), WI-003 (7/10), WI-004 (7/10), WI-005 (6/10), WI-006 (8/10), WI-007 (7/10). **Average 7.2/10 — GOOD, not yet SOTA.**
