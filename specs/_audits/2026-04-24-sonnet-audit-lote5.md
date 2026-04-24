---
id: AUDIT-LOTE5-SONNET
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-24
reviewers: [Sonnet 4.6 via Agent tool]
supersedes: null
superseded_by: null
tags: [audit, lote5, tla, runbooks]
---

# Audit Lote 5 + runbooks + TLA+ specs (Sonnet)

## Escopo revisado

**Arquivos auditados:**

- `specs/00_framework.md` (v0.5.0)
- `specs/03_architecture/invariant_registry.md` (novo — Lote 5.4)
- `specs/03_architecture/key_management.md` (novo — Lote 5.4)
- `specs/03_architecture/auth_model.md`, `storage_semantics_matrix.md`, `security_model.md`
- `specs/03_architecture/failure_modes.md`, `resilience_patterns.md`, `slo_catalog.md`
- `specs/03_architecture/privacy_model.md`, `compliance_matrix.md`, `data_model.md`
- `specs/03_architecture/observability_model.md`, `remote_cache_product_profile.md`
- `specs/03_architecture/adrs/ADR-0012`, `ADR-0013`, `ADR-0014` (todos FROZEN)
- `specs/05_quality/runbooks/` (20 runbooks: RB-FM-051, 054, 057, 202, 205, 206, 253, 254, 300, 302, 303, 400, 403, 404, BREACH-NOTIF, BYOK-REVOKE, GDPR-ERASURE-HOLD, HSM-UNAVAILABLE, KEY-COMPROMISE, SLO-AVAIL-CP)
- `specs/tla/tenant_isolation.tla` + `.cfg`, `gc_correctness.tla` + `.cfg`, `cas_integrity.tla` + `.cfg`
- `specs/tla/README.md`
- `scripts/validate_references.py`, `scripts/validate_specs.py`
- `specs/_audits/matrix-stride-ctrl.csv`, `specs/_audits/iso27001-soa.csv`
- `TODO.md` (raiz)

**Validações executadas:**
- `validate_specs.py --verbose`: 38 docs com schema completo, 6 YAML-only. Zero erros.
- `validate_references.py`: zero dangling references. Orphan check executado.
- Inspeção manual de TLA+ specs (TLC não disponível no ambiente; 153 state files do último run confirmam execução real).
- Grep sistemático por TODOs, aliases EVT, stale references, e coverage gaps.

---

## Resolution check — findings anteriores (S-01 a S-20)

**S-01 (EVT taxonomy violation sistêmica):** ✓ ADDRESSED.
Framework §35.7 expandido de 24 para 46 tipos canônicos (EVT-001 a EVT-046). Todos os 142 aliases substituídos por IDs numéricos nos 11 canonical sources. Alias table em §35.7.1.1 permite aliases mnemônicos com mapeamento obrigatório. Compliance_matrix usa EVT-032..046 corretamente. Zero aliases `EVT-<NOME_LIVRE>` detectados fora da alias table. Validator de evidências (`validate_evidence.py`) ainda não implementado — gap técnico, não crítico.

**S-02 (TLA+ contradiction opcional vs obrigatório):** ✓ ADDRESSED.
`auth_model.md §8.3` corrigido: "TLA+ spec (**OBRIGATÓRIO** — CRITICAL invariant)" com referência explícita a `specs/tla/tenant_isolation.tla`. `storage_semantics_matrix.md §3.9` corrigido: "TLA+ **obrigatório**". Ambos citam "Corrigido S-02/F-03". Spec tenant_isolation.tla criada e existente. Não é mais ambíguo.

**S-03 (Retenção audit log 2 anos vs 7 anos):** ✓ ADDRESSED.
`auth_model.md §7.2` reescrito com pipeline explícito: hot (90d) → warm (1y) → cold (≥7y, R2 Object Lock). Texto: "retenção total ≥ 7 anos para SOC 2 / ISO 27001 / LGPD Art. 16". `storage_semantics_matrix.md §3.8` também corrigido (linha 164: "Purge de hot tier | Cron DO rotaciona D1 após 90 dias | Dados continuam disponíveis no archive"). Alinhamento completo com security_model/compliance_matrix.

**S-04 (SBOM format inconsistente):** ✓ ADDRESSED via ADR-0014 (FROZEN).
Framework `§35.7 EVT-010` atualizado: "CycloneDX 1.5+ JSON (preferido no ecossistema Rust via cargo-cyclonedx) ou SPDX 2.3+ JSON/YAML; ambos aceitos (ADR-0014)". Security_model §8.2 alinhado. ADR-0014 em FROZEN. Contradição resolvida via decisão formal.

