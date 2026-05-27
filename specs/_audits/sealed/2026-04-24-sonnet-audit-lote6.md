---
id: AUDIT-LOTE6-SONNET
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-24
reviewers: [Sonnet 4.6 via Agent tool]
supersedes: null
superseded_by: null
tags: [audit, lote6, tla-v2, final]
---

# Audit Lote 6 (Sonnet — 3ª iteração)

## Escopo

**Commits auditados:**
- `1f89b6f` Lote 6.1: TLA+ specs v2 (G-01/G-02/G-03)
- `2c976be` Lote 6.2: INV-AUDIT-APPEND-ONLY TLA+ + TODO.md fix (T-01, T-02)
- `362e6af` Lote 6.3: validator INV CamelCase + EVT disambiguation (G-04, G-05)
- `cb16321` Lote 6.4: 6 runbooks P1 (T-03, T-04)
- `5427c93` Lote 6.5: consent P2.1 expandido (G-07)
- `3253b2f` Lote 6.6: sprint HIGH_RISK 10-12 signoffs + cleanup (G-08..G-13)
- `7f4187b` whitelist CTRL-KEY-030..032 Fase 2
- `cb0c9d8` Lote 6.7: orphan CTRLs + tier interpolation (T-06, T-09)
- `ee5d3e7` fix expandir CTRL-ISO-001..005 em FM-253

**Validações executadas:**
- `validate_references.py --warn-orphans`: zero dangling. Orphans documentados.
- `validate_references.py --json`: contadores CTRL(87/88), RB(26/27), SLO(13/19), INV(26/44) analisados.
- Inspeção manual completa dos 4 TLA+ specs e configs.
- Grep sistemático por EVT-001 misuse, "a criar", magic constants, tautologias, stale refs.
- TLC não executado localmente (Java runtime ausente); state files confirmam runs: `states/26-04-24-17-24-34/gc_correctness.st` e `states/26-04-24-17-24-40/audit_immutability.st`.

---

## Resolution check

### Sonnet anteriores (T-01 a T-12)

**T-01 (TODO.md plaintext tenant_id HIGH):** ✓ ADDRESSED.
`TODO.md` linha 23 reescrita para o formato completo HMAC com nota explícita: "HMAC obrigatório, plaintext `tenant_id` em key é **proibido** — viola CTRL-AUTH-004 + CTRL-ISO-001 e quebra INV-TENANT-ISOLATION. Corrige T-01 do audit Lote 5." Alinhado com `remote_cache_product_profile.md §7.1`. Fix substantivo, não teatro.

**T-02 (INV-AUDIT-APPEND-ONLY CRITICAL sem TLA+ e sem waiver HIGH):** PARTIAL — com regressão técnica.
`specs/tla/audit_immutability.tla` foi criado (Lote 6.2). A `invariant_registry.md §3.6` linha 116 agora referencia o spec. Estado files confirmam execução real. Porém:
1. A TLA+ coverage matrix em `invariant_registry.md §4` linha 171 ainda diz: `"✅ (sem TLA+ necessário — enforcement storage layer)"` — stale, contradiz linha 116 do mesmo arquivo. Contradição interna no canonical source.
2. `audit_immutability.tla` tem dois invariantes com **definições idênticas** (`InvAuditAppendOnly` e `InvAuditChainIntact` — ambos: `\A i \in 2..Len(audit_log): audit_log[i].prev_hash = audit_log[i-1].record_hash`). Um deles é dead code. Ver U-03 abaixo.
3. Modelo adversarial fraco (ver U-04 abaixo).

**T-03 (6 P1 FMs sem runbook HIGH):** ✓ ADDRESSED.
Todos os 6 runbooks criados: `RB-FM-007`, `RB-FM-062`, `RB-FM-100`, `RB-FM-101`, `RB-FM-156`, `RB-FM-258`. `failure_modes.md §6.1` atualizado para listar todos os 26 RBs. Fix completo.

**T-04 (FM-253 referência a RB-FM-303 errada MEDIUM):** ✓ ADDRESSED.
`failure_modes.md §3.7 FM-253` coluna CTRLs agora lista `RB-FM-253` corretamente (além de CTRL-ISO-001..005 expandidos via commit `ee5d3e7`).

