---
id: "WI-S01-001"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-002", "FF-HR-005"]
parent: "S-01"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "AUTH-MODEL"
  - "KEY-MANAGEMENT"
  - "INVARIANT-REGISTRY"
  - "DATA-MODEL"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
tags: ["wi", "s01", "tenant-isolation", "hmac", "crypto", "foundation"]
---

# WI-S01-001 — Lib `tenant_path`: HMAC tenant prefix derivation

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-01](../sprint.md) · **Assignee:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** até ≥ 2 reviewers nomeados
> **inherits_from:** SECURITY-MODEL + AUTH-MODEL + KEY-MANAGEMENT + INVARIANT-REGISTRY + DATA-MODEL + REMOTE-CACHE-PRODUCT-PROFILE

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S01-001 |
| Título | Lib `tenant_path`: HMAC tenant prefix derivation |
| Sprint | S-01 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-002 (tenant isolation), FF-HR-005 (crypto control) |
| Tier | Todos (core de multi-tenant isolation) |
| Fase produto | Fase 1 — Remote Cache |

## 1. Intent

Implementar **crate Rust interno `corelink-tenant-path`** que encapsula a única função do sistema autorizada a derivar o prefixo HMAC usado em R2/KV/DO keys para isolamento de tenant. Function signature:

```rust
pub fn derive_prefix(tdk: &TenantDerivationKey, tenant_id: Uuid) -> TenantPrefix;
```

Onde `TenantPrefix` é `[u8; 16]` (16 bytes raw; encoding textual canônico = `HMAC16 = b64(HMAC_SHA256(TDK, tenant_id_bytes))[0:16]` per `remote_cache_product_profile.md §7.1` + `storage_semantics_matrix.md §3`). Zero call sites alternativos permitidos: compile-time enforcement via `#[deny(missing_docs, unsafe_code)]` + `pub(crate)` em `derive_prefix_raw_internal`.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

O isolamento de tenant no CoreLink depende inteiramente do prefixo HMAC-SHA256 (CTRL-AUTH-004) sendo **derivado por UMA única função** em TODO o codebase. Qualquer bug — dev calcula prefix "do jeito dele" em um call site obscuro, regressão que usa `tenant_id` plaintext (como o audit finding T-01 apontou no TODO.md), ou mismatch entre Worker e Background Worker — quebra `INV-TENANT-ISOLATION` (CRITICAL).

O ataque direto é: **Tenant A autentica, deriva prefix `P_A`, mas por bug escreve em `P_B`** (path traversal via config drift, typo no test, ou erro de refactor em code review incompleto). Sistema acha que é write legítimo (HMAC válido; apenas o prefix apontado está errado). Blob de A aparece no namespace de B. Próxima leitura de B vê blob alheio. Cross-tenant exposure.

A mitigação *é architectural*: centralizar derivação num ponto único, testável, type-safe. `TenantPrefix` é um newtype que **só** pode ser construído via `derive_prefix()`. Lib expõe 1 função pública + 1 helper de encoding (`to_hmac16` — RFC 4648 §5 base64 URL-safe truncated 16 chars; alinha HMAC16 canonical). Compile-time: nenhum caller pode construir `TenantPrefix` diretamente (field private). Se dev precisa de prefix, **obrigatoriamente** passa pela função. Se chamar derivação 2x com mesmo input, retorna mesmo output (determinismo).

Isso é o padrão **type-driven security**: o sistema de tipos carrega o invariante. Bugs de layer-above ficam impossíveis por construção.

**Risk justification para HIGH_RISK:**
- **FF-HR-002**: toca `INV-TENANT-ISOLATION` diretamente. Framework §33.5.3 manda HIGH_RISK automático.
- **FF-HR-005**: implementa CTRL-AUTH-004 (HMAC tenant prefix) + CTRL-ISO-001 (HMAC tenant prefix enforcement). Controles de segurança canônicos.
- Blast radius: todos os tenants em todas as regiões.
- Reversibility: one-way-door se ship com bug — deploy rollback não recupera blobs já contaminados em outros namespaces.

Por isso exige: 11 sign-offs canonical HIGH_RISK, TLA+ check, property test 10k iter, SAST clean, adversarial review, chaos test, PRR completa.

## 3. Customer Impact & Journey

**JTBD:** "Como tenant enterprise, eu preciso garantia de que meus blobs NUNCA aparecem em namespace de outro cliente, independente de bug em código interno ou credencial roubada de colaborador."