**S-05 (20 PAT-XXX dangling em failure_modes.md):** ✓ ADDRESSED.
Todos os 20 PATs adicionados a `resilience_patterns.md`. Validate_references confirma 51 PATs definidos, zero dangling. Casos de nome truncado (PAT-DUAL-APPROVAL → PAT-DUAL-APPROVAL-001, PAT-ROLL-FORWARD → PAT-ROLL-FORWARD-001) resolvidos com entries canônicas.

**S-06 (R2 key format inconsistente — HMAC vs plaintext):** ✓ ADDRESSED.
`remote_cache_product_profile.md §7.1` reescrito com HMAC explícito, seção intitulada "Layout R2 (com HMAC tenant prefix — CTRL-AUTH-004)". REG-NAMESPACE-001 atualizado: "Plaintext tenant_id em key é **proibido**". `storage_semantics_matrix.md §3.9` alinhado. Porém: `TODO.md` linha 23 ainda contém `cas/{tenant_id}/{digest}` (ver T-05 abaixo).

**S-07 (INV naming inconsistente):** ✓ ADDRESSED.
`invariant_registry.md` criado como fonte canônica de todos os INV-XXX. Formato canônico `INV-<DOMAIN>-<NAME>` documentado. Aliases históricos (`INV-TenantIsolation`, `INV-AuditLogImmutability`, etc.) mapeados em §5. INVs previamente ausentes (`INV-CONF-AT-REST`, `INV-CONF-IN-FLIGHT`, `INV-AVAIL-ISOLATION`, `INV-CAS-INTEGRITY`) agora têm entries com severity, enforcement, e TLA+ status.

**S-08 (SLO-AVAIL-CAS-GET enterprise 99.99% vs 99.95%):** ✓ ADDRESSED.
`slo_catalog.md §4.2` corrigido: "Target (enterprise): 99.95% (alinhado com tier table §3; 99.99% requer baseline ≥ 30 dias + ADR — proibido sem evidence)". Ambígua "99.99% aspiracional" removida. Tier table §3 e per-SLO entry agora consistentes.

**S-09 (FM-051 P1 sem justificativa de override):** ✓ ADDRESSED.
`failure_modes.md §1` agora documenta explicitamente: "`S = 5` força minimum **P1** independente de RPN". Todos os FMs com override anotados como "P1 (S=5 → upgrade)". FM-051 alinhado com o padrão. Regra de FMEA original (O reflete ocorrência sem mitigações) também documentada (§1 linha 81-83).

**S-10 (CTRL-META-001, CTRL-GC-001, CTRL-CRED-001, CTRL-NET-001..004 dangling):** ✓ ADDRESSED.
Security_model agora tem §6.11 (Credentials: CTRL-CRED-001..003), §6.12 (Network Perimeter: CTRL-NET-001..005), §6.13 (Data Integrity: CTRL-META-001, CTRL-GC-001). Todos com implementação, evidence, e revalidação documentadas. Zero dangling CTRLs na STRIDE matrix.

**S-11 (sprint_contract.md e PRR sem lane-aware sign-offs):** ✓ ADDRESSED.
`production_readiness_review.md`: YAML header tem `lane: "STANDARD"` e `lane_forcing_factors: []`. §21.1 tem tabela lane-conditional: LOW_RISK (3 sign-offs), STANDARD (6), HIGH_RISK (9 obrigatórios + Adversarial Reviewer). `sprint_contract.md`: §20 tem regra lane explícita com 3/5/7 sign-offs por lane. Herança de lane documentada (herda do feature WI, pode escalar nunca rebaixar).

**S-12 (GC race de re-referência via AC entry mid-GC):** ✓ ADDRESSED via TLA+ + INV-GC-004.
`gc_correctness.tla` modela explicitamente o `mark_started_at` timestamp. `CanSweep(b)` verifica: `~(∃e ∈ DOMAIN ac_entries: b ∈ blob_refs AND created_at >= mark_started_at)`. `invariant_registry.md §3.4 INV-GC-004` formalmente definida como CRITICAL. FF-HR-011 adicionado ao framework via ADR-0012 (FROZEN). INV-GC-001 + INV-GC-004 ambas cobertas por `gc_correctness.tla` (TLC verde: ~21k states).