**T-05 (key_management.md refs "a criar" stale MEDIUM):** ✓ ADDRESSED.
Referências "a criar em Lote 5.7" para `RB-KEY-COMPROMISE`, `RB-HSM-UNAVAILABLE`, `RB-BYOK-REVOKE` removidas de `key_management.md`. `privacy_model.md §11.3` RB-BREACH-NOTIF: still shows "a criar" → **NOT verified** — grep retornou `privacy_model.md:402` com `legal/dpa/v1.md — a criar pré-GA` (mas este é legal backlog, aceitável) e `storage_semantics_matrix.md` não mostra mais a TLA+ ref stale. Parcialmente endereçado, resíduo em legal backlog (aceitável pré-GA).

**T-06 (SLO 5 tiers na tabela mas per-SLO só team/enterprise MEDIUM):** ✓ ADDRESSED via interpolation rule.
`slo_catalog.md §3.1` (linhas 98-107) adiciona regra de interpolação explícita para free/solo/business derivados de team e enterprise. A matemática tem inconsistência menor (ver U-07 abaixo).

**T-07 (gc_correctness.tla sem resurrection path MEDIUM):** NOT ADDRESSED, but deferred.
Comentário na linha 192-193: `"O modelo v2 não tem resurrection path"`. A nota reconhece o gap. Nenhuma ação tomada além de documentar. Permanece como gap de coverage formal.

**T-08 (tenant_isolation.tla path guessing bounds 50% hit rate MEDIUM):** PARTIAL.
Lote 6.1 adicionou `TryWriteCrossTenant` adversarial action. O `PathGuess` bounds problem com 2 tenants permanece (espaço de guessing = {1,2} = mesmo tamanho dos prefixos). Mais importante: ver U-02 abaixo para problema mais profundo que subsume T-08.

**T-09 (48 CTRL orphans MEDIUM):** PARTIAL.
Lote 6.7 cross-referenciou alguns CTRLs em FMs. Após Lote 6.7, validate_references ainda lista **31 CTRL orphans** (vs 48 anteriores). Progresso real mas incompleto. Os críticos CTRL-ISO-002..005 agora aparecem em FM-253 — esse subconjunto foi endereçado.

**T-10 (RB-BREACH-NOTIF placeholder@law-firm.com LOW):** PARTIAL.
`RB-BREACH-NOTIF.md` linha 99 agora diz "Placeholder `legal@humangr.com` route interno" (não mais "placeholder@law-firm.com"). Templates legais ainda "a criar" (3 refs). Aceitável pré-GA com nota explícita de tracking.

**T-11 (PAT-AUTHZ-002 orphan LOW):** NOT ADDRESSED.
`validate_references.py --warn-orphans` ainda lista `PAT-AUTHZ-002` como orphan. Nenhuma ação tomada neste lote. Ruído baixo, sem impacto operacional.

**T-12 (cas_integrity.tla hash_fn como VARIABLE LOW):** NOT ADDRESSED.
`cas_integrity.tla` ainda declara `hash_fn` em `VARIABLES`. Estado não mudou. `hash_fn \in [Bodies -> Digests]` no Init gera 6 universos paralelos para {b1,b2} → {d1,d2,d_wrong}. Impacto de performance, não de corretude. Aceitável dado MaxOps=3.

---

### GPT anteriores (G-01 a G-13)

**G-01 (gc_correctness.tla GC atômico vs multi-pass CRITICAL):** PARTIAL — modelo correto, cfg incompleto.
`gc_correctness.tla` v2 (Lote 6.1) reescrito com `GCMarkStep(b)` que visita um blob por vez. O interleaving entre mark parcial e `UpdateActionResult` é agora modelado. A proteção via `mark_started_at` em `GCSweepBlob` está correta. Porém: **`gc_correctness.cfg` lista apenas `InvGCReRefProtected` no bloco `INVARIANT`**. `InvGCReachableNeverDeleted` (INV-GC-001 — o invariante primário) está definido no .tla mas **não está no .cfg**. O cfg header diz "Endereça INV-GC-001 + INV-GC-004" mas TLC só verificou INV-GC-004. Ver U-01 abaixo.

