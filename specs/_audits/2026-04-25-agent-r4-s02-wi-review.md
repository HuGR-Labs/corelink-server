---
id: "AUDIT-2026-04-25-AGENT-R4-S02"
type: "audit"
doc_status: "DRAFT"
audit_status: "CLOSED"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-05-27"
reviewer: "Agent R4 (Claude Opus 4.7, 1M context, independent reviewer)"
scope: "Lote 10.2 — Sprint S-02 NEW Work Items (WI-S02-002 .. WI-S02-006)"
baseline_template: "WI-S02-001 (não revisado; usado como referência)"
---

> **CLOSED 2026-05-27** — S-02 sprint implementation sealed via git tag `s02-impl-sealed`; this independent review record is delivered. See `specs/_audits/2026-05-27-audit-triage-post-w36.md` for triage methodology.

# Agent R4 — Lote 10.2 S-02 WI Review (independent reviewer)

> Reviewer: independent SOTA reviewer; sem viés ao autor.
> Escopo: 5 NEW WIs (002..006). WI-S02-001 usado como reference baseline.
> Tom: crítico, técnico, sem "diplomacia". Spec final → código → produção.

---

## Veredito Geral

**GO-WITH-FIXES.** Os 5 WIs mantêm fidelidade ≥ 80% ao template HIGH_RISK estabelecido em WI-S02-001 e, em alguns aspectos, **avançam o estado da arte do framework** (WI-S02-004 introduz Mann-Whitney U como evidence-grade adversarial test; WI-S02-006 codifica RB-FM-253 dry-run automatizado). Porém **degradação sistemática** em quatro dimensões versus o baseline foundation (001): (a) **seções 22–28 atrofiadas** (Cost Analysis em 1 linha; STRIDE/LINDDUN reduzido a 2 bullets; Knowledge Transfer 1 linha; Sub-tasks ainda completos mas Risk Register com colunas abreviadas R/P/D/I/E sem legenda); (b) **inconsistência de sign-off count** (WI-002..005 dizem "11 roles" enquanto sprint contract §14 diz "13 total" — inclusive WI-006 confirma 13); (c) **lacunas técnicas reais** (negative cache concurrency race entre `put_miss` e `invalidate_on_write` não tem TLA+/property test claim; constant-time middleware não trata "Mann-Whitney p > 0.05 sustained sob load mixed" — single sample não generaliza; FindMissingBlobs cross-tenant masking afirma "indistinguishable" mas o WI não cobre o caso onde a soma response-size + timing leak é distinguível); (d) **baseline produção gaps** que o template de 32 seções **deveria** ter forçado e não forçou — WebSocket/HTTP-2 server push para streaming, circuit breaker para D1/KV, cold start budget, fallback degradation modes. Nenhum dos 5 é REJECT — todos têm intent claro, escopo limpo, Gherkin razoável, e dependências corretas. Nenhum dos 5 é SOTA puro 9-10 sem polish. **Não selem S-02 antes dos P0 fixes** (sign-off count inconsistency, ADR-0023 path com placeholder, MissReason→HTTP 410 confusion, prop_negative_cache race spec).

---

## Per-WI Findings

### WI-S02-002 (GetBlob + FindMissingBlobs)
**Score: 7.5/10** — GOOD, sólido, 4 gaps materiais.

**Strengths:**
- Narrative articula bem o "batch enumeration vulnerability" como ataque concreto (PAT B + 1000 random digests → existence oracle). O framing em §2 é acima de média.
- Decision §9.3 ("cross-tenant masked as missing, not 403") é o trade-off correto e é declarado explicitamente — alinha com CTRL-ISO-005.
- Parallel AuthZ via `join_all` com bounded concurrency 100 é tecnicamente defensável e quantificado (1000×5ms sequential vs 50ms parallel).
- Gherkin tem 9 scenarios cobrindo happy path + cross-tenant + batch limit + REAPI conformance — coverage adequada.
- §10.2.2 "10k batch enumeration → 0 cross-tenant existence leak" é métrica-driven com método statistical implícito.

**Gaps:**

1. **(P0) Negative cache integration ordering ambígua.** §6.1 afirma que FindMissingBlobs "Per-digest D1 AuthZ check"; mas WI-S02-005 §6.1 afirma que FindMissingBlobs faz "Batch lookup: parallel `negative_cache.lookup` for all digests; cache misses fall-through to D1." **Contradição direta**: WI-002 não menciona consultar negative cache antes de D1. Deve haver uma sentence em §6.1 declarando "If WI-S02-005 SEALED, cache lookup precede D1 AuthZ". Hoje como escrito, dependendo da ordem de SEAL, comportamento muda.