**S-13 (Tier mismatch — 5 tiers produto, 3 tiers SLO):** ✓ ADDRESSED.
`slo_catalog.md §3` agora tem tabela com 5 tiers: free, solo, team, business, enterprise. Alinhado com `remote_cache_product_profile.md §8.2`. Porém: per-SLO entries em §4.x ainda mostram apenas `team` e `enterprise` targets (solo e business não têm targets por-SLO individualizados — ver T-06 abaixo).

**S-14 (CTRL-ISO-001 sem evidence nem revalidação):** ✓ ADDRESSED.
`security_model.md §6.3 CTRL-ISO-001`: "EVT-002 (property test cross-tenant) + EVT-022 (TLA+ INV-TENANT-ISOLATION) | Semestral (alinhado com TDK rotation, ver `key_management.md §3`)". Evidence e revalidação agora preenchidos.

**S-15 (FF-HR-011 proposto mas não existia no framework):** ✓ ADDRESSED via ADR-0012.
ADR-0012 (FROZEN) cria FF-HR-011 formalmente. `00_framework.md §33.5.3` linha 2055 lista: "**FF-HR-011**: Altera algoritmo de garbage collection, refcount, ou invariante de reachability". ADR processado corretamente.

**S-16 (PAT-AUTHZ-002 dangling em slo_catalog):** ✓ ADDRESSED.
`resilience_patterns.md §3.9` adiciona PAT-AUTHZ-001 (entry formal) e PAT-AUTHZ-002 (alias histórico com nota de mapeamento). `slo_catalog.md §4.10` atualizado para referenciar PAT-AUTHZ-001. Validate_references: zero dangling.

**S-17 (compliance_matrix.md tabelas markdown sem cabeçalho):** ✓ ADDRESSED.
Confirmado via leitura: §2.3 (linha 98), §2.4 (linha 106), §2.5 (linha 113), §2.6 (linha 119) — todas têm `| TSC Criterion | Requisito resumido | CTRLs internos | Evidence |` + separador.

**S-18 (storage_semantics_matrix §3.8 sem mencionar R2 archive):** ✓ ADDRESSED.
`storage_semantics_matrix.md §3.8` agora lista 3 tiers: hot (D1/90d), warm (Logpush/R2), cold (R2 Object Lock/≥7y). Pipeline explícito documentado. "Corrigido S-03/S-18" anotado.

**S-19 (Zero P0 FMs suspeito, O otimista em FMs adversariais):** ✓ ADDRESSED.
`failure_modes.md §1` documenta: "FMs adversariais (FM-253, FM-254, FM-303) têm O **mínimo 2** mesmo sem evidência observada". FM-253: O atualizado de 1 para 2 (RPN 20→40). FM-254: O 1→2 (RPN 25→50). FM-303: O 1→2 (RPN 20→40). Todos anotados "O 1→2 em S-19 audit Lote 3+4".

**S-20 (compliance_matrix.md orphan — sem inherits_from):** ✓ ADDRESSED.
`compliance_matrix.md` YAML header: `inherits_from: ["SECURITY-MODEL", "PRIVACY-MODEL", "OBSERVABILITY-MODEL", "AUTH-MODEL", "KEY-MANAGEMENT"]`. Cinco fontes declaradas. Overcorrected na direção positiva (inclui KEY-MANAGEMENT, que não existia no audit anterior).

---

## Novos findings

### CRITICAL (bloqueia GA)

Nenhum. Os blockers do audit anterior foram endereçados. Porém há dois HIGH que juntos podem criar problema em auditoria SOC 2.

---

### HIGH

---

**T-01: TODO.md linha 23 contradiz diretamente o S-06 fix — implementação planeja usar plaintext `tenant_id` em R2 key**

`TODO.md` (raiz, linha 23): `"Namespacing de R2 key por tenant: cas/{tenant_id}/{digest}"`.

O audit anterior S-06 identificou que plaintext `tenant_id` viola CTRL-ISO-001. O fix foi correto nos specs: `remote_cache_product_profile.md §7.1` e REG-NAMESPACE-001 agora proíbem explicitamente plaintext tenant_id. Mas `TODO.md` — o roadmap de implementação semana 2 — ainda instrui a implementar o formato **inseguro**.

Impacto: TODO.md é o documento que um desenvolvedor implementando o Semana 2 vai seguir. Se o roadmap não for atualizado, o primeiro código de produção vai criar R2 keys sem HMAC, violando REF-NAMESPACE-001 e tornando CTRL-ISO-001 teatro desde o dia 1.