**G-02 (cas_integrity.tla BitRot com flag booleana CRITICAL):** ✓ ADDRESSED.
`BitRot(d, new_body)` em v2 **muta** `r2_storage[d] = new_body` (linha 100). O `r2_storage` agora é genuinamente alterado pelo adversário. `InvCASIntegrityUncorrupted` check para d sem flag; `InvClientVerifyIsSound` prova que client que recebe `verify_ok` obteve body correto mesmo sob BitRot. Fix substantivo.

**G-03 (tenant_isolation.tla invariante só cobre read CRITICAL→HIGH):** ADDRESSED com ressalva.
Lote 6.1 adicionou: `InvTenantIsolationWrite`, `InvTenantIsolationEnum`, `TryWriteCrossTenant`, `List` retornando conteúdo explícito. O cfg lista os 5 invariantes. Porém: `Write(p, b)` tem assertion tautológica (`/\ tenant_of[p] = t` onde `t` é definido como `tenant_of[p]` no LET) e `TryWriteCrossTenant` registra "denied" sem jamais tentar a write. Ver U-02 abaixo para análise completa.

**G-04 (validate_references.py cego para INV CamelCase HIGH):** ✓ ADDRESSED.
Regex `INV` expandido para `[A-Za-z][A-Za-z0-9_-]+`. `INV-TenantIsolation`, `INV-CASIdempotency`, etc. agora detectados. `invariant_registry.md §3.11` adicionou 3 INVs legados com aliases. Whitelist atualizada. Fix completo.

**G-05 (EVT taxonomy semântica aberta CRITICAL→HIGH):** PARTIAL — residual.
EVT-047 (AUDIT_EVENT), EVT-048 (DSR_EVIDENCE), EVT-049 (CONSENT_EVENT) criados em framework §35.7.1. Refactors executados em 6 arquivos. Mas `privacy_model.md` linha 441 ainda usa `EVT-001` (CI_LOG) para "Sub-processor register diff check Semanal" — evidentemente não é um CI pipeline log. Ver U-05 abaixo.

**G-06 (trilha FM→RB quebrada HIGH):** ✓ ADDRESSED.
`failure_modes.md §6.1` agora lista todos os 26 RBs. FM-253 referencia RB-FM-253 correto. Orphans de runbooks (RB-FM-051/054/202/206/400/403/404) agora catalogados em §6.1. Trilha fechada.

**G-07 (CTRL-PRIV-CONSENT sem proof of informed consent HIGH):** ✓ ADDRESSED.
CTRL-PRIV-CONSENT-001..006 expandidos em Lote 6.5. Campos adicionados: `notice_text_hash`, `notice_version`, `locale`, `wording_id`, `ui_capture_timestamp`, `submission_timestamp`. CTRL-PRIV-CONSENT-005 (notice versioning + re-consent em mudança material). CTRL-PRIV-CONSENT-006 (screenshot evidence opcional enterprise). Fix substantivo para GDPR Art. 7.

**G-08 (sprint HIGH_RISK inconsistente com framework MEDIUM):** ✓ ADDRESSED.
`sprint_contract.md §20` linhas 872/895 agora declarados "10–12 sign-offs obrigatórios (alinha com framework §33.5.4.3)". Tabela detalhada com 11 papéis (10 obrigatórios + 1 condicional). Framework `§33.5.4.3` linha 2127: "Total mínimo: 10-12". Alinhados.

**G-09 (key_management.md CTRL-KEY-020..022 drift MEDIUM):** ✓ ADDRESSED.
Renumeração executada: `CTRL-KEY-030..032` são os placeholders Fase 2 BYOE. `CTRL-KEY-020` (all key ops logged) e `CTRL-KEY-021` (dual-approval) são controles ativos em §8.5. `CTRL-KEY-022` foi descontinuado (não aparece mais). Whitelist atualizada. `key_management.md` linha 172 documenta a renumeração com nota de rastreabilidade.

**G-10 (material "a criar" em paths críticos MEDIUM):** PARTIAL.
`key_management.md` stale refs removidas. `storage_semantics_matrix.md` spec TLA+ não mais "a criar". Residual aceitável: `RB-BREACH-NOTIF` templates legais (3 refs "a criar") com nota pré-GA. `legal/dpa/v1.md` ainda "a criar pré-GA" (legal backlog externo, aceitável).