2. **(P1) Tenant path re-derivation por request não está explícito.** Para cada digest no batch, o handler precisa derivar a chave R2 path. Como evita deriving 1000 vezes? Cache em-process da derivation? Ou batched? §9.1 fala de parallel AuthZ mas não de path derivation. Em pior caso 1000× HMAC compute = ~10ms extra (com `corelink-tenant-path` é <10us per call, OK), mas deveria ter scenario Gherkin: "Path derivation reused via TenantPrefix cached per-request".

3. **(P1) Memory bound em GetBlob 4 MiB inline NÃO consider gRPC max-message frame.** §14.2.9 diz "memory bounded ≤ 8 MiB peak" mas REAPI v2 + tonic default `max_decoding_message_size = 4 MiB` — handler pode receber rejection antes de hit handler. Decision §9.4 menciona "gRPC max message size default 4 MiB; aligning avoids fragmentation" mas não documenta config explícito (`tonic::transport::Server::builder().max_decoding_message_size(...)`). Dev integrator vai descobrir em runtime.

4. **(P1) FindMissingBlobs response size leak não tratado.** Cross-tenant digest masking via "missing" funciona em status, mas response_body size cresce linearly com response.missing.len(). Atacante pode infer existence indiretamente: "se eu enviar 1000 digests todos cross-tenant, response.missing tem 1000 elementos. Se eu enviar 1000 digests onde 1 existe, response.missing tem 999 elementos. Diff = 1 byte JSON wrapping" — observable timing+size oracle. WI-S02-004 trata timing apenas; **size padding não é mencionado em nenhum WI**. Adicionar scenario: "Response payload size MUST be padded ou response.missing MUST always retornar all-input-digests-with-status (não filtered)".

5. **(P2) "REAPI v2 conformance" sem versão fixada.** §10.2.1 cita `bazelbuild/remote-apis` mas não fixa commit hash ou release tag. Spec drift em upstream pode quebrar conformance arbitrariamente. Adicionar pinning explicit (e.g., `bazelbuild/remote-apis @ v2.13.0`).

6. **(P2) Sign-off count inconsistente.** §30 diz "11 roles incl. Architect + Security + AppSec" mas sprint.md §14 diz **13 total** (todos os 11 + Crypto SME + AppSec). WI-002 não toca crypto — argumento "Crypto SME inclui parallel AuthZ statistical review" é fraco — então 12 makes sense, mas alinhamento com sprint contract precisa explicit (1 line saying "12 sign-offs; Crypto SME advisory dispensável aqui justification: not crypto-touching").

---

### WI-S02-003 (corelink-client-verify)
**Score: 8/10** — GOOD-toward-SOTA, melhor dos 5 NEW. Apenas 3 gaps.

**Strengths:**
- Crate **stand-alone** decision (§9.2) é correta — single Rust truth + 3 FFI = zero drift; rationale explícito.
- Default-on enforcement (§9.1) é formal correctness argument: opt-in security = optional security. CTRL-CAS-002 alignment direto.
- Stream-aware verify (§9.3) com fail-fast incremental hash + 1 MiB chunk + 2 MiB peak é well-budgeted para FFI consumers (Python pyO3, JS WASM têm budget tight).
- §9.4 (subtle crate justification) e §9.5 (Tokio feature-gated) demonstram engineering taste — não over-engineering.
- Property test suite em §6.1.6 inclui `prop_verify_constant_time` 1000 partial-match samples — endereça side-channel attack class no client side.
- §28 R-001 "Default-off slipped via dep upgrade" é um risco real e pegado.

**Gaps:**

1. **(P1) "Re-verify client-side é genuíno (sem TOCTOU)?" — não fully resolved.** Cliente faz: download body B → compute BLAKE3(B) → compare com expected D. Mas onde D vem? Do mesmo servidor que entregou B. Atacante MitM substitui B *e* D — verify passa mas cliente foi enganado. Defense correta: D vem de **out-of-band trusted source** (manifest assinado, CI lockfile, etc.). Spec não distingue "verify-as-bit-rot-detection" vs "verify-as-MitM-defense". §2 menciona MitM como ameaça em narrative, mas o attestation chain de onde vem `expected: &Digest` é assumido trusted sem documentation. Adicionar §9.X "Threat model: verify defends against bit rot + server-side post-write corruption; for full MitM defense, expected_digest MUST come from signed manifest (out-of-scope; client app responsibility)".

2. **(P1) cdylib + cbindgen é frágil para JS WASM target.** Cargo.toml em §6.1.1 menciona "Build target `cdylib` para C ABI". JS WASM consume via `wasm-bindgen` que é não-C-ABI; gera glue diferente. cbindgen export útil para Python (cffi) e Go (cgo) **mas não para JS**. WI-S15-004 (downstream) vai descobrir isso. Adicionar nota: "JS WASM consumes via wasm-bindgen; cbindgen header serves Python/Go only".