Recomendação: atualizar `TODO.md` linha 23 para `HMAC(TDK_tenant, tenant_id)[:16]/<digest_fn>/<hex>` e adicionar nota referenciando `remote_cache_product_profile.md §7.1` e `key_management.md §2`.

---

**T-02: INV-AUDIT-APPEND-ONLY é CRITICAL mas tem exemption de TLA+ sem waiver formal — viola CTRL-FORMAL-001**

`invariant_registry.md §4 TLA+ coverage matrix` (linha 161): "INV-AUDIT-APPEND-ONLY | Cobertura D1 schema + daily verify | ✅ (sem TLA+ necessário — enforcement storage layer)".

O framework §64 e invariant_registry §64 são explícitos: "CRITICAL → TLA+ **obrigatório** (CTRL-FORMAL-001)". A exemption "enforcement storage layer" não tem waiver formal, não tem ADR justificando a exceção, e não tem entry em `_waivers/`. O argument é que D1 CHECK constraint + R2 Object Lock são suficientes — mas isso é um compensating control, não um TLA+ proof.

O audit anterior S-12 mostrou que D1 consistency guarantees têm nuances (Sessions API scope). Se D1 permite uma janela onde duas transações concurrent ambas conseguem fazer INSERT na audit_log (race no ID gerador ou hash chain), INV-AUDIT-APPEND-ONLY seria violado silenciosamente. TLA+ capturaria isso; D1 schema constraints não modelam concorrência.

Impacto: SOC 2 Type II auditoria que cheque CTRL-FORMAL-001 vai perguntar "onde está o TLA+ da invariante CRITICAL INV-AUDIT-APPEND-ONLY?" A resposta atual é "não tem, temos exemption não-documentada". Isso é um gap direto. Em auditoria externa, o auditor pode rejeitar a evidence.

Recomendação: ou (a) criar `specs/tla/audit_append_only.tla` modelando o protocolo de append + hash chain com concorrência, ou (b) abrir um waiver formal com `expires_at`, `compensating_control`, e aprovação Security Lead + Architect, ou (c) reclassificar INV-AUDIT-APPEND-ONLY de CRITICAL para HIGH (requer ADR — provavelmente não desejável).

---

**T-03: 6 P1 FMs sem runbook violam o próprio contrato de failure_modes.md §6 — gap de cobertura operacional não rastreado**

`failure_modes.md §6` (linha 246): "Cada FM P0/P1 **DEVE** ter runbook em `specs/05_quality/runbooks/RB-<FM-ID>.md`".

P1 FMs sem runbook:
- FM-007: Deserialization RCE (S=5, RPN=25 — pior RPN categoria compute)
- FM-062: CAS/AC hash collision BLAKE3 (S=5, RPN=25)
- FM-100: DNS outage registrar/CF (S=5, RPN=10, já menciona "runbook" no campo CTRLs mas runbook não existe)
- FM-101: CF edge outage global (S=5, RPN=5)
- FM-156: Supply chain dep malicioso (S=5, RPN=25)
- FM-258: Insider data exfil via support tool (S=5, RPN=25)

O texto atual usa a cláusula de escape "a criar conforme WIs avançam" mas não tem tracking de quais WIs vão criar cada runbook, nem prazo. FM-007 (Deserialization RCE) em particular é diretamente explorable: é a superfície de ataque mais comum em APIs públicas e não tem runbook de resposta.

Impacto: se FM-007 for triggeredado via dep vulnerável, o oncall não tem procedimento documentado. "A criar" sem WI de tracking é anti-pattern do próprio framework.

Recomendação: criar WIs backlog para os 6 runbooks faltantes (prioridade: FM-007, FM-156, FM-258 primeiro por serem adversariais; FM-100/101 depois por terem dependência de vendor). Anotar cada FM faltante com `RB: (WI-XXX backlog)` no campo CTRLs.

---

### MEDIUM

---

**T-04: FM-253 CTRL column referencia RB-FM-303 em vez de RB-FM-253 — cross-reference incorreta**

`failure_modes.md §3.7 FM-253` (linha 178): "TLA+ INV-TenantIsolation + **RB-FM-303**".