**G-11 (ADRs FROZEN com checklists abertos MEDIUM):** PARTIAL — com regressão.
ADR-0012 linha 64 ainda tem `[ ] Atualizar failure_modes.md §3.7 (FM-300, FM-303, FM-404) com referência ao novo forcing factor FF-HR-011`. Este item permanece aberto mesmo com o ADR em `FROZEN`. FM-300/303/404 não referenciam `FF-HR-011` em seus campos CTRLs. ADR-0013 tinha `[ ] Criar invariant_registry.md` e `[ ] Criar key_management.md` — ambos criados, mas checklists não marcados `[x]`. Theater leve.

**G-12 (compliance_matrix version drift LOW):** ✓ ADDRESSED.
Front matter e corpo agora consistentes: `version: 0.2.0`, `Versão: 0.2.0`, `updated: 2026-04-24`. Alinhados.

**G-13 (slo_catalog linha 115 chama RB-SLO-AVAIL-CP de "stub" LOW):** ✓ ADDRESSED.
`slo_catalog.md` linha 126: `"RB-SLO-AVAIL-CP"` referenciado como link ativo sem label "stub". Fix completo.

---

## Novos findings

### CRITICAL

---

**U-01: gc_correctness.cfg não verifica INV-GC-001 (reachable never deleted) — invariante primária ausente do model check**

`specs/tla/gc_correctness.cfg` bloco INVARIANT contém apenas `InvGCReRefProtected`. O header do arquivo afirma: "Endereça INV-GC-001 + INV-GC-004 (CRITICAL em invariant_registry §3.4)". `InvGCReachableNeverDeleted` (que mapeia para INV-GC-001) está definida no .tla (linha 197) mas **ausente do .cfg**.

Consequência: TLC verificou apenas INV-GC-004 (a proteção de re-referência pós-mark). INV-GC-001 — "blob reachable quando sweep acontece nunca é deletado" — não foi model-checked. O claim de que o GC está formalmente verificado é parcialmente falso: a invariante primária e mais importante não está na run do TLC.

Agravante: `InvGCReRefProtected` (a única invariante no cfg) usa um magic constant `deleted_at - 10` (linha 214). Com `MaxTime=10` e `GracePeriod=2`, `deleted_at` pode ser no máximo 10, tornando `deleted_at - 10 = 0` — e a condição `created_at >= 0` é sempre verdadeira para timestamps naturais. Assim `InvGCReRefProtected` colapsa para `~(exists e: b in ac_entries[e].blob_refs)`, que é **trivialmente garantida pela precondição de `GCSweepBlob`** (que também requer ausência de refs com mark_started_at). A invariante é tautológica no modelo configurado e portanto inútil como verificação.

Correção: (a) adicionar `InvGCReachableNeverDeleted` ao bloco INVARIANT do cfg; (b) substituir `deleted_at - 10` por `mark_started_at` na definição de `InvGCReRefProtected` para tornar a verificação semanticamente correta.

---

### HIGH

---

**U-02: tenant_isolation.tla — Write assertion é tautológica; TryWriteCrossTenant nunca exerce storage layer**

Dois problemas estruturais na spec v2:

**(a)** `Write(p, b)` linha 90: `/\ tenant_of[p] = t` onde `t` é definido como `tenant_of[p]` no bloco LET. Esta é uma identidade, não uma verificação — equivale a `x = x`. A "assertion dupla" comentada como `(CTRL-AUTHZ-002)` não adiciona nenhuma restrição ao espaço de estados do TLC. Qualquer modelo que passe com esta assertion passaria sem ela.

**(b)** `TryWriteCrossTenant(p, victim_tenant, b)` linhas 162-177: a action registra `<<"write", b, "denied">>` no log mas nunca tenta mutar `r2_storage`. Isso significa que o TLC nunca explora um estado onde o atacante **efetivamente escreveu** em namespace errado. `InvTenantIsolationWrite` verifica se "write allowed implica blob em owned_blobs do tenant" — mas como TryWriteCrossTenant nunca produz outcome "allowed", a invariante de write é verificada apenas sobre writes legítimos de `Write(p, b)`, onde `owned_blobs` é atualizado atomicamente com o write. A invariante de write isolation é **trivialmente verdadeira por construção**, não por verificação adversarial.