**Journey touch-points:**
- Transparente ao cliente direto (é crate interno).
- Indiretamente: afeta **todo** write/read/list — lib errada = isolamento quebrado.
- Crítico para tier `enterprise` (contratos com DPA + BYOK).

## 4. Capability Mapping

- **CAP-CAS-002** (tenant isolation criptografa via HMAC prefix) — IMPLEMENTA.
- **CAP-CAS-001** (BLAKE3 content addressing) — dep downstream (próximo WI).

Trace canônico: `auth_model.md §8.1 (5 camadas de defesa)` → camada 5 (R2 key prefix) → esta lib.

## 5. Tipo e Classificação

- **Tipo:** Foundation (infra core)
- **Lane:** HIGH_RISK
- **Lane forcing factors:** FF-HR-002, FF-HR-005

## 6. Escopo

### 6.1 In-scope

- Crate `corelink-tenant-path` em `crates/tenant-path/`.
- Struct `TenantDerivationKey([u8; 32])` + `impl` pra load from Cloudflare Secrets binding.
- Struct `TenantPrefix([u8; 16])` — newtype with private field.
- Function `derive_prefix(tdk: &TenantDerivationKey, tenant_id: Uuid) -> TenantPrefix` (HMAC-SHA256, info="path" via HKDF).
- Impl `Display` para `TenantPrefix` com encoding **HMAC16 canonical** = `b64(HMAC_SHA256(TDK, tenant_id_bytes))[0:16]` (RFC 4648 §5 base64 URL-safe; truncate 16 chars; alinha `remote_cache_product_profile.md §7.1` + `storage_semantics_matrix.md §3`; URL-safe sem padding).
- Errors: `DeriveError` enum apenas para casos onde input é estruturalmente inválido (ex: TDK com tamanho errado — deveria ser impossível por construção, mas defense-in-depth).
- Unit tests (12 cases): edge cases de HMAC, determinism, injectivity pair-wise com dataset fixo.
- Property tests `proptest`: 10k iter — for all pairs of distinct tenant_ids, prefix outputs differ. Determinism.
- Fuzz target `cargo-fuzz`: random bytes inputs não panickam.
- Benchmark `criterion`: < 100μs per derive em CF Workers (<50 MB runtime).
- Documentation: rustdoc completo com exemplo de uso + anti-pattern section.

### 6.2 Out-of-scope

- ❌ Integração com Clerk SSO (PAT → tenant_id resolution) — WI-S01-005.
- ❌ Uso em R2 write path — WI-S01-003.
- ❌ Uso em D1 queries — WI-S01-004.
- ❌ TDK rotation logic — `key_management.md §3` define; implementação em Sprint S02+.
- ❌ BYOK integration — Sprint S05+ (Fase 2 enterprise).
- ❌ Frontend / admin UI.

## 7. Anti-Scope (expandido para HIGH_RISK)

- **Não** adicionar função de "derive prefix com override" ou "derive prefix for testing" no scope público.
- **Não** expor raw bytes de `TenantPrefix` fora do crate (todo encode passa por `Display`).
- **Não** usar `unsafe` em lugar nenhum (enforce via `#![deny(unsafe_code)]`).
- **Não** permitir comparação de `TenantDerivationKey` via `Eq`/`PartialEq` públicos (prevent accidental log leaks).
- **Não** derivar `Debug` com default — custom impl que redacta bytes para `"TenantDerivationKey(REDACTED)"`.

## 8. Acceptance Criteria (Gherkin)

### AC-1: Determinism

```gherkin
Given TDK fixa "fixture-tdk-32-bytes-constant-for-test"
  And tenant_id fixo = UUIDv7 "01938af0-abcd-7123-8456-000000000001"
When derive_prefix é chamado 1000 vezes
Then todos os 1000 outputs são idênticos
```

### AC-2: Injectivity (property test 10k pairs)

```gherkin
Given TDK fixa
  And dois tenant_ids distintos T_a e T_b
When derive_prefix(TDK, T_a) e derive_prefix(TDK, T_b) são computados
Then os outputs são distintos (pelo menos 1 byte diferente)
```

### AC-3: Cross-TDK separation

```gherkin
Given dois TDKs distintos T1 e T2
  And tenant_id fixo
When derive_prefix(T1, tid) e derive_prefix(T2, tid) são computados
Then outputs são distintos
```

### AC-4: Encoding HMAC16 canonical (b64 URL-safe)