RB-FM-253 existe em `specs/05_quality/runbooks/RB-FM-253-cross-tenant-read.md`. RB-FM-303 é o runbook de "AC Entry Aponta Para Blob de Outro Tenant" — um FM diferente (FM-303). FM-253 deveria referenciar RB-FM-253 (seu próprio runbook). Esta é uma cross-reference incorreta no campo mais crítico do pior FM do produto.

O próprio doc (`failure_modes.md §6.1`, linha 257) cita corretamente: "`RB-FM-253` (cross-tenant read) → **HIGHEST priority**", confirmando que RB-FM-253 é o runbook correto para FM-253.

Impacto: oncall triggeriando FM-253 via alert, consultando `failure_modes.md`, vê `RB-FM-303` e abre o runbook errado (AC cross-tenant em vez de cross-tenant read direct). Em SEV-1 com SLA 15min, 2 minutos de confusão são significativos.

Recomendação: `failure_modes.md §3.7 FM-253` CTRLs column: substituir `RB-FM-303` por `RB-FM-253`.

---

**T-05: key_management.md §7.2 tem 3 referências "a criar" para runbooks que foram criados no Lote 5.12 — stale**

`key_management.md` (linhas 208-210):
```
- `RB-KEY-COMPROMISE` (a criar em Lote 5.7).
- `RB-HSM-UNAVAILABLE` (a criar em Lote 5.7).
- `RB-BYOK-REVOKE` (a criar em Lote 5.7).
```

Todos os três runbooks foram criados no Lote 5.12 e existem em `specs/05_quality/runbooks/`. A referência "a criar" é stale e cria confusão desnecessária — sugere que o sistema de resposta a incidentes de chaves está incompleto quando não está.

Similarmente: `privacy_model.md §11.3` (linha 425): "`RB-BREACH-NOTIF` (a criar)" — RB-BREACH-NOTIF existe em `specs/05_quality/runbooks/RB-BREACH-NOTIF.md`. `storage_semantics_matrix.md §3.9` (linha 176): "`specs/tla/tenant_isolation.tla` (a criar)" — spec existe desde Lote 5.13.

Impacto: leitores de good faith assumem que features críticas de segurança não estão implementadas quando estão.

Recomendação: remover todas as 5 referências stale "a criar" nos 3 docs afetados e substituir por links diretos.

---

**T-06: SLO catalog tem 5 tiers na tier table (§3) mas per-SLO entries (§4.x) só definem targets para team/enterprise — solo e business ficam sem SLA formal**

`slo_catalog.md §3`: 5 tiers com availability alvo (free=99.5%, solo=99.7%, team=99.9%, business=99.93%, enterprise=99.95%).

`slo_catalog.md §4.x` per-SLO entries: apenas `Target (team)` e `Target (enterprise)`. Solo e business não têm targets definidos por-SLO. Isso significa que um cliente `solo` assina um produto com "SLO disponibilidade = 99.7% (da tier table)" mas sem nenhum SLO de latência, correção ou frescor definidos. O error budget para solo/business não pode ser calculado sem esses targets.

O S-13 fix adicionou as tiers à tabela §3 mas não propagou para os 13 SLOs individuais em §4.

Impacto: contrato com cliente `business` não tem SLA de latência documentado. Se um cliente `business` processar um SLA breach, a documentação só mostra 99.93% para availability — sem nada para latência. Lacuna contratual.

Recomendação: para cada SLO em §4.x, adicionar `Target (solo)` e `Target (business)` interpolando razoavelmente entre free/team e team/enterprise respectivamente. Alternativamente: explicitar que solo/business herdam targets de team e business herda de enterprise (com nota explícita).

---

**T-07: gc_correctness.tla modelo não tem "resurrection path" (soft_deleted → active) — cenário de re-upload pós-sweep não modelado**

`gc_correctness.tla` action `GCSweepBlob(b)` (linha 139): sets `blob_meta[b].state = "soft_deleted"`. Após isso, `UpdateActionResult(e, refs)` é bloqueado para `b` porque requer `blob_meta[b].state = "active"` (linha 100). O modelo não tem ação `ResurrectBlob(b)` que faz o caminho soft_deleted → active.

Na produção: quando um cliente tenta fazer `CAS::BatchUpdateBlobs` com um blob que foi soft-deleted, o sistema deve aceitar o re-upload (o blob existe, o digest é idempotente) e "ressuscitar" a entry — atualizando state para `active` e `last_referenced_at`. Este fluxo não é modelado no TLA+.