Para verificar write isolation genuinamente, o modelo precisaria de uma ação que tente inserir em `r2_storage[<<victim_prefix, b>>]` e verificar que invariantes bloqueiam ou que nenhum estado com cross-tenant storage é atingível.

Impacto: os dois invariantes adicionados no Lote 6.1 para "endereçar G-03" passam pelo TLC mas não provam o que prometem. A cobertura real de write isolation é zero.

---

**U-03: invariant_registry.md §4 TLA+ coverage matrix contradiz §3.6 no mesmo arquivo — stale após Lote 6.2**

`invariant_registry.md` linha 116 (§3.6): `"D1 CHECK constraint + R2 Object Lock + daily chain verify (PAT-AUDIT-VERIFY-001) + **TLA+ em specs/tla/audit_immutability.tla** (criado Lote 6.2)"`.

`invariant_registry.md` linha 171 (§4 coverage matrix): `"INV-AUDIT-APPEND-ONLY | Cobertura D1 schema + daily verify | ✅ (sem TLA+ necessário — enforcement storage layer)"`.

A mesma linha de tabela que Lote 6.2 deveria ter atualizado permaneceu com o texto da justificativa de exemption. O canonical source contradiz a si mesmo. Em auditoria SOC 2, o auditor lê o coverage matrix e conclui que INV-AUDIT-APPEND-ONLY não tem TLA+.

Correção: atualizar linha 171 para `"INV-AUDIT-APPEND-ONLY | specs/tla/audit_immutability.tla | ✅ criado Lote 6.2"`.

---

**U-04: audit_immutability.tla modelo adversarial é teatral — TryDelete/Replace/Reorder nunca mutam o log**

`audit_immutability.tla` modela os ataques como: adversário "tenta" delete/replace/reorder, mas o modelo **garante por construção** que `audit_log` e `hash_counter` ficam `UNCHANGED`. `InvAuditAppendOnly` verifica que a hash chain é válida — mas a chain é válida por definição porque ninguém pode quebrá-la no modelo.

O invariante nunca pode ser violado: o modelo não tem nenhuma ação que altere audit_log além de `AppendEvent`. TLC verificando este modelo descobre ~5335 estados onde a hash chain sempre permanece íntegra — o que é trivialmente garantido pelo modelo, não pela propriedade do sistema real.

Analogia com o problema anterior (T-02): a justificativa original da exemption era "enforcement storage layer (D1 + R2 Object Lock)". O TLA+ criado em resposta modela exatamente o mesmo argumento como axioma, não como propriedade verificada. O TLA+ não adiciona rigor novo; institucionaliza o raciocínio da exemption como "spec".

Para ter valor real, o modelo precisaria de uma ação `BuggyAppend(i, replacement)` que MUTA `audit_log[i]` e verificar que `InvAuditAppendOnly` detectaria a violação da chain — i.e., o TLA+ precisa mostrar que o invariante É violável quando a proteção falha, não apenas que não é violado quando o adversário não tem poder.

Adicionalmente: `InvAuditAppendOnly` e `InvAuditChainIntact` têm **definições bit-a-bit idênticas** (linhas 142-152). Um deles é dead code na verificação.

---

### MEDIUM

---

**U-05: G-05 residual — privacy_model.md:441 ainda usa EVT-001 (CI_LOG) para verificação de sub-processors semanal**

`privacy_model.md` linha 441: `"Sub-processor register diff check | Semanal | EVT-001"`.

Lote 6.3 commit refatorou EVT-001 em privacy_model §5.6 (consent) e §6.1 (DSR) mas perdeu esta linha em §12 (tabela de testes periódicos). Um diff check semanal de sub-processors não é um log de CI pipeline (EVT-001 = CI_LOG). A evidência correta seria EVT-047 (AUDIT_EVENT) para registro de compliance, ou EVT-040 (AUDIT_REPORT) se gera relatório formal.

G-05 está marcado como ADDRESSED mas este resíduo específico permanece. Em auditoria de conformidade LGPD Art. 41 (sub-processor management), a evidência de monitoramento semanal apontará para EVT-001 cujo formato é "URL GitHub Actions / GitLab CI run" — irrelevante para registro de mudança em lista de sub-processors.

---

**U-06: ADR-0012 FROZEN com checkbox aberto — FM-300/303/404 não referenciam FF-HR-011**