```gherkin
Given um TenantPrefix resultado de derive_prefix
When Display é chamado
Then output tem exatamente 16 caracteres ASCII do alfabeto base64 URL-safe (RFC 4648 §5)
  And alinha contrato canonical HMAC16 = b64(HMAC_SHA256(TDK, tenant_id_bytes))[0:16] per remote_cache_product_profile.md §7.1
  And não contém padding ('=')
  And não contém caracteres reservados de URL (/, ?, #, %)
```

### AC-5: No-panic on adversarial input (fuzz)

```gherkin
Given inputs aleatórios gerados por cargo-fuzz
When derive_prefix é chamado por 1h em CI
Then zero panics
  And zero crashes
```

### AC-6: Performance

```gherkin
Given benchmark `criterion` em ambiente CF Workers-like
When derive_prefix é chamado 10k vezes
Then p99 latency < 100μs
  And memory allocation < 256 bytes per call
```

### AC-7: Secret hygiene (CTRL-CRED-001)

```gherkin
Given TenantDerivationKey instanciada
When Debug é chamado (ex: panic trace, println!, log macro)
Then output é "TenantDerivationKey(REDACTED)" exatamente
  And não expõe qualquer byte da key real
```

## 9. Design Decisions

### 9.1 HMAC-SHA256 vs HMAC-BLAKE3

**Decidido:** HMAC-SHA256 (ADR-0043 forward — ADR-0015 já alocado para reproducible-build-best-effort; ADR-0043 é HMAC tenant prefix algorithm choice canonical para S-01).

**Rationale:**
- HMAC-SHA256 é FIPS 140-2 compliant; HMAC-BLAKE3 não.
- BLAKE3 é usado para content hashing onde velocidade importa mais.
- Isolamento de tenant precisa FIPS pra compliance SOC 2 enterprise.

### 9.2 HKDF com `info="path"`

Derivação final: `TenantPrefix = HKDF_Expand(HMAC_Extract(TDK, tenant_id), info="path", L=16)`.

Separa do uso de TDK para AC signing (info="ac-sig") e envelope encryption (info="envelope"), garantindo que comprometer path-HMAC não leake outras derivações.

### 9.3 16 bytes vs 32 bytes

16 bytes truncados de HMAC-SHA256 = 128 bits entropia. Collision probability = 2^-64 para prefix guessing — aceitável para namespacing (não para auth). Trade-off: path length em R2/D1 indexing.

## 10. Completeness Criteria SOTA (HIGH_RISK = TODAS aplicáveis)

- [ ] **10.1 Código** compila sem warnings em `cargo clippy -D warnings` (EVT-001)
- [ ] **10.2 Tests** unit (12 cases) + property (10k iter) + integration (em WI-S01-003) passam (EVT-002)
- [ ] **10.3 Coverage** ≥ 95% pelos property tests + unit (EVT-003)
- [ ] **10.4 Observability** — não aplicável (crate sem I/O; métricas emitidas no call site)
- [ ] **10.5 Security**: SAST clean (EVT-005) + dependency audit (EVT-007) + no unsafe (SBOM verify)
- [ ] **10.6 Performance**: criterion benchmark p99 < 100μs (EVT-004)
- [ ] **10.7 Bench** integrado em `benches/history/` (EVT-004 tracking)
- [ ] **10.8 Fuzz**: cargo-fuzz target rodado 1h (EVT-008) — zero findings
- [ ] **10.9 Mutation** testing: cargo-mutants > 80% kill rate (EVT-009)
- [ ] **10.10 Docs**: rustdoc com ≥ 1 example; lib-level doc explicando invariante (EVT-016 knowledge transfer)
- [ ] **10.11 Reproducible build** verificado (SBOM hash match entre 2 runners)

## 11. Definition of Done

Todas as checkboxes de §10 **+**:

- [ ] TLA+ `tenant_isolation.tla` verde em CI (EVT-022) — invariante formal protegida.
- [ ] PR approved por Tech Lead + Security Reviewer + Architect (EVT-015)
- [ ] Sign-off §30 assinado por todos os 11 papéis HIGH_RISK canonical (EVT-016)
- [ ] Release tagged + SBOM CycloneDX 1.5 assinado (EVT-010 + EVT-011)
- [ ] Documentation merged + linked em `auth_model.md §8.1` como implementação de camada 5.

## 12. Invariants

### 12.1 Invariantes mantidas por este WI