Consequência: se um bug introduz uma regressão que impede a ressurreição (e.g., o re-upload é ignorado quando `state != active`), o TLA+ não detectará. O invariante `InvGCReachableNeverDeleted` não cobre o cenário de "blob foi soft-deleted, re-referenciado via upload, mas não ressuscitado — cliente acredita que blob está no CAS mas está deletado".

Note: este cenário é diferente do S-12 (que cobria re-referência via AC update antes do sweep). Este é re-referência via re-upload pós-sweep.

Impacto: gap de coverage formal em cenário plausível de uso (re-upload após eviction). Pode resultar em cache miss silencioso se resurrection path tiver bug.

Recomendação: adicionar `ResurrectBlob(b)` action ao gc_correctness.tla que reverte `soft_deleted → active` quando cliente faz re-upload, e adicionar invariante que garante que blob re-referenciado (via AC update OR re-upload) nunca está em estado terminal se o CAS o aceita.

---

**T-08: tenant_isolation.tla path guessing bounds colapsam a proteção HMAC a 50% de taxa de acerto — modelo é mais fraco que a realidade**

`tenant_isolation.tla cfg` (linha 14): `Tenants = {t1, t2}`. Como `hmac_prefix \in [Tenants -> 1..Cardinality(Tenants)]`, o espaço de prefixos do atacante é `{1, 2}` — idêntico ao espaço de prefixos válidos.

`PathGuess(p, guessed_prefix, b)` (linha 162): `guessed_prefix \in 1..Cardinality(Tenants)`. Com 2 tenants, o atacante adivinhar o prefix correto tem probabilidade 1/2 = 50%.

Na produção, HMAC-SHA256 com 16 chars base64 = 96 bits de output. O espaço de guessing é 2^96. Probabilidade de acerto: 2^-96 (tratado como 0 no modelo real).

O modelo prova corretude para o caso 2-tenant, mas a abstração HMAC colapsada ao tamanho do conjunto de tenants significa que a resistência a path guessing não é exercitada com a mesma assimetria que na produção. O invariant `InvTenantIsolation` passa porque o AuthZ check (não o HMAC) bloqueia o read cross-tenant — mas o modelo não stressou o HMAC como barreira primária.

Impacto: a TLA+ proof é correta mas não exercita o escenário onde HMAC é a única barreira. Se o AuthZ check tiver um bug não-modelado, o HMAC seria a última linha de defesa — e o modelo não prova que HMAC é suficiente sozinho.

Recomendação: adicionar um cenário adversarial onde o `AuthZ check é bypassado` (modelar falha do middleware) e verificar que `InvNamespaceConsistency` ainda protege. Alternativamente, usar Apalache (symbolic model checking) para verificar propriedades com espaço de prefixos não-paramétrico.

---

**T-09: 48 CTRL orphans — volume excessivo de controles definidos mas nunca referenciados fora do canonical source**

`validate_references.py --warn-orphans` detecta 48 CTRLs definidos mas sem referência fora do canonical source. Exemplos: CTRL-ISO-002..005, CTRL-NET-001..005, CTRL-EXEC-001..003, CTRL-KEY-002..021, CTRL-PRIV-002, 004, 010..015, 020..022, 030..032.

Comparando com os 48 orphans: 18 são CTRL-KEY-* (novos; esperados orphans pois key_management.md é novo e FMs/runbooks ainda não citam esses CTRLs). 15 são CTRL-PRIV-* com numbers altos (010-032, muitos são controles específicos LINDDUN que FMs não referenciam diretamente). Porém: CTRL-ISO-002..005 (5 controles de tenant isolation) nunca são citados fora de security_model — eles são parte da "5 camadas de defesa" mas nenhum runbook, FM, ou compliance_matrix linha os cita explicitamente.

Impacto: não é um dangling reference (definidos mas não usados), mas indica que 48 controles existem sem cross-linkage para os FMs que deveriam mitigar e para as evidências que deveriam validar. Em uma auditoria SOC 2 Type II, o auditor vai pedir "mostre que CTRL-ISO-003 está sendo testado" — sem referência de FM ou runbook, a cadeia de evidência é fraca.

Recomendação: não é necessário referenciar cada CTRL em todo FM, mas os controles CRITICAL (CTRL-ISO-002..005, CTRL-EXEC-*, CTRL-FORMAL-002) deveriam aparecer em pelo menos um FM ou runbook para fechar o loop de verificação. Criar task de triage: verificar quais dos 48 orphans são genuinamente "not referenced by design" vs. esquecidos.