`specs/03_architecture/adrs/ADR-0012-ff-hr-011-gc-reachability.md` linha 64: `"[ ] Atualizar failure_modes.md §3.7 (FM-300, FM-303, FM-404) com referência ao novo forcing factor."` Este é o único item incompleto em um ADR FROZEN.

Verificação em `failure_modes.md`:
- FM-300: `"INV-GC-001 TLA+ + PAT-SOFT-DELETE-001"` — sem FF-HR-011.
- FM-303: `"INV-TenantIsolation + test integração + RB-FM-303"` — sem FF-HR-011.
- FM-404: `"INV-GC-001 TLA+ linearizability"` — sem FF-HR-011.

FF-HR-011 é o forcing factor que obriga lane HIGH_RISK para qualquer WI que altere GC/refcount/reachability. Sem a referência nos FMs, o time que abre um WI para FM-300/404 não saberá que deve forçar HIGH_RISK. O forcing factor existe mas sua aplicação está invisível no ponto de uso.

---

**U-07: tier interpolation matemática é internamente inconsistente com a própria tier table**

`slo_catalog.md §3.1` declara regra de interpolação de latência: `enterprise = team × 0.67`.

Verificação com CAS GET latency (team=300ms, tier table enterprise=200ms):
- `300 × 0.67 = 201ms` (tier table diz 200ms — delta 0.5%)
- `business = 300 × 0.85 = 255ms` (tier table diz 250ms — delta 2%)
- `solo = 300 × 1.3 = 390ms` (tier table diz 400ms — delta 2.5%)
- `free = 300 × 1.6 = 480ms` (tier table diz 500ms — delta 4%)

A regra de interpolação em §3.1 não reproduz os valores declarados na própria tabela §3. Para um cliente `solo`, a regra diz 390ms mas o contrato diz 400ms. Para `free`, 480ms vs 500ms. Estes são targets diferentes — qual prevalece? O preamble diz que §4.x pode "override interpolation via linha explícita", mas os SLOs em §4.x não têm linha solo/business — logo a interpolation rule prevalece, contradizendo a tier table.

Impacto: contratual. SLA de latência para tiers free/solo/business é ambíguo entre §3 (tier table) e §3.1 (interpolation rule).

---

### LOW

---

**U-08: ADR-0013 e ADR-0014 têm checkboxes não marcados mesmo com items completados**

`ADR-0013` linhas 72-73: `[ ] Criar invariant_registry.md` e `[ ] Criar key_management.md` — ambos criados em Lote 5.4/5.10 mas checkboxes não marcados `[x]`.

`ADR-0014` linhas 74-77: 4 items de pipeline CI (cargo-cyclonedx, SPDX opcional, cosign, Dependency-Track) permanecendo `[ ]`. Aceitável para implementação futura mas ADR está FROZEN — os items abertos pertencem a um WI de backlog, não ao ADR.

Padrão: ADRs devem documentar decisões e status de implementação; items de backlog futuro devem ser WIs. ADRs com checkboxes eternamente abertos criam ambiguidade sobre o que foi decidido vs o que foi executado.

---

**U-09: validate_references usa_count=88 > definitions=87 para CTRL — CTRL-KEY-030 contado como "use" mas é placeholder em whitelist**

`validate_references.py --json` reporta `CTRL uses=88` vs `definitions=87`. A diferença de 1 é explicada: `key_management.md` linha 172 contém `"CTRL-KEY-030..032"` como texto de nota, o regex extrai `CTRL-KEY-030` como match (a notação `..032` não captura 031 e 032 individualmente). CTRL-KEY-030 é whitelisted, logo não aparece como dangling.

O report "zero dangling references" é tecnicamente correto mas mascara que um placeholder de Fase 2 está sendo "usado" (via nota de texto) — a whitelist absorve sem alertar. Se o whitelist for removido ou o placeholder for promovido prematuramente, este comportamento tornará-se dangling silencioso.

Impacto baixo mas a discrepância use/definition é confusa para futuros auditores.

---

## Veredito final

**CONDITIONAL — mais próximo de GREEN do que RED, mas com blockers técnicos reais nos TLA+**

O Lote 6 representa um volume de trabalho genuíno e endereça a maioria dos 25 findings anteriores de forma substantiva. Os fixes operacionais (runbooks, FM-253 ref, CTRL-PRIV-CONSENT expandido, sprint 10-12 signoffs, EVT disambiguation, tier interpolation) são reais e fecham gaps. O TODO.md fix é crítico e foi executado corretamente.