| ID (invariant_registry.md) | Tipo | Enforcement local |
|---|---|---|
| INV-TENANT-ISOLATION (CRITICAL) | Global | `derive_prefix` é único ponto de derivação; property test garante injectivity |
| INV-CAS-IDEMPOTENCY (CRITICAL) | Deriva | HMAC determinístico → mesmo tenant_id → mesmo prefix sempre |
| INV-CONF-AT-REST (HIGH) | Suporta | TDK em Cloudflare Secrets HSM-backed (não em heap persist) |

### 12.2 Formal spec

- **TLA+:** `tenant_isolation.tla` (existente, v3 pós Lote 7.1) cobre este invariante. Este WI NÃO adiciona novo spec; implementa a camada concreta do que spec modela abstratamente.
- **Property test:** `tests/prop_tenant_path.rs` (a criar no WI) é a "ponte" entre TLA+ abstract e Rust concreto.

## 13. Artifacts Produced

- `crates/tenant-path/Cargo.toml`
- `crates/tenant-path/src/lib.rs` (single-file crate, ~200 LOC target)
- `crates/tenant-path/src/error.rs`
- `crates/tenant-path/tests/prop_tenant_path.rs`
- `crates/tenant-path/benches/derive.rs`
- `crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs`
- `crates/tenant-path/README.md` (rustdoc-generated + badges)
- CI workflow `.github/workflows/tenant-path.yml` (roda tests + bench + fuzz 1h nightly)

## 14. Quality Standards SOTA (HIGH_RISK = TODAS inclui 14.9)

- **14.1 Code quality**: clippy strict + no unsafe + no unwrap in lib code (Result everywhere)
- **14.2 Test quality**: property > unit; hypothesis-driven not coverage-driven
- **14.3 Docs quality**: cada pub item tem rustdoc; lib has architecture ADR linked
- **14.4 Perf quality**: criterion baseline committed; alertas regression > 10%
- **14.5 Security quality**: threat-modeled per security_model §5.1 (Spoofing); pentested mentalmente contra PathGuess (ver tenant_isolation.tla)
- **14.6 Observability quality**: N/A (lib); callers (WI-S01-003, 005) emitem
- **14.7 Operability quality**: sem runtime state; pure function
- **14.8 Evolvability quality**: API v1 congelada ao fim do WI; breaking change = bump major + WI novo
- **14.9 Sustainability** (HIGH_RISK only): memory footprint < 1 KiB steady; zero allocations em steady-state após warmup

## 15. Chaos Experiments (HIGH_RISK = obrigatório)

- **Experiment C-1**: inject TDK com bytes aleatórios em test harness. Expected: determinism holds. Actual: measure.
- **Experiment C-2**: pauperize PRNG do OS durante fuzz run. Expected: zero panics (HMAC não depende de randomness run-time).
- **Experiment C-3**: in staging após WI-S01-003 integra: rotacionar TDK mid-flight e verificar que reads com prefix velho falham corretamente (INV-KEY-NO-SKIP em `key_management.md §3.2`).

Evidence: EVT-023 (CHAOS_EXPERIMENT_REPORT).

## 16. Production Readiness Review (HIGH_RISK = obrigatório)

PRR separada — criar `PRR-S01-001-tenant-path.md` após implementação completa, seguindo `_templates/production_readiness_review.md` com lane=HIGH_RISK, 11 sign-offs canonical.

## 17. Sub-tasks

- **ST-001**: Setup crate skeleton + Cargo.toml dependencies (2h)
- **ST-002**: Implement `TenantDerivationKey` + HMAC-SHA256 wrapper (4h)
- **ST-003**: Implement `derive_prefix` + HKDF-Expand (4h)
- **ST-004**: Implement `TenantPrefix` newtype + HMAC16 Display (b64 URL-safe truncated; canonical) (3h)
- **ST-005**: Unit tests 12 cases (3h)
- **ST-006**: Property tests 10k iter (4h)
- **ST-007**: cargo-fuzz target + CI integration (3h)
- **ST-008**: Criterion benchmark + baseline commit (2h)
- **ST-009**: Rustdoc + README + integration em auth_model.md §8.1 (3h)
- **ST-010**: ADR-0043 (HMAC algorithm choice; tenant prefix derivation) (2h)
- **ST-011**: Chaos experiments C-1, C-2 (4h)
- **ST-012**: PRR preparation + sign-off orchestration (4h)

Total: ~38h efetivas (~5 dias úteis de 1 dev full-time em HIGH_RISK com revisões).

## 18. Dependencies