---

### LOW

---

**T-10: RB-BREACH-NOTIF.md linha 99: "placeholder@law-firm.com" — contato de Legal é placeholder em runbook de breach**

`RB-BREACH-NOTIF.md` (linha 99): "**Legal (TBD — outsource):** placeholder@law-firm.com".

Em uma situação de breach real, este runbook será aberto em emergência. "placeholder@law-firm.com" é um contato inválido. O runbook também tem 3 referências a templates legais "(a criar)": `legal/breach-notification-anpd-template.md`, `legal/breach-notification-dpc-template.md`, `legal/breach-notification-subject-template.md`.

Para compliance GDPR Art. 33 (72h notification), o runbook precisa de contatos reais antes de qualquer customer enterprise. É aceitável para estágio atual (pre-GA, sem clientes reais), mas precisa de ticket de rastreamento.

Recomendação: adicionar nota no runbook: "⚠️ Preencher antes do GA: contratos legais externos + templates DPC/ANPD". Criar WI de backlog com owner Privacy Officer.

---

**T-11: PAT-AUTHZ-002 é ORPHAN no validate_references (definido em resilience_patterns.md como alias, nunca usado em outros docs)**

`resilience_patterns.md §3.9` define PAT-AUTHZ-002 como alias de PAT-AUTHZ-001 para compatibilidade de audits. Porém, `validate_references.py --warn-orphans` identifica PAT-AUTHZ-002 como orphan (definido mas sem uso externo).

`slo_catalog.md §4.10` agora usa PAT-AUTHZ-001 (corretamente). O alias PAT-AUTHZ-002 existe mas não é referenciado por ninguém. A nota no resilience_patterns.md menciona "Referência mantida para compat dos audits Lote 3+4" — mas audits vivem em `_audits/` que é excluído do validator. Portanto o alias não tem uso real.

Impacto: baixo. Um orphan inofensivo mas que aumenta ruído no --warn-orphans e pode confundir futuros contribuidores.

Recomendação: se não há usos reais de PAT-AUTHZ-002, remover o entry de alias para reduzir ruído. Alternativamente, adicionar comentário ao slo_catalog mencionando o alias histórico.

---

**T-12: `cas_integrity.tla` usa `hash_fn` como VARIABLE em vez de CONSTANT — estruturalmente incomum e pode inflacionar state space**

`cas_integrity.tla` (linha 30): `hash_fn` declarado em `VARIABLES`. Todas as actions têm `UNCHANGED <<hash_fn, ...>>`. O Init o inicializa com uma função injetiva aleatória.

O efeito: TLC enumera TODOS os possíveis `hash_fn` injetivos como estados iniciais distintos. Com Bodies={b1,b2} e Digests={d1,d2,d_wrong}: 6 funções injetivas = 6 universos paralelos verificados. Isso é mais conservador que um CONSTANT hash mas também cria state space maior sem valor proporcional — `hash_fn` não varia durante a execução, então verificar invariantes sobre 6 hash functions distintas é redundante se as invariantes são agnósticas ao hash concreto (o que são).

Correto: declarar `hash_fn` como CONSTANT no cfg e simplificar o Init. O invariante InvCASIntegrity (`Hash(r2_storage[d]) = d`) vale para qualquer hash injetivo; verificar 6 instâncias é redundante.

Impacto: modelo funciona corretamente mas é mais lento que necessário e menos legível. Em state spaces maiores (quando bounds forem escalados), isso importa.

Recomendação: mover `hash_fn` para CONSTANTS no .cfg e remover da variável set. Ajustar Init para assumir hash_fn fixo. Este é refactor de performance/clareza, não de corretude.

---

## Veredito final

**CONDITIONAL — mais próximo de GREEN do que o audit anterior**

O Lote 5 representa uma resposta de alta qualidade aos 20 findings do audit anterior. Todos os S-01 a S-20 foram endereçados de forma substantiva — não teatro. Os críticos (S-01 EVT taxonomy, S-02 TLA+ obrigatório, S-03 retenção, S-04 SBOM ADR, S-06 HMAC R2 key) receberam fixes estruturais com ADRs, specs TLA+ reais, e anotações de rastreabilidade.

As TLA+ specs são trabalho genuíno: o código TLA+ está correto, os modelos são internamente consistentes, e os state files confirmam execução real pelo TLC (153 state files no último run). Os 3 specs cobrem os invariantes mais críticos do produto (tenant isolation, GC correctness, CAS integrity) com bounds apropriados para CI rápido e documentação clara das abstrações.