Os três blockers desta iteração:

**1. U-01 (gc_correctness.cfg não verifica INV-GC-001):** O invariante primário do GC — reachable never deleted — não está no model check. O cfg só verifica INV-GC-004 via uma fórmula tautológica. A claim "G-01 ADDRESSED" no commit `1f89b6f` é verdadeira para o modelo mas não para a verificação. Correção: duas linhas no cfg + fix da fórmula `deleted_at - 10 → mark_started_at`.

**2. U-02 (tenant_isolation.tla write isolation teatral):** `InvTenantIsolationWrite` não prova nada porque o adversário (TryWriteCrossTenant) nunca recebe poder de escrever. A assertion `tenant_of[p] = t` é uma tautologia. G-03 foi marcado como addressed mas write isolation permanece não verificada formalmente.

**3. U-04 (audit_immutability.tla adversarial é por construção):** O 4º TLA+ spec prova que um modelo onde ninguém pode mutar o log tem chain válida. Isso não é evidência de que o sistema real mantém a chain íntegra sob condições adversariais. O spec não adiciona rigor além do raciocínio informal original da exemption.

Sem os 3 fixes acima, o Lote 6 entrega progresso operacional real mas os TLA+ specs — o core da diferenciação formal do CoreLink — ainda têm fragilidades estruturais que um auditor técnico detectaria na primeira leitura.

---

## Observações meta

**Pontos fortes do Lote 6:**

- `gc_correctness.tla` v2 — o modelo multi-pass com `GCMarkStep` e `mark_started_at` é genuinamente melhor que v1. O race de re-referência está corretamente modelado em `GCSweepBlob` e `UpdateActionResult`. A lógica de proteção está certa; o cfg e InvGCReRefProtected é que precisam de ajuste.
- `cas_integrity.tla` v2 com `BitRot` que muta `r2_storage` — este é o fix mais limpo do lote. A reformulação dos invariantes em `InvCASIntegrityUncorrupted` + `InvClientVerifyIsSound` é elegante e semanticamente correta.
- CTRL-PRIV-CONSENT-001..006 com `notice_text_hash + notice_version + locale + wording_id + ui_capture_timestamp` — nível de rigor comparável a implementações GDPR enterprise maduras. Endereça Art. 7 de forma genuína.
- `failure_modes.md §6.1` com 26 RBs completos e nenhum "a criar" sem tracking — gap de rastreabilidade fechado.
- `sprint_contract.md §20` com 10-12 signoffs HIGH_RISK alinhados ao framework — a inconsistência histórica foi finalmente removida.
- Lote 6.3 EVT disambiguation: a criação de EVT-047/048/049 resolve o problema raiz de EVT-001 sendo usado como coringa para eventos semanticamente distintos. O refactor cobriu 6 arquivos de forma sistemática.

**Gaps não avaliados:**

- TLC não executado. State files existem para `gc_correctness.tla` (17:24:34) e `audit_immutability.tla` (17:24:40) mas não para `tenant_isolation.tla` e `cas_integrity.tla` — os runs desses dois specs são do Lote 5 (state dir com 153+ files, referenciado no audit anterior). Novos state files do Lote 6.1 não foram confirmados para tenant_isolation e cas_integrity.
- `scripts/validate_specs.py` retorna erro "jsonschema não instalado" — validação de schema não executável no ambiente.
- Código Rust em `src/main.rs` permanece stub. Nenhuma implementação auditada.
- `_waivers/` vazio — sem waivers formais registrados; aceitável enquanto os compensating controls estiverem documentados nos canonical sources.

**Gap SOTA restante:**

A diferenciação CoreLink em specs formais é real mas os TLA+ specs têm uma fragilidade sistêmica: os modelos adversariais são implementados como "attempt is always rejected" (TryWriteCrossTenant, TryDelete, TryReplace) em vez de "attempt may succeed; invariant catches it". Isso é um padrão de TLA+ que prova corretude do happy path, não robustez sob adversário. Para specs que vão suportar discurso de "formal proof-backed security", esta distinção é relevante e perceptível por qualquer revisor com experiência em model checking.