- **Hard:** Framework v1.0.0-rc1 ✅ (done).
- **Hard:** Canonical sources INVARIANT-REGISTRY, KEY-MANAGEMENT, SECURITY-MODEL, AUTH-MODEL em DRAFT ✅.
- **Hard:** TLA+ `tenant_isolation.tla` passa verde local ✅ (done Lote 7.1).
- **Soft:** Cloudflare Workers Secrets binding mock para local dev — criar no ST-002.

## 19. Effort Estimate (PERT, HIGH_RISK)

| Scenario | Horas |
|---|---|
| Best case | 28h |
| Most likely | 38h |
| Worst case | 60h (se ADR-0043 trava em bikeshed ou HMAC bench falhar CF target) |
| **PERT** (4O + B + W) / 6 | **~42h** |

**T-shirt equivalente:** M (Medium, 1 sprint week).

## 20. Time-boxing & Escalation

- **Start:** 2026-04-28 (Mon)
- **Mid-check:** 2026-05-02 (Fri)
- **Target complete:** 2026-05-05 (Mon)
- **Escalation trigger:** se > 60h gastos e incompleto, escalate ao Aprovador Final para re-scoping ou split em WIs adicionais.

## 21. Observability Plan

Herda `observability_model.md §4`. **Delta local:** nenhuma métrica/log/trace emitido diretamente por este crate (pure function). Callers devem emitir:

- `corelink_tenant_path_derive_duration_seconds` (histogram; p99 target < 100μs).
- `corelink_tenant_path_errors_total{error}` (counter; deveria ser sempre 0 em produção).

## 22. Cost Analysis (HIGH_RISK = obrigatório + TCO 12m)

- **Cost per call:** ~20μs CPU + ~256 bytes stack. Em CF Workers: negligível (free tier).
- **TCO 12m:** ~0 (código interno, sem infra extra). Impacto indireto: permite tier enterprise launch (revenue positivo).
- **Risk cost:** falha deste WI = bloqueio de **todo** o roadmap S01–S04 (cross-tenant + GC + AC todos dependem).

## 23. API / Contract Impact

- **API externa:** zero (crate interno).
- **API interna:** define contrato `derive_prefix` que será consumido por WI-S01-003, 004, 005. Bump major do crate **DEVE** trigger re-audit de callers.

## 24. Post-mortem Hooks (HIGH_RISK = obrigatório)

Trigger automático de post_mortem se:
- Qualquer SEV-1 em produção referenciar bug em `tenant_path::derive_prefix` (implicaria FM-253).
- Property test regression detectada em CI (zero tolerância).
- SAST finding em PR subsequente que modifique a lib.

## 25. Rollback / Recovery

- **Code rollback:** revert commit; git tag previous version. Callers não mudam API → sem cascata.
- **Data rollback:** N/A (crate não persiste data).
- **Se regressão em production:** emergency degrade_mode=read-only enquanto re-deploy ocorre (PAT-DEGRADE-001). RPO = 0 (data já escrita é protegida pelo prefix já correto).

## 26. Security & Privacy (HIGH_RISK = STRIDE + LINDDUN completos)

### 26.1 STRIDE (herda `security_model.md §5`)

| Ameaça | Aplica? | Mitigação local |
|---|---|---|
| **S** Spoofing | Sim (Tenant A passa-se por B) | HMAC-SHA256 cryptographic — atacante precisa quebrar HMAC para forge prefix de outro tenant |
| **T** Tampering | Sim (muta body mid-derive) | Função é pura (não há state inter-call); input imutável (UUID + TDK ref) |
| **R** Repudiation | N/A (crate interno; audit no call site) | — |
| **I** Info disclosure | TDK em memory heap? | Custom Debug impl redacta; zeroize pending (ST-010 adiciona `zeroize` crate) |
| **D** DoS | Slow-HMAC? | Criterion benchmark garante < 100μs; resistente a adversarial input |
| **E** Elevation | Bypass via raw byte construction | Newtype com field private; impossible by construction |

### 26.2 LINDDUN (herda `privacy_model.md §4`)

| Threat | Aplica? |
|---|---|
| **L** Linkability | Indiretamente (prefix ≡ identidade do tenant). Mitigação: HMAC sem reveal de tenant_id plaintext. |
| **I** Identifiability | Dev com acesso a TDK pode reverter tenant_id → prefix, mas HMAC one-way impede o inverso. OK. |
| **N** Non-repudiation | N/A (sem log neste crate). |
| **D** Detectability | N/A. |
| **U** Unawareness | N/A (crate interno; cliente não interage). |