Os 3 bloqueadores desta iteração:

1. **T-01 (TODO.md plaintext tenant_id)**: spec diz HMAC, roadmap de implementação diz plaintext. Quem escrever o código de isolamento na semana 2 vai criar uma vulnerabilidade silenciosa seguindo o TODO.md. Fix é trivial mas crítico.

2. **T-02 (INV-AUDIT-APPEND-ONLY CRITICAL sem TLA+ e sem waiver)**: violação direta de CTRL-FORMAL-001. Para SOC 2 Type II, a cadeia "CRITICAL invariant → TLA+ obrigatório → CI falha se não verde" precisa ser fechada ou waivered formalmente.

3. **T-03 (6 P1 FMs sem runbook)**: a regra é clara no próprio doc. FM-007 (Deserialization RCE) e FM-156 (Supply chain) em particular têm superfície de ataque adversarial real e precisam de runbook antes de qualquer customer enterprise.

Sem os 3 fixes acima, o Lote 5 está em CONDITIONAL. Com eles, pode ser considerado GREEN para fins de spec (excluindo o fato de que todo o código real ainda não existe, que é uma limitação esperada do processo spec-first).

---

## Observações meta

**Pontos fortes do Lote 5:**

- O `invariant_registry.md` é excelente: naming canonical, TLA+ coverage matrix, alias table histórica, severity classification — é o tipo de artefato que auditores SOC 2 adoram ver. Resolve sistematicamente o problema de naming drift que aflige projetos de longa duração.
- O `gc_correctness.tla` modela corretamente o race S-12 com `mark_started_at` — é a parte mais difícil de acertar em TLA+ para sistemas de GC distribuídos. O `CanSweep` predicate com o check de INV-GC-004 é elegante.
- O `key_management.md` com hierarquia completa (Root HSM → KEK-GLOBAL → TDK → DEK), BYOK, lifecycle states, e CTRL-KEY-001..021 é genuinamente SOTA para um produto pre-GA. Coloca CoreLink à frente de BuildBuddy Enterprise nesse aspecto.
- O validate_references.py é bem implementado: regex corretos, definition anchors múltiplos, whitelist documentada, orphan detection via `--warn-orphans`. O fato de ter zero dangling references em 38 docs + 87 CTRLs + 51 PATs é real — não mágica da whitelist (a whitelist só cobre placeholders e aliases legítimos, não mascaramento de erros reais).
- As 3 ADRs em FROZEN com processo correto (título → contexto → decisão → consequências) são referência de como fazer ADRs.
- O `RB-FM-253` (cross-tenant read) e `RB-KEY-COMPROMISE` são runbooks de qualidade: SLAs definidos, steps sequenciados por fase temporal, forensics explícito, post-mortem obrigatório.

**Escopos não avaliados:**

- TLC não foi executado localmente (tooling ausente no ambiente). A claim de "119k / 21k / 3k states" e "verde" é baseada nos state files existentes e no código TLA+ ser sintaticamente e logicamente correto. Não é possível confirmar independentemente a ausência de violations sem rodar TLC.
- `legal/dpa/v1.md`, `legal/breach-notification-*-template.md`: ainda marcados "a criar" — dependências legais fora do escopo técnico.
- `scripts/validate_evidence.py` não existe ainda — o validator de EVT seria a verificação end-to-end da taxonomia EVT corrigida no S-01.
- Código Rust em `src/` tem apenas `main.rs` (stub). A spec está significativamente à frente do código, o que é o objetivo do processo spec-first, mas significa que nenhuma invariante foi verificada em código real.
- ISO 27001 SoA (`iso27001-soa.csv`, 46 linhas) parece completo para Annex A mas não foi auditado em profundidade — requer especialista ISO 27001 certificado para validação final.
- STRIDE-CTRL matrix CSV existe e tem conteúdo real (não placeholder). Não foi auditado entry por entry.

**Gap SOTA restante:**

NativeLink e Buildbarn têm implementações open-source com anos de battle-testing. CoreLink tem specs melhores, mas zero código de produção. O gap SOTA para CoreLink não é conceitual (a arquitetura é sólida) — é operacional: sem métricas de build reais, sem customers, sem 30 dias de baseline para os SLOs que acabaram de ser prometidos. Isso é esperado para um produto spec-first v0.5.0, mas é o gap real.