3. **(P1) 95% coverage threshold (§14.3.3) sem exception list.** Async stream code paths são notoriamente hard to cover (cancelled drop futures, mid-poll error). 95% provável significa "spend 4h hunting last 5%" — sem ROI. Define exception list: "coverage gate is 90% lib code + 95% pure verify path; async cancellation paths exempted with `#[coverage(off)]` ou allow-list".

4. **(P2) Property test for `prop_verify_constant_time` afirma "variance < 5%" mas não específica método.** §10.3.2 "criterion EVT-002" — criterion variance é diff between samples, não specific p-value. Should adopt same methodology de WI-S02-004: Mann-Whitney U entre matched-vs-mismatched digest samples; p > 0.05 OR criterion variance < 5%. Atual scheme aceita "criterion noisy day = test red" como flake.

5. **(P2) Stand-alone build assertion (§10.3.4) não é CI-enforced.** Decisão de "no Worker deps" é easy to slip via accidental import. Spec deve add CI step `cargo build -p corelink-client-verify --no-default-features` em isolated workspace OR explicit `[workspace.dependencies]` not `path = "../corelink-worker"`.

---

### WI-S02-004 (constant-time middleware)
**Score: 7/10** — GOOD-but-fragile. Mann-Whitney é ambicioso, mas execution lacks rigor.

**Strengths:**
- ADR-0023 explicitamente flagged em §9.7 — único WI dos 5 que cria nova ADR; já whitelisted em `validate_references.py` line 167 (verifiquei). Boa hygiene.
- Mann-Whitney U vs t-test rationale (§9.2) é estatisticamente sound — long-tailed latency distributions break normality assumption. Industry-correct choice.
- Seeded jitter via `request_id` (§9.4) endereça correlation attack que naive jitter sofre.
- §15 chaos experiments incluem "Mann-Whitney run em production sample" sustained 7d — moves beyond unit test.

**Gaps:**

1. **(P0) "p > 0.05 indistinguishable" é mathematically backwards-as-stated.** §1, §2, §10.4.1 todos dizem "assert p-value > 0.05 (indistinguishable)". Tecnicamente: p > 0.05 = "fail to reject null hypothesis" = "data não evidence against null". **Não prova distributions são same**. Pode meaning "small sample size, no power"). Defense correta: power analysis. Spec deveria add: "(a) compute test power 1-β ≥ 0.8 with effect size d=0.2 (small); (b) target sample N ≥ 10000 per arm; (c) report confidence interval on median diff, target |Δmedian| < 1ms with 95% CI". Mann-Whitney U sem power analysis é cargo-cult.

2. **(P0) Padding via `tokio::time::sleep_until` é defensável vs alternatives mas não anti-correlated com Worker pre-emption.** Worker scheduler em CF Workers tem sub-ms quanta; `sleep_until(start + target)` em uma boundary onde target já passou (handler took 250ms, target 200ms) returns Ready imediatamente — leaks "took longer than target" timing diff. §9.5 menciona static target mas não trata "elapsed > target" case. Spec precisa: "If elapsed > target_p99_ms, log SEV-3 anomaly + emit response sem additional padding (não shorten); rely on rate-limit + alert para detection".