### 26.3 Novos trust boundaries

Nenhum. Mantém TB-0..TB-5 canônicos.

## 27. Knowledge Transfer (HIGH_RISK = obrigatório + onboarding test)

- Rustdoc completo publicado interno.
- Arch diagram em `auth_model.md §8.1` atualizado com box "Layer 5 = crate tenant-path".
- Onboarding test: novo dev consegue derive prefix corretamente em 15 min seguindo rustdoc + exemplo.
- Presentation em eng all-hands pós-merge.

## 28. Risk Register (HIGH_RISK = inclui detectability + exposure + residual)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual pós-mitigação |
|---|---|---|---|---|---|---|
| R-001 | HMAC-SHA256 crate `hmac` tem CVE oculto | L | M | HIGH | `cargo-audit` daily; dupla hash opcional | LOW (CVE resposta 48h) |
| R-002 | Bench p99 > 100μs em CF Workers real | M | L | MEDIUM | Staging benchmark em CF runtime antes de PR | LOW |
| R-003 | Dev futuro adiciona `pub fn derive_prefix_raw()` | L | H | CRITICAL | `#[deny(missing_docs)]` + code review gate + property test regression | MEDIUM (code review é humano) |
| R-004 | TDK leak em log via panic trace | L | L | CRITICAL | Custom Debug redact + zeroize + SAST grep | LOW |

## 29. Review Checkpoints (HIGH_RISK = code + design + pre-merge + adversarial)

- **Design review** (após ST-003): Tech Lead + Security Lead revêem API + invariantes.
- **Pre-PR review** (após ST-009): todos os 11 signoff roles canonical chegam em parallel branches.
- **Adversarial review** (após merge em staging): pentest interno tentando forge prefix de outro tenant via fuzz + code review.

## 30. Sign-off (HIGH_RISK = 11 roles canonical; framework §33.5.4.3)

Preencher ao final. Alinha 1:1 com matriz canônica em `00_framework.md §33.5.4.3`.

| Papel | Nome | Critério | Assinatura | Data |
|---|---|---|---|---|
| WI Owner | Gustavo Schneiter | §11 DoD 100% | _____________ | YYYY-MM-DD |
| Code Reviewer | TBD | §10.1-10.2 ✅ | _____________ | YYYY-MM-DD |
| Security Reviewer | TBD | §26 + SAST + dep audit | _____________ | YYYY-MM-DD |
| Privacy Reviewer | TBD | §26.2 LINDDUN | _____________ | YYYY-MM-DD |
| SRE Reviewer | TBD | §21 observability + §15 chaos | _____________ | YYYY-MM-DD |
| QA Reviewer | TBD | §8 AC + property test | _____________ | YYYY-MM-DD |
| Product Reviewer | TBD | §3 customer impact | _____________ | YYYY-MM-DD |
| Architect | TBD | §9 ADR + §12 invariantes | _____________ | YYYY-MM-DD |
| Cost Owner | TBD | §22 TCO | _____________ | YYYY-MM-DD |
| Legal | N/A | — | — | — |
| Sprint Owner | Gustavo Schneiter | §17 sub-tasks done | _____________ | YYYY-MM-DD |
| Aprovador Final | Gustavo Schneiter | Lane HIGH_RISK completa | _____________ | YYYY-MM-DD |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | Criação WI — Foundation da lib tenant_path. Lane HIGH_RISK com FF-HR-002 + FF-HR-005. 12 sub-tasks, ~42h estimado. |

## 32. Apêndice: Anti-patterns evitados

Este WI foi estruturado para evitar explicitamente:

- **AP-004 ("testes cosméticos")**: Property test 10k iter + fuzz 1h não é teatro.
- **AP-007 ("cerimônia sem rigor")**: Lane HIGH_RISK é justificada por FF-HR-002 real, não "sinto que é importante".
- **AP-019 ("rigor superficial")**: Type-driven security (newtype private field) carrega invariante no sistema de tipos, não no comentário.
- **AP-028 ("smuggling global state")**: `TenantDerivationKey` passado como argumento, nunca como static. Explicit is better.
- **AP-WAIVER-001..005**: Sem waivers neste WI.

---

**Fim de WI-S01-001.** Next: implementar ST-001 (skeleton crate). ADR-0043 (HMAC choice; tenant prefix derivation) a criar em paralelo.