3. **(P1) "Defensável vs alternativas (constant-time crypto only, response padding)" — comparação superficial.** §9.1 says "Padding 200 OK adds latency tax sem security benefit". Counter-argument não tratado: response *body size* leaks information (see WI-002 gap #4 above). Constant-time middleware sem response size padding = partial defense. ADR-0023 deve declarar this trade-off + reference WI-002 size leak gap.

4. **(P1) Adversarial test "10k×10k samples Mann-Whitney" sem config para CI flake.** §10.4.1 statistical test em CI is inherently flaky — even with p-threshold 0.05, 1-in-20 runs falham. Spec não tem retry policy ou Bonferroni correction para multiple-test. Should add: "Test runs N=3 trials; reject only se ALL 3 trials p < 0.05 (Šidák correction)". Caso contrário CI red é normal noise.

5. **(P1) Métrica `corelink.cas.side_channel.timing_diff_ms` (§6.1.5) computation method não está spec'd.** "Continuously from sliding window métricas" é hand-wave. Window size? Aggregation function (median, p99 diff)? Sample weighting? Without spec, dashboard implementation will diverge from test method.

6. **(P2) "Adaptive ML padding (overkill at GA)" (§9.5) é fine, mas spec não cobre "static target_p99_ms inadequate sob carga sustained" — i.e., target = 200ms é fine quando handler 50ms; mas se cold cache spike pushes p99 to 250ms organicamente, padding to 200ms = no-op. Should add adaptive monitoring: alert se target_p99_ms < observed_p99_handler + 30ms — operator must increase target.

7. **(P2) Statistician advisor (§29 review checkpoint) é "TBD". Recommend designating Crypto SME OR external consultant; "Statistician advisor" sem name = fictional gate. (Same critique applies entire S-02 staffing-blocked status.)

---

### WI-S02-005 (negative cache KV)
**Score: 7.5/10** — GOOD com 1 sleeper risk e 4 gaps.

**Strengths:**
- §2 narrative explicita 5 catastrophic bugs (stale negative, cross-tenant, wrong reason, TTL too long/short) — concrete failure modes, não generic.
- §9.4 explicit invalidation em write path (não TTL only) é correct call. Distinção bem articulada: TTL-only = 5min stale window; explicit = ≤5s.
- Per-tenant key construction (§6.1, §9.2) é correctly defense-by-construction — bug em key lookup sintaticamente impossível para cross-tenant leak.
- §6.1.5 FindMissingBlobs batch integration (cache lookup parallel + populate post-batch) é well-thought.

**Gaps:**

1. **(P0) Race condition `put_miss` vs `invalidate_on_write` NÃO tem TLA+/property test claim.** §2 §10.5.4 fala de "100% successful writes trigger invalidate" mas o **race window** é: T+0 cliente A probes digest_X → handler returns 404 + populates cache.put_miss; T+1ms cliente B writes digest_X → invalidate_on_write fires ANTES de put_miss completes (concurrent KV ops). KV ordering em CF é eventual. Resultado: post-invalidate, put_miss arrives → stale negative wins. WI-006 §6.1.4 lista `prop_negative_cache_correctness 10k iter race conditions` mas WI-005 não declara o specific race + mitigation (e.g., "put_miss must include monotonic version stamp via DO; invalidate em wrong order is no-op").

2. **(P1) MissReason::Tombstoned → HTTP 410 conflict com WI-S02-001.** §6.1.2 e §8 (Gherkin Tombstoned) declaram "Tombstoned should be 410 Gone, not 404"; **mas** WI-S02-001 §6.1.6 + §8 (Tombstoned scenario) declaram "404 + COR_CAS_BLOB_NOT_FOUND". Spec contradicts. WI-005 §8 last scenario says "Tombstoned distinction reserved for future S-06 GC" — mas isso é cop-out: ou enum tem 3 variants e handler routes them, ou enum tem só 1 effectively. Decision deve ser explicit em ADR ou inline: "MissReason enum existe para future; at GA todos map to 404; spec freeze HTTP semantic at 404".

3. **(P1) Probe storm cost reduction "≥ 80% measured" (§10.5.1) — methodology fragile.** "Cost reduction" é função de probe distribution: random digests (worst case for cache; first iteration always cold) vs realistic Bazel batch (high repeated digest hit ratio). 80% claim depende de assumed access pattern. Spec deve fix benchmark scenario: "1000 unique non-existent digests probed twice; second iteration p99 ≤ 50ms; cost(second) / cost(first) ≤ 20%".

4. **(P1) KV strong consistency CF claim (§8 last Gherkin) é otimista.** CF KV docs: "Eventually consistent globally; ≤ 60s para read-after-write em outras regiões". §8 says "p99 stale window ≤ 5s (KV strong consistency CF)" — **incorrect**. KV é eventual consistent global. Per-region read-after-write is strongly consistent (~ms), but cross-region is up to 60s. Spec should clarify: "Stale window ≤ 5s within originating region; cross-region eventual ≤ 60s — accept para cache invariant since cache miss = fall-through". Then chaos experiment §15.4 ("TTL boundary") becomes more meaningful.

5. **(P2) Per-region binding wrangler.toml (§6.1.6) "KV_NEGATIVE_CACHE_<REGION>" — REGION enum não está spec'd.** Sprint regions são "wnam, enam, weur, eeur, apac" (likely). Binding names need explicit list to prevent typo bugs em wrangler. Add table: REGION ∈ {wnam, enam, weur, eeur, apac}; binding name strict template.

6. **(P2) "Cache pollution" (§2) handled via "KV LRU eviction at scale" + "per-tenant scope" — mas KV não tem LRU eviction; KV é unbounded billable storage até user delete. Pollution em scale = cost storm para tenant atacante (cobertura via S-08 quota). Spec misrepresents KV semantics; should say: "atacante's tenant absorbs storage cost; rate-limit + quota S-08 cap exposure".

---

### WI-S02-006 (property tests RB+PRR)
**Score: 8/10** — GOOD ship-gate WI; 2 sleeper risks.

**Strengths:**
- Concrete deliverables (§6.1): 4 property tests + bit-rot integration test + RB-FM-253 dry-run script + PRR doc + adversarial review. Lista é executável.
- Bit rot direct R2 injection (§9.2) > mock — endereça FM-051 honestamente. Pentest-grade approach.
- §9.3 RB-FM-253 automated script (não manual) é correct — drift detection é primary value.
- §10.6.1 "100k iter green sustained 7d nightly" — exige durability evidence, não single CI green.
- §6.1.5 adversarial review é internal pentest scope (not external; defer S-20). Reasonable.

**Gaps:**

1. **(P0) "Round-robin" entre tenants no proptest é GENUINELY útil ou cargo-cult?** WI title é "Property Tests Round-Robin PRR". Spec body **nunca menciona round-robin explicitly**. §6.1.1 lista 4 props mas nenhum tem "round-robin" semantic — `prop_tenant_isolation_read_path` é "50 tenants × 1000 random read ops" (random sampling, not round-robin). Reviewer suspicion: "round-robin" é spec-as-marketing, não spec-as-engineering. Either (a) remove "Round-Robin" do title and sprint contract, OR (b) add explicit prop strategy "shrinker rotates through tenant set deterministically; covers all (tenant_i, tenant_j) cross pairs at least once em N iter". Without rigor, property test = 50 tenants × 1000 random = miss subtle (i,j) interactions.

2. **(P0) PRR sign-off list completeness vs sprint contract.** §6.1.4 "Sign-off matrix (13 roles)" — but lists só "Owner + Final Approver + 11 roles" + Crypto SME = 13. Sprint contract §14 lists 13 (Owner + Final + 11 numbered roles incl. AppSec at #11 + Crypto SME at #13 = 13). WI-006 §1 says "13 sign-offs" + §9.5 "Treating as mandatory ensures scrutiny". Self-consistent. But §28 R-005 "PRR sign-off staffing missing (Tier 1 reviewers)" + sprint.md staffing-blocked = real risk. Need waiver path explicit ("if Tier 1 missing, defer SEAL or accept residual?"). Today: ambiguous.

3. **(P1) Bit rot 10 scenarios (§6.1.2) listed without enumeration.** "(corrupt single byte, corrupt range, corrupt at end, swap with another blob, truncate, etc.)" — 5 listed + "etc." = 5 unspecified. Adversarial pentester would itemize. Add table: 10 scenarios numbered with byte ranges + expected error code.

4. **(P1) "100k iter ≤ 90s em CI" (§10.6.1) is tight on GitHub Actions standard runners.** 100k property test iterations × per-iter setup (D1 + R2 + KV mock) = realistic 200-300s. §10.6.1 says ≤ 90s — likely require parallelism or in-memory mocks. Spec doesn't say which. If switch to in-memory mock = property test value reduces (real D1 quirks not exercised). Trade-off undocumented.

5. **(P2) "RB-FM-253 dry-run script" (§6.1.3) creates `scripts/rb_fm_253_dry_run.sh` — bash script automation conflict with project policy on "no shell scripts > 30 lines without justification" (if such policy exists). Should consider Rust binary (`cargo run --bin rb-fm-253-dry-run`) for type safety + reuse of test infrastructure.

6. **(P2) Staging environment availability (§18 hard blocker) é unclaimed — does staging env exist? S-01 deployed there? If S-02 is first sprint to need staging, infra cost not budgeted in §22 ("CI cost: ~$0").

7. **(P2) §13 Adversarial review report path: `specs/_audits/2026-XX-XX-pentest-s02.md` — placeholder date. Should be `2026-05-XX` (after S-02 implementation timeline) OR resolved before SEAL.

---

## Cross-WI Consistency

**Strengths:**

- Dependencies are coherent: 002 ⟂ 001 (both reuse TenantCtx + AuthZ helper from 001; declared in 002 §18). 003 depends only on S-01 WI-S01-002 (corelink-hash) — correct, since client-verify is stand-alone. 004 wraps 001+002 handlers — correct. 005 hooks into 001 (read) + S-01 WI-S01-005 (write) + 002 (batch) — correct. 006 ⟂ 001..005 SEALED — correct ship gate.
- Reuse of crates is consistent: `corelink-tenant-path` (S-01) cited in 001 §18 + 005 cache key uses `tenant_id_hex`. `corelink-hash` (S-01 WI-S01-002) cited in 003 §18.
- ADR-0023 is correctly placed in WI-S02-004 §9.7 + §10.4.5 + §13 + §29.4 + sprint.md S02-D4. Whitelisted in `scripts/validate_references.py` line 167.

**Inconsistencies (P0/P1):**

1. **(P0) Sign-off count drift.** Sprint S-02 §14 = 13 total. WI-S02-001 §30 sign-off table has 14 rows (Owner + Final + 12 roles incl. 2 peer reviewers + Crypto SME). WI-S02-002..005 §30 say "11 roles" / "HIGH_RISK 10-12". WI-S02-006 §30 says "13 roles". **Recommended: standardize all 5 NEW WIs to "13 sign-offs per sprint.md §14"; one-line justification if reduced for non-crypto-touching WI (002, 005).**

2. **(P0) MissReason::Tombstoned → HTTP semantic conflict.** WI-005 §8 last scenario says Tombstoned should be 410 Gone, but WI-001 §6.1.6 says soft-delete returns 404. Two specs of same behavior. Decision needed: stay 404 at GA; defer 410 to S-06 GC sprint with explicit migration ADR.

3. **(P1) Negative cache integration ordering em FindMissingBlobs.** WI-002 §6.1 doesn't mention cache lookup; WI-005 §6.1.5 says it does. Add 1-line cross-ref in 002 §18: "If WI-005 SEALED, batch handler delegates first to negative_cache.lookup_batch()".

4. **(P1) Audit emission cross-tenant attempt — emitted in 001 at 403; in 002 at 404 mask.** WI-002 §8 cross-tenant scenario says "audit event emitted; 404 not 403" — note that audit emission om path masked-as-404 is necessary forensic but operator alert shouldn't trigger end-user-detected 404 (they could observe distinct error_code). Confirm error_code para internal audit vs. response body remain consistent.

5. **(P1) `corelink-tenant-path` crate path mismatch.** S-01 WI-S01-001 §17 declares crate at `crates/tenant-path/` (no `corelink-` prefix). S-02 WI-S02-001 §13 references `crates/corelink-worker/...` — but doesn't reference path crate. Just a S-01 inconsistency carried forward; not specific to NEW WIs but worth flagging para next lote.

---

## Technical Accuracy Issues

1. **WI-002 GetBlob/FindMissingBlobs:** R2 read with Range — correct that range support is REAPI v2 ByteStream-only (WI-001), not GetBlob unary. Tenant_path re-derivation per request: uses single TenantPrefix derived from PAT once + reused for batch — implicit, but should be explicit (gap above).
2. **WI-002 FindMissingBlobs:** Negative cache hit consulted **before** R2 — correct in WI-005 spec but missing in WI-002 itself. Cross-WI gap.
3. **WI-003 client-verify:** no_std-friendly — spec doesn't mention `no_std`; only that crate is Tokio-feature-gated. JS WASM via wasm-bindgen actually requires `std` (allocator). Claim "no_std-friendly" per task brief is overstated; spec should clarify "minimal std footprint, not no_std".
4. **WI-003 BLAKE3 incremental:** correct usage; BLAKE3 supports `update(chunk)` natively. No accuracy issue.
5. **WI-003 client-side re-verify TOCTOU:** real concern only if expected_digest comes from same channel as body. For Bazel, expected_digest comes from action result (ActionCache lookup, signed by server) — out-of-band. WI doesn't discuss attestation chain. Genuine gap.
6. **WI-004 timing padding via middleware:** defensible vs constant-time crypto only (which Argon2id from S-03 covers, not CAS handlers) and response padding (more invasive, not covered). Mann-Whitney U effectiveness measure is correct in principle; spec lacks power analysis (gap above). One subtle technical issue: `tokio::time::sleep_until(start + target)` resolution on CF Workers Wasm is **~1ms** (not ns); padding granularity should be ≥ 5ms to dominate jitter — spec doesn't constrain this.
7. **WI-005 negative cache poison cross-tenant:** prevented by-construction via per-tenant key. Verified. Spec correct.
8. **WI-005 KV strong consistency claim:** as noted gap #4 — incorrect; KV is eventually consistent globally.
9. **WI-006 100k property test runtime:** see gap #4 — likely under-budgeted.

---

## Missing Gaps for Production

These are **not in any of the 5 NEW WIs** but should be in S-02 ship gate (or explicit deferral with WI ID):

1. **WebSocket / HTTP-2 server push for streaming.** ByteStream::Read em WI-001 uses gRPC streaming (HTTP/2 inherent). HTTP `GET /v1/cas/<digest>` uses chunked transfer-encoding. No mention of HTTP/2 server push, no CF compatibility statement, no Worker WebSocket fallback. **For 1 GiB blob serving over potentially flaky connection, no spec for resumable downloads beyond REAPI `read_offset`.** Should add to WI-001 anti-scope or explicit defer.

2. **Backpressure on streaming.** WI-001 mentions chunk 1 MiB + Worker memory ≤ 50 MiB peak. But what if client is slow consumer? R2 stream reader keeps buffering. Backpressure mechanism (slow R2 reads when client buffer full) NOT specified. Realistic Worker OOM scenario.

3. **TLS pinning client-side.** WI-003 verify defends bit rot + post-write corruption — but WI explicitly notes MitM as threat in §2 narrative. Without TLS pinning at SDK side (WI-S15-004 forward), MitM with rogue cert is possible. Should add forward-looking note in WI-003.

4. **Fallback if KV down.** WI-005 §15.1 chaos experiment "KV outage: graceful fall-through to D1 + R2". Good. But what about **D1 down**? WI-001 AuthZ check requires D1; if D1 down, all reads 503. No spec for read-only fallback (e.g., serve from negative cache + R2 ACL — but ACL not implemented in M0). This is realistic Cloudflare incident scenario.

5. **Circuit breaker patterns.** WI-005 §15 mentions `PAT-CIRCUIT-BREAKER-001 forward-looking`. **None of WI-001..005 implement circuit breaker.** Repeated D1 timeouts → cascade to all reads. Add to S-02 scope OR explicit defer (S-08?).

6. **Cold start budget.** Cloudflare Workers cold start is 5-50ms. SLO "p99 cold ≤ 300ms" — does this include cold start? Spec doesn't say. WI-001 §10.1.4 says "D1 AuthZ overhead ≤ 5ms p99" — measured cold or warm? Test scaffolding spec missing.

7. **REAPI v2 conformance test runner.** WI-002 §10.2.1 "REAPI conformance 100% pass" but **bazelbuild/remote-apis test suite does not natively include conformance test framework**. There's bazelbuild/remote-apis-tools, but coverage is partial. Spec assumes a maturity that doesn't exist in upstream; WI-002 §17.8 (3h to integrate) is under-budgeted.

8. **Quota enforcement at read time.** S-02 doesn't gate reads on quota (S-08 forward). But abuse storm of 1M reads × 1 KB × 1 PAT = realistic in S-02 staging. WI-005 says "rate-limit S-08 forward" — but S-02 should at minimum log + alert on per-PAT read rate spike. Not specced.

9. **Client connection pooling guidance.** SDK side decision: persistent connection vs per-request. For 1000-digest FindMissingBlobs batch followed by 100 PutBlobs, connection reuse matters. WI-003 client-verify is silent on this. Forward to S-15 OK, but should mention.

10. **Observability: distributed tracing across WI-001..005 paths.** Each handler emits own métricas. **No trace_id propagation spec for end-to-end Bazel build → CAS read → R2 → response.** S-09 is forward dependency; WI-001 §21 says "8 métricas" + "1 dashboard" but no `trace_id` field in audit emission spec.

---

## Comparison vs WI-S02-001 Template

WI-S02-001 (baseline) is **denser, more rigorous in sections 22–32** vs the 5 NEW WIs:

| Section | WI-001 (baseline) | WI-002..005 avg | WI-006 |
|---|---|---|---|
| §1 Intent | ~30 lines + code block | ~20 lines + code | ~25 lines |
| §2 Narrative | 38 lines, 5 paragraphs | 30 lines avg, 4 paragraphs | 35 lines |
| §22 Cost Analysis | 7 bullet points + TCO 12m breakdown | 1-2 lines (e.g., 002 = 2 lines) | 2 lines |
| §26 Security & Privacy | STRIDE 6 dimensions + LINDDUN 7 dimensions | STRIDE+LINDDUN 4-6 bullets total (e.g., 005 = 2 bullets) | 1 line |
| §27 Knowledge Transfer | Tech talk + onboarding test + doc location | 1 line tech talk title | 1 line |
| §28 Risk Register | 7 rows, full 6-col (ID/R/P/D/I/E/Res/Mitigation) | 6 rows, abbreviated headers (R/P/D/I/E/Res) without legend | 5 rows |
| §30 Sign-off | Full 14-row table with Name + Date + Signed Y/N | 1-line "11 roles incl. ..." (NO matrix) | 1 line |
| Anti-patterns §32 | 8 enumerated | 6-7 enumerated | 7 |

**Pattern**: foundation WI gold-plates, downstream WIs short-circuit. This is **acceptable** for non-foundation WIs IF sprint-level §14 captures the table once. **However**, the 1-line replacement is too terse: "11 roles incl. Architect" is non-actionable. Recommendation: each WI §30 should have **a 13-row table with checkboxes** (even if unsigned at WI creation), not free-text "11 roles incl.".

**Quality degradation grade**: B+ → C+ from foundation to NEW WIs. Acceptable but should not degrade further in next sprints.

---

## Recommendations

### P0 — MUST fix before S-02 SEAL

1. **Sign-off count harmonize:** WI-002, 003, 004, 005 §30 → standardize to "13 sign-offs per sprint.md §14" OR explicit reduced count with justification. Add 13-row table (matching WI-001 pattern) even if unsigned.
2. **MissReason→HTTP code decision:** ADR ou inline freeze: at GA, MissReason::Tombstoned maps to 404 (not 410). WI-005 Gherkin scenario fix; remove "future S-06" cop-out.
3. **Mann-Whitney power analysis:** WI-004 §10.4.1 add "test power 1-β ≥ 0.8 with effect d=0.2; sample N ≥ 10000 per arm; |Δmedian| ≤ 1ms with 95% CI". Without this, p > 0.05 claim is statistically uninformative.
4. **Negative cache concurrency property:** WI-005 add specific scenario: "concurrent put_miss vs invalidate_on_write ordering; invalidate uses monotonic version stamp" + WI-006 §6.1.1 prop_negative_cache_correctness exercise this race explicitly.
5. **Round-robin spec rigor:** WI-006 title says "Round-Robin PRR" but body doesn't specify. Either remove title qualifier OR add explicit round-robin shrinker strategy.
6. **ADR-0023 path placeholder fix:** WI-004 §13 lists `ADR-0023-...` (literal ellipsis). Replace with concrete file name `ADR-0023-constant-time-timing-padding.md`.

### P1 — SHOULD fix before SEAL (or queue P1 fix)

7. **Cross-WI integration ordering:** WI-002 §6.1 + §18 reference cache lookup before D1 (matching WI-005 §6.1.5).
8. **FindMissingBlobs response size leak:** WI-002 add scenario: pad response.missing list size OR always return full input list with status flags.
9. **TOCTOU/attestation chain:** WI-003 §9 add subsection on threat model — verify defends bit rot + server-side corruption; out-of-band manifest required for full MitM defense.
10. **Padding edge case:** WI-004 add scenario "elapsed > target_p99_ms": log SEV-3 anomaly + emit response without negative padding.
11. **KV consistency claim correction:** WI-005 §8 stale window scenario — clarify ≤ 5s within originating region; cross-region eventual ≤ 60s.
12. **Bit rot 10 scenarios enumeration:** WI-006 §6.1.2 list all 10 with byte ranges + expected error code.
13. **100k property test runtime budget:** WI-006 §10.6.1 — clarify in-memory mock vs real D1 + revise 90s budget upward if real D1.
14. **REAPI conformance pinning:** WI-002 §10.2.1 fix to `bazelbuild/remote-apis @ <commit-hash or tag>`.
15. **Coverage threshold exception list:** WI-003 §14.3.3 — async stream paths + `#[coverage(off)]` allowance.
16. **gRPC max-message-size config:** WI-002 add tonic config explicit (4 MiB) in §6.1.

### P2 — Nice-to-have / next iteration

17. CI flake mitigation for Mann-Whitney: WI-004 §10.4.1 add Šidák correction or 3-trial retry.
18. Métrica `timing_diff_ms` aggregation method spec in WI-004 §6.1.5.
19. Per-region KV binding name table in WI-005.
20. Static-target inadequacy alert in WI-004.
21. RB-FM-253 dry-run as Rust binary (not bash script).
22. Adversarial review file date placeholder in WI-006 §13.
23. Forward gaps (WebSocket, backpressure, circuit breaker, cold start, distributed tracing) — explicit "deferred to S-XX" anti-scope in S-02 sprint.md OR new WI-S02-007 catch-all.

---

## Final Verdict

**GO-WITH-FIXES.** All 5 NEW WIs are within "GOOD" range (7–8/10); none are REJECT. Average score **7.6/10**. The sprint is shippable AFTER P0 fixes (sign-off harmonization, MissReason decision, Mann-Whitney power analysis, negative cache concurrency property, round-robin spec rigor, ADR-0023 path). P1 fixes (15 items) should be batched as second pass before SEAL or accepted as known-debt with WI-S02-007 catch-all. P2 fixes can defer to S-03 or post-GA. The work demonstrates engineering taste (Mann-Whitney over t-test, default-on enforcement, parallel AuthZ with bounded concurrency, explicit cache invalidation over TTL-only) but **degraded rigor in sections 22–32 vs WI-S02-001 baseline** must not become normalized — next sprints should restore foundation-level density. Spec **will be code**; gaps left here become bugs in production: the highest-bug-yield gap is **#1 Mann-Whitney p > 0.05 without power analysis** (statistical methodology not actionable as test pass criterion) and **#4 negative cache race condition** (real concurrent KV ordering bug). Fix those first.

---

**Reviewer Confidence**: 0.85. Reviewed 6 WIs (1 baseline + 5 new), sprint contract, validate_references.py, ADR registry, S-01 cross-references, baseline canonical IDs (FM-051, FM-253, RB-FM-253, PAT-KV-TTL-001). Did not deep-read TLA+ specs nor failure_modes.md; observations on those modules are inferred from WIs. Statistical methodology critiques (Mann-Whitney power) are textbook-grade.
