---
id: AUDIT-LOTE3-4-SONNET
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-24
reviewers: [Sonnet 4.6 via Agent tool]
supersedes: null
superseded_by: null
tags: [audit, lote3, lote4]
---

# Audit Lote 3 + Lote 4 (Sonnet)

## Escopo revisado

**Lote 3 — templates** (commit c257c7c):
- `specs/_templates/work_item.md`
- `specs/_templates/sprint_contract.md`
- `specs/_templates/subtask.md`
- `specs/_templates/adr.md`
- `specs/_templates/production_readiness_review.md`
- `specs/_templates/waiver.md`

**Lote 4 — canonical sources** (commits 171d279 + 14063b2):
- `specs/03_architecture/storage_semantics_matrix.md`
- `specs/03_architecture/auth_model.md`
- `specs/03_architecture/remote_cache_product_profile.md`
- `specs/03_architecture/security_model.md`
- `specs/03_architecture/observability_model.md`
- `specs/03_architecture/privacy_model.md`
- `specs/03_architecture/failure_modes.md`
- `specs/03_architecture/resilience_patterns.md`
- `specs/03_architecture/slo_catalog.md`
- `specs/03_architecture/compliance_matrix.md`
- `specs/03_architecture/data_model.md`

**Validação de schema:** todos 11 canonical sources passam `validate_specs.py --verbose` (schema OK).

---

## Findings

### CRITICAL (bloqueia GA)

---

**S-01: EVT taxonomy violation sistêmica — nenhum canonical source usa os IDs canônicos EVT-NNN**

Os 24 tipos canônicos de evidence estão definidos no framework `00_framework.md §35.7` como `EVT-001` a `EVT-024` com nomes específicos (`CI_LOG`, `TEST_OUTPUT`, `SAST_SCAN_REPORT`, `CHAOS_EXPERIMENT_REPORT`, `LOAD_TEST_REPORT`, `RUNBOOK_EXECUTION`, etc.). A regra do framework (§35.7, citada em `work_item.md §10`) é explícita: toda caixa de Completeness/DoD **DEVE** referenciar ao menos 1 evidence artifact tipado conforme §35.7.1.

Os 11 canonical sources do Lote 4 **ignoram completamente essa taxonomia** e inventam aliases não-canônicos:

| Alias usado nos docs | Canônico correto | Docs afetados |
|---|---|---|
| `EVT-UNIT_TEST_PASS` | `EVT-002 TEST_OUTPUT` | security_model, resilience_patterns, compliance_matrix |
| `EVT-INTEGRATION_TEST_PASS` | `EVT-002 TEST_OUTPUT` | privacy_model, data_model, resilience_patterns |
| `EVT-PENTEST_REPORT` | Não existe (DAST = `EVT-006`; externo = `EVT-021`) | security_model, compliance_matrix |
| `EVT-SCHEMA_VALIDATION` | Não existe | compliance_matrix, observability_model |
| `EVT-SLSA_PROVENANCE` | `EVT-011 BINARY_SIGNATURE` | security_model, compliance_matrix |
| `EVT-RUNBOOK_VALIDATION` | `EVT-017 RUNBOOK_EXECUTION` | security_model, privacy_model, compliance_matrix |
| `EVT-MONITORING_REPORT` | Não existe | observability_model:429 |
| `EVT-SAST_SCAN` | `EVT-005 SAST_SCAN_REPORT` | security_model, compliance_matrix |
| `EVT-CHAOS_REPORT` | `EVT-023 CHAOS_EXPERIMENT_REPORT` | security_model, resilience_patterns |
| `EVT-LOAD_TEST` | `EVT-024 LOAD_TEST_REPORT` | resilience_patterns, slo_catalog |
| `EVT-CLIENT_CONFORMANCE_TEST` | Não existe | security_model |
| `EVT-CONFIG_SNAPSHOT` | Não existe | security_model |
| `EVT-ADR_DECISION` | Não existe | security_model |
| `EVT-BUG_BOUNTY_REPORT` | Não existe | security_model |
| `EVT-POLICY_SIGN` | Não existe | compliance_matrix:78 |
| `EVT-TRAINING_RECORD` | Não existe | compliance_matrix:79 |
| `EVT-AUDIT_PLAN` | Não existe | compliance_matrix:81 |
| `EVT-ACCESS_REVIEW` | Não existe | compliance_matrix:86 |
| `EVT-NETWORK_DIAGRAM` | Não existe | compliance_matrix:87 |
| `EVT-SCAN_REPORT` | Não existe | compliance_matrix:88 |
| `EVT-DEPLOY_LOG` | Não existe | compliance_matrix:91 |
| `EVT-WAIVER_ACTIVE` | Não existe | compliance_matrix:92 |
| `EVT-VENDOR_REVIEW` | Não existe | compliance_matrix:93 |
| `EVT-DR_DRILL` | Não existe | compliance_matrix:99 |
| `EVT-ERASURE_TEST` | Não existe | compliance_matrix:103, 115 |
| `EVT-DATA_CLASSIFICATION_DOC` | Não existe | compliance_matrix:103 |
| `EVT-LEGAL_REVIEW` | Não existe | compliance_matrix:112 |
| `EVT-AUDIT_LOG` | Não existe como EVT | compliance_matrix:113 |
| `EVT-DPIA` | Não existe | compliance_matrix:115, privacy_model:422 |

**Impacto:** o validator de evidence (`scripts/validate_evidence.py`, previsto no framework §35.7) vai rejeitar todos estes registros quando for implementado. Mais grave: qualquer auditoria SOC 2 que use esses IDs como referência vai encontrar "evidence type não catalogado" — invalidando a cadeia de evidência. O compliance_matrix em particular é o ponto de mapeamento para SOC 2 Type II, tornando esse finding crítico para a certificação.

**Recomendação:** criar um mapeamento oficial ou (a) renomear os aliases para os IDs numéricos canônicos, ou (b) expandir o framework §35.7 com os tipos adicionais necessários via ADR antes de qualquer uso. Opção (b) é mais realista dado o número de tipos extras, mas requer ADR + bump minor no framework.

---

**S-02: INV-TenantIsolation TLA+ declarada "opcional" em dois docs e "obrigatória" em três — contradição direta sobre invariante CRITICAL**

`auth_model.md §8.3` (linha 384): `"INV-TenantIsolation pode ter spec formal TLA+ (§13 do framework)"` — usa linguagem permissiva ("pode").

`storage_semantics_matrix.md §3.9` (linha 174): `"Verificado via property test (§13 TLA+ opcional)"` — explicitamente "opcional".

Contra-evidência:
- `security_model.md §3 TB-3` (linha 116): `"INV-TenantIsolation (TLA+ obrigatório)"`.
- `security_model.md §6.9 CTRL-FORMAL-001` (linha 312): `"CI falha se invariante CRITICAL não tem model check verde"`.
- `data_model.md §7` (linha 359): `"CRITICAL invariants requerem TLA+ model (CTRL-FORMAL-001)"`, seguido de INV-DATA-TENANT-ISOLATION na lista.
- `remote_cache_product_profile.md §15` (linha 469): `INV-TenantIsolation | CRITICAL | FF-HR-002 | Property test + TLA+`.

**Impacto:** a invariante mais crítica do produto (core do business de multi-tenant cache) tem status contraditório. Se uma equipe citar `auth_model.md §8.3` ou `storage_semantics_matrix.md §3.9` como justificativa para não implementar o TLA+ check, estarão violando o framework mas com documentação de respaldo. Isso cria ambiguidade fatal em auditorias e PRRs.

**Recomendação:** corrigir `auth_model.md §8.3` para remover "opcional" e alinhar com a linguagem normativa de CTRL-FORMAL-001. Corrigir `storage_semantics_matrix.md §3.9`. Ambas as mudanças requerem bump patch.

---

**S-03: Retenção de audit log contraditória — auth_model diz 2 anos, todos os outros dizem 7 anos**

`auth_model.md §7.2` (linha 341): `"audit_log retido por 2 anos no D1/Neon + archived em R2 com retenção permanente"`.

Contradições diretas:
- `security_model.md §1` (linha 64): `"retention ≥ 7 anos para SOC 2"`.
- `security_model.md §2 AST-AUDIT` (linha 80): `"Retention 7 anos"`.
- `security_model.md §6.8 CTRL-AUDIT-005` (linha 306): `"Retention 7 anos (SOC 2)"`.
- `data_model.md §5.3` (linha 315): `"Object Lock Governance Mode 7y"`.
- `compliance_matrix.md §8.2` (linha 258): `"EVT-AUDIT_LOG retention 7y"`.
- `privacy_model.md §2` (linha 86): `"Audit log | 7y (SOC 2)"`.

**Impacto:** o `auth_model.md` é o canonical source da identidade. Um eng implementando o audit trail de autenticação (login, revogação, escalation) vai encontrar `auth_model.md §7.2` e configurar retenção de 2 anos, violando SOC 2 (que exige 1+ anos com evidência) e potencialmente LGPD Art. 37. A discrepância de "hot" (D1, 2 anos) vs "cold" (R2, permanente) não está clara o suficiente — "permanente" contradiz "7 anos" de outros docs em contexto de legal hold.

**Recomendação:** corrigir `auth_model.md §7.2` para alinhar: hot tier em D1/Neon pode ser menor (ex: 90d a 1 ano operacional), mas o archive no R2 deve ser explicitamente "≥ 7 anos" conforme SOC 2, não "permanente" (que é ambíguo). Adicionar nota explicando a diferença entre hot path e archive tier.

---

**S-04: SBOM format inconsistente — security_model diz CycloneDX 1.5, framework diz SPDX 2.3+**

`security_model.md §8.2` (linha 376): `"Formato: CycloneDX 1.5 (JSON). Gerado via cargo-cyclonedx."`.

`security_model.md §6.4 CTRL-SUPPLY-003` (linha 267): `"CycloneDX gerado em build"`.

Contra-evidência:
- `00_framework.md §35.7 EVT-010` (linha 2644): `"SBOM | Software Bill of Materials | SPDX 2.3+ JSON/YAML"`.
- `00_framework.md §1767` (menção implícita): `"formato SPDX"`.

**Impacto:** a evidence taxonomy canônica do framework define SBOM como SPDX 2.3+, mas o canonical source de segurança implementa CycloneDX 1.5. Quando o `validate_evidence.py` for implementado e verificar o formato do EVT-010, vai rejeitar o SBOM CycloneDX como não-conforme. Adicionalmente, ferramentas de compliance (Dependency-Track, NTIA SBOM minimum elements) aceitam ambos, mas a inconsistência entre docs vai gerar confusão operacional.

**Recomendação:** decidir via ADR qual formato é canônico. CycloneDX 1.5 é provavelmente a escolha certa para o ecossistema Rust (cargo-cyclonedx existe; cargo-spdx é menos maduro). Se CycloneDX for escolhido, atualizar `00_framework.md §35.7 EVT-010` com aprovação Architect + Security Lead.

---

### HIGH (fix antes do próximo lote)

---

**S-05: 20 PAT-XXX referenciados em failure_modes.md não existem em resilience_patterns.md**

`failure_modes.md` referencia os seguintes PAT-IDs que não têm entry correspondente em `resilience_patterns.md`:

```
PAT-ABUSE-DETECT-001      (FM-255)
PAT-BACKOFF-001           (FM-150)
PAT-DNS-TTL-001           (FM-100)
PAT-DUAL-APPROVAL         (FM-201, FM-205) — definido como PAT-DUAL-APPROVAL-001
PAT-ERROR-ISOLATE-001     (FM-006)
PAT-GC-HEALTHCHECK-001    (FM-305)
PAT-INPUT-HARDEN-001      (FM-007)
PAT-KV-TTL-001            (FM-054)
PAT-MEMORY-001            (FM-002)
PAT-MONOTONIC-001         (FM-350)
PAT-ONLINE-MIGRATE-001    (FM-056)
PAT-PATCH-SLA             (FM-155)
PAT-QUEUE-EVENTS-001      (FM-151)
PAT-READ-YOUR-WRITES-001  (FM-052)
PAT-ROLL-FORWARD          (FM-204) — definido como PAT-ROLL-FORWARD-001
PAT-RUNBOOK-DRILL         (FM-202) — inexistente
PAT-SESSION-CONSISTENCY-001 (FM-055)
PAT-SWEEPER-001           (FM-060)
PAT-TIMEOUT-002           (FM-004)
PAT-TTL-JITTER-001        (FM-352)
```

Dois casos específicos de nome truncado: `failure_modes.md` usa `PAT-DUAL-APPROVAL` (sem `-001`) enquanto `resilience_patterns.md` define `PAT-DUAL-APPROVAL-001`. Idem para `PAT-ROLL-FORWARD` vs `PAT-ROLL-FORWARD-001`. Esse não é apenas cosmético — um validator cross-reference vai reportar miss.

**Impacto:** `failure_modes.md §5` afirma: "Referências a IDs PAT-XXX serão formalizadas em `resilience_patterns.md`". Metade dos IDs referenciados em FMs nunca foram formalizados. Um WI que herda de FAILURE-MODES e tenta implementar mitigação de FM-006 (Panic em Rust não capturado) encontra `PAT-ERROR-ISOLATE-001` que não existe no catálogo canônico — forçando redefinição ad-hoc, exatamente o anti-pattern que `resilience_patterns.md` proíbe.

**Recomendação:** para cada PAT ausente, criar entry mínima em `resilience_patterns.md §3` com: problema, solução, FMs mitigados, evidence. Para os casos de nome truncado (PAT-DUAL-APPROVAL, PAT-ROLL-FORWARD), padronizar com `-001` suffix no `failure_modes.md`.

---

**S-06: R2 key format inconsistente em 3 canonical sources — isolamento de tenant em risco**

Três documentos descrevem o mesmo R2 key format com estruturas diferentes:

`data_model.md §5.1` (linhas 281-283): key = `<hmac(tenant_key, tenant_id)[:16]>/<algo>/<hex[0:2]>/<hex[2:4]>/<hex>`.

`remote_cache_product_profile.md §7.1` (linha 239): key = `cas/{tenant_id}/blake3/{digest}` — usa `tenant_id` plaintext, sem HMAC.

`storage_semantics_matrix.md §3.9` (linha 168): key = `cas/{tenant_id}/{digest}` — usa `tenant_id` plaintext, sem HMAC e sem `digest_fn`.

`security_model.md §3 TB-3` (linha 114): `"Prefixo de path derivado via HMAC(tenant_key, tenant_id)"`.

**Impacto:** a diferença entre HMAC e plaintext `tenant_id` é a diferença entre ter e não ter o controle `CTRL-ISO-001` (HMAC tenant prefix) operacional. Se a implementação seguir `remote_cache_product_profile.md §7.1` (que define o canonical source de CAS/AC/GC), o tenant prefix será plaintext — qualquer R2 bucket policy incorreta ou ausente resultaria em cross-tenant read direto via path guessing. O claim de segurança em `security_model.md §6.3 CTRL-ISO-001` depende do HMAC existir.

**Recomendação:** escolher um formato canonical único em ADR. O mais seguro é o de `data_model.md §5.1` (com HMAC). Atualizar `remote_cache_product_profile.md §7.1` e `storage_semantics_matrix.md §3.9` para refletir o formato com HMAC. `REG-NAMESPACE-001` em `remote_cache_product_profile.md` também precisa ser atualizado.

---

**S-07: INV naming inconsistente entre canonical sources — mesmo invariante com dois nomes**

`security_model.md §1` (linha 64): invariante de audit log = `INV-AUDIT-APPEND-ONLY`.

`storage_semantics_matrix.md §3.8` (linha 162) e `remote_cache_product_profile.md §15` (linha 470): mesma invariante = `INV-AuditLogImmutability`.

`data_model.md §7` (linha 354): `INV-DATA-AUDIT-CHAIN` (terceiro nome para conceito adjacente).

Idem para tenant isolation:
- `security_model.md` e maioria: `INV-TenantIsolation`
- `data_model.md §7` (linha 353): `INV-DATA-TENANT-ISOLATION`

Quatro invariantes mencionadas apenas em `security_model.md §1` sem aparecer em nenhum outro doc: `INV-CONF-AT-REST`, `INV-CONF-IN-FLIGHT`, `INV-AVAIL-ISOLATION`, `INV-CAS-INTEGRITY`. Nenhuma destas aparece em `data_model.md §7` (catálogo de invariantes), criando coverage gap.

**Impacto:** TLA+ specs precisam de nomes canônicos. Se houver dois nomes para o mesmo invariante, haverá dois `.tla` files ou um file que referencia um nome que ninguém mais usa. PRRs que checam `INV-AUDIT-APPEND-ONLY` não vão bater com specs que referenciam `INV-AuditLogImmutability`. Adicionalmente, `INV-CONF-AT-REST` e `INV-CONF-IN-FLIGHT` são declaradas como invariantes de um sistema multi-tenant mas não têm definição formal, owner, ou enforcement documentados.

**Recomendação:** criar um arquivo de referência `specs/03_architecture/invariant_registry.md` (ou seção em `data_model.md §7`) com todos os INV-IDs canônicos, descrição, enforcement, e status TLA+. Escolher um nome canônico por invariante. O prefixo `INV-DATA-` de `data_model.md` parece mais discriminante — mas precisa de ADR.

---

**S-08: SLO-AVAIL-CAS-GET enterprise target 99.99% contradiz tier table que diz 99.95%**

`slo_catalog.md §3` (linha 83): tier enterprise → availability alvo = `99.95% (21m/mo)`.

`slo_catalog.md §4.2 SLO-AVAIL-CAS-GET` (linha 114): `"Target (enterprise): 99.99%"`.

**Quatro noves** (99.99%) implica budget de 4.38 minutos/mês. Em infraestrutura Cloudflare sem SLA próprio de 99.99% documentado para Workers + R2 combinados, esse target é operacionalmente irreal para um produto em fase 0.1.0. Para referência, BuildBuddy Enterprise publica 99.9% para CAS. JFrog Artifactory Cloud garante 99.9% no tier enterprise.

A inconsistência entre §3 (99.95%) e §4.2 (99.99%) é direta: um enterprise customer assinando contrato baseado na tier table vai ter SLA diferente do que o SLO-AVAIL-CAS-GET promete ao oncall.

**Impacto:** (a) contradição contratual entre §3 e §4.2; (b) target 99.99% irreal sem evidence de baseline — `slo_catalog.md §1.3` exige "baseline medido ≥ 30 dias" para propor SLO. Nenhum baseline existe.

**Recomendação:** alinhar §4.2 com §3 (99.95% enterprise). Adicionar nota explícita que 99.99% pode ser aspiracional pós-GA com evidência de baseline mínima de 30 dias. Aplicar mesma regra às demais inconsistências menores (SLO-AVAIL-CP enterprise é 99.95% alinhado, mas a CAS GET enterprise quebra o padrão sem justificativa).

---

**S-09: FM-051 P1 com RPN=20 viola a regra de classificação do próprio documento**

`failure_modes.md §1` (linha 71): regra explícita: `"30 ≤ RPN < 60 → P1"` e `"RPN < 30 → P2"`.

`failure_modes.md §3.2 FM-051` (linha 111): S=5, O=1, D=4, RPN=20, classificado `P1` sem justificativa de override.

Contraste com FM-007 (linha 104): S=5, O=1, D=5, RPN=25, classificado `P2 (mas classe S=5 força P1)` — com justificativa explícita do override. FM-062 (linha 122) e FM-156 (linha 146) usam a mesma nota explícita.

**FM-051 viola a regra e não explica o override.** Se existe uma regra implícita `"S=5 → upgrade para P1"`, ela precisa ser documentada em §1 e aplicada consistentemente. Atualmente:
- FM-006 (S=4, RPN=16) → P2 (S=4 não força upgrade)
- FM-050 (S=4, RPN=16) → P2
- FM-051 (S=5, RPN=20) → P1 (sem nota de justificativa)
- FM-007 (S=5, RPN=25) → P2 (S=5 → P1 com nota)

A inconsistência sugere que FM-051 recebeu P1 manualmente sem aplicar o processo documentado.

**Recomendação:** ou (a) adicionar nota `"(S=5 → P1)"` em FM-051 para alinhar com o padrão dos outros FMs com override, ou (b) documentar explicitamente em §1 a regra `"S=5 implica P1 minimum"` e aplicar retroativamente a FM-006 (S=4 não afeta) e FM-050.

---

**S-10: CTRL-META-001, CTRL-GC-001, CTRL-CRED-001, CTRL-NET-001..004 referenciados em STRIDE mas não definidos no catálogo**

`security_model.md §5 STRIDE` referencia os seguintes CTRLs que **não aparecem em nenhuma seção §6.X** do catálogo:

| CTRL referenciado | Onde usado | Definição no catálogo |
|---|---|---|
| `CTRL-META-001` | THR-T-003 (linha 175) | ❌ ausente |
| `CTRL-GC-001` | THR-T-003 (linha 175) | ❌ ausente |
| `CTRL-CRED-001` | THR-I-006 (linha 198) | ❌ ausente |
| `CTRL-NET-001` | THR-S-003 (linha 165) | ❌ ausente |
| `CTRL-NET-002` | THR-S-003 (linha 165) | ❌ ausente |
| `CTRL-NET-003` | THR-S-004 (linha 166) | ❌ ausente |
| `CTRL-NET-004` | THR-I-005 (linha 197) | ❌ ausente |

`CTRL-NET-005` (linha 287) está definido em §6.6 mas CTRL-NET-001..004 não. Para CTRL-CRED-001 e CTRL-META-001, não existe seção §6.X de Credenciais ou Metadata.

**Impacto:** `security_model.md §11` afirma que a matriz STRIDE → CTRL é "completa em `_audits/matrix-stride-ctrl.csv`" (que não existe). As 7 ameaças cujos CTRLs são dangling ficam sem implementação verificável, evidence obrigatória, ou revalidação. THR-T-003 (Metadata tamper → GC deleta blob vivo) é particularmente crítico pois o CTRL-META-001 prometido nunca foi definido.

**Recomendação:** criar seções §6.11 Credentials e §6.12 Network Perimeter (CTRL-NET-001..004, CTRL-CRED-001) e §6.13 Data Integrity (CTRL-META-001, CTRL-GC-001). Para cada CTRL, seguir o mesmo template: implementação, evidence, revalidação. Alternativamente, mapear para CTRLs existentes se houver sobreposição (ex: CTRL-CRED-001 pode mapear para CTRL-PRIV-001 + CTRL-INPUT-001).

---

**S-11: sprint_contract.md e production_readiness_review.md sem lane-aware sign-offs**

`work_item.md §30` tem tabela de sign-off explicitamente anotada com `LOW_RISK / STANDARD / HIGH_RISK` com mínimos por lane (3 / 5-8 / 10-12 roles), conforme `framework §33.5.4.3`.

`sprint_contract.md §20` (linha 866-878): tabela de sign-off fixa com 7 roles sem qualquer referência a lane. Um sprint `HIGH_RISK` (com forcing factors FF-HR-002, FF-HR-005, etc.) tem exatamente o mesmo processo de sign-off que um sprint `LOW_RISK`. O campo `lane: "STANDARD"` existe no YAML header (linha 7) mas é puramente declarativo — sem efeito na cerimônia.

`production_readiness_review.md`: **não tem campo `lane`** no YAML header e **nenhuma referência** a lane, forcing factors, ou diferenciação por risk level. Uma PRR para deploy de feature que toca INV-TenantIsolation (HIGH_RISK, FF-HR-002) tem exatamente o mesmo processo que uma PRR de update de documentação.

**Impacto:** a lógica de "reduzir cerimônia em LOW_RISK, ampliar em HIGH_RISK" (core do framework Lote 2) não se propaga para sprint e PRR. Dado que o Lote 3 era especificamente "aplicar inheritance + lane annotations aos templates", esse é um gap direto no objetivo do lote.

**Recomendação:** (a) adicionar campo `lane` + `lane_forcing_factors` ao YAML header de `production_readiness_review.md`; (b) adicionar nota na §20 de `sprint_contract.md` e sign-off de PRR com lane-conditional requirements análogos ao `work_item.md §30`.

---

### MEDIUM (backlog explícito)

---

**S-12: GC algorithm — INV-GC-004 não cobre o race de re-referência via AC entry mid-GC**

`remote_cache_product_profile.md §9.4 INV-GC-004` (linha 328): `"Race entre upload e GC é safe: concurrent upload completa FIRST → D1 insert → GC sweep então enxerga reachable."` O argumento é: novo blob tem age < 24h → grace period protege.

Cenário não coberto: blob B tem 48h de idade (já passou do grace). Foi recentemente *re-referenciado* via novo AC entry (`UpdateActionResult` que aponta para B nos outputs). Se:
1. GC Phase 1 (Mark) roda → B não está no set M (AC entry velha tinha sido evicted ou B estava temporariamente órfão)
2. `UpdateActionResult` é escrito no D1 após Phase 1 → agora B é reachable
3. GC Phase 2 (Sweep) roda → B age > 24h, NOT in M → **B é deletado**

INV-GC-001 é violado sem que a grace period ajude. Isso não é coberto por INV-GC-004 que só fala de "upload" (novo blob). O algoritmo pressupõe snapshot atômico do D1 durante Mark, mas D1 com Sessions API só garante consistência sequencial por sessão, não snapshot global.

**Impacto:** se o GC rodar diariamente (REG-GC-001) enquanto clientes estão ativamente fazendo builds, a janela de vulnerabilidade é a duração do Phase 1 scan (potencialmente minutos para 500M blobs estimados). Qualquer blob re-referenciado durante esse window pode ser deletado, causando cache misses forçados (blocos de build falham ao tentar baixar output intermediário).

**Recomendação:** o TLA+ spec obrigatório para INV-GC-001 deve modelar explicitamente a concorrência entre GC Mark phase e UpdateActionResult. Como mitigação operacional interim: GC Phase 2 deve usar um timestamp `mark_started_at` e aplicar grace period adicional: qualquer blob cujo AC entry foi criado após `mark_started_at` deve ser preservado (ou manter grade period de 48h em vez de 24h).

---

**S-13: Produto tem 4 tiers em remote_cache_product_profile mas 3 em slo_catalog — "Solo" e "Business" são fantasmas**

`remote_cache_product_profile.md §8.2` (linhas 268-272): define 5 tiers: `Free`, `Solo`, `Team`, `Business`, `Enterprise`.

`slo_catalog.md §3` (linhas 81-83): define apenas 3 tiers: `free`, `team`, `enterprise`.

`work_item.md §0 Tier` (linha 153): lista `Free | Solo | Team | Business | Enterprise | Todos` (5 tiers).

**Impacto:** `Solo` (100 GB, AC TTL 30d) e `Business` (2 TB, AC TTL 365d) têm eviction policies definidas mas sem SLOs, sem SLIs, e sem error budget. Um cliente `Business` assinando contrato não tem SLA documentado em nenhum canonical source. A inconsistência é tanto de produto (quantos tiers existem?) quanto operacional (qual o SLO de um tenant Business quando o cache está degradado?).

**Recomendação:** alinhar o tier model em ADR. Se existem 5 tiers, `slo_catalog.md §3` precisa documentar os SLOs dos tiers `solo` e `business`. Se existem apenas 3 tiers para GA, `remote_cache_product_profile.md §8.2` deve ser simplificado.

---

**S-14: CTRL-ISO-001 não tem evidence nem revalidação definidos**

`security_model.md §6.3` (linha 255):
```
| CTRL-ISO-001 | HMAC tenant prefix | Ver CTRL-AUTH-004 | — | — |
```

Os campos Evidence e Revalidação estão explicitamente em branco (`—`). O §6 introdutório define: "Cada CTRL **DEVE** ter: descrição, implementação, owner (time), evidência obrigatória, teste de conformidade, frequência de revalidação."

`CTRL-ISO-001` é o controle de primeira camada do tenant isolation (o HMAC prefix que impede path guessing). É citado em THR-I-001 (cross-tenant read, o pior cenário), e é parte da cadeia `CTRL-ISO-001..005` que protege INV-TenantIsolation. Um controle CRITICAL sem evidence nem revalidação é um control que nunca é verificado.

**Recomendação:** definir: evidence = `EVT-002 TEST_OUTPUT` (property test que verifica que dois tenants distintos produzem prefixos distintos via HMAC), revalidação = `Semestral (key rotation)` (alinhado com CTRL-AUTH-004). Owner = Security Lead.

---

**S-15: FF-HR-011 proposto em remote_cache_product_profile.md mas não existe no framework**

`remote_cache_product_profile.md §15` (linha 471): `"INV-GC-001 (reachable never deleted) | CRITICAL | (novo FF-HR-011 proposto)"`.

O framework `00_framework.md §33.5` define 10 forcing factors (`FF-HR-001` a `FF-HR-010`). `FF-HR-011` não existe. Um canonical source não pode propor novos forcing factors unilateralmente — isso requer ADR + bump minor no framework (conforme processo do Lote 2).

**Impacto:** qualquer WI que toque GC e use `lane_forcing_factors: ["FF-HR-011"]` vai falhar validação de schema (o schema valida pattern `^FF-HR-\\d{3}$` mas pode ter enum validation). Mais importante: o forcing factor inexistente não escala para HIGH_RISK automaticamente — o WI pode ser tratado como STANDARD sem que o autor saiba que INV-GC-001 exige HIGH_RISK.

**Recomendação:** criar ADR propondo `FF-HR-011: Modifica algoritmo de GC ou invariante de reachability com blast radius cross-tenant` e atualizar framework. Até lá, usar `FF-HR-002` (modifica tenant isolation) ou `FF-HR-005` (modifica contrato de durabilidade).

---

**S-16: PAT-AUTHZ-002 referenciado em slo_catalog.md não existe em resilience_patterns.md**

`slo_catalog.md §4.10 SLO-CORRECT-ISO` (linha 203): `"Fonte: Assertions de tenant_id em toda storage call (PAT-AUTHZ-002)"`.

`resilience_patterns.md`: nenhuma entry para `PAT-AUTHZ-002`. O catálogo tem `CTRL-AUTHZ-001` e `CTRL-AUTHZ-002` em `security_model.md`, mas nenhum `PAT-AUTHZ-002` (padrão de resiliência, não controle de segurança).

**Impacto:** dangling reference no SLO da invariante de isolamento mais crítica. O SLI `corelink_isolation_assertion_total` depende de um padrão de implementação que não está formalizado.

**Recomendação:** ou (a) criar `PAT-AUTHZ-001` em `resilience_patterns.md §3` descrevendo o padrão de assertion dupla de `tenant_id` em toda storage call (problema, solução, FMs mitigados: FM-253, FM-303), ou (b) substituir a referência pelo CTRL correto (`CTRL-AUTHZ-002`).

---

### LOW / POLISH

---

**S-17: compliance_matrix.md usa tabelas de markdown inválidas para §2.3 e §2.4 (linhas faltando cabeçalho)**

`compliance_matrix.md §2.3` (linhas 97-99) e §2.4 (linhas 103-104): as rows da tabela aparecem sem que a tabela tenha sido explicitamente "aberta" com cabeçalho + separador. O Markdown renderiza como texto corrido em alguns parsers, não como tabela. Isso pode causar falha em ferramentas de extração automática de compliance evidence.

**Recomendação:** adicionar header row `| Criterion | Requisito | CTRLs | Evidence |` + separador `|---|---|---|---|` antes de cada grupo de linhas em §2.3, §2.4, §2.5, §3.3.

---

**S-18: storage_semantics_matrix.md §3.8 declara audit log retention de "2 anos via scheduled purge" para D1 — inconsistência com todos os outros docs (já coberto em S-03, mas este doc em específico também diz que o purge é feito via "cron DO" — sem R2 archive mencionado)**

`storage_semantics_matrix.md §3.8` (linha 160): `"Retention | 2 anos via scheduled purge | Implementado via cron DO"`.

Não menciona o archive em R2 `audit-<region>` com Object Lock 7 anos. Dá a impressão de que os audit events do sistema operacional (não apenas auth) são purgados após 2 anos sem archive. Essa inconsistência é menos severa que S-03 mas cria o mesmo risco de implementação.

**Recomendação:** atualizar para explicitar o pipeline completo: `"hot: D1 90d → warm: R2 logpush → cold: R2 Object Lock 7y"`.

---

**S-19: "Zero findings" de P0 no FMEA é suspeito dado o tamanho da superfície**

O threshold de P0 (`RPN ≥ 60 AND S ≥ 4`) não é atingido por nenhum dos 52 FMs catalogados. O RPN mais alto é FM-300 (GC refcount bug, S=5, O=2, D=4, RPN=40) — ainda abaixo de 60.

Isso significa que nenhum FM do CoreLink requer "TLA+ ou fuzz obrigatório + runbook com SLA ≤ 15 min" pelo critério de P0. Mas o framework diferencia P0 de P1: P1 tem SLA de ≤ 1h (imputado de runbooks genéricos), enquanto P0 tem SLA de ≤ 15 min explícito. FM-253 (cross-tenant read) e FM-300 (GC deleta blob vivo) parecem candidatos naturais a P0 dada sua criticidade técnica, mas seus RPN ficam abaixo do threshold por O=1 (baixa ocorrência).

O sistema de scoring pode estar otimista em O (occurrence). Se um bug de cross-tenant read existir, sua frequência em staging não é "nunca observado" — é "provável em teste de carga" (O=2 mínimo, que levaria FM-253 a S×O×D = 5×2×4=40, ainda P1).

**Recomendação:** revisitar os valores de O para FMs de classe adversarial (FM-253, FM-254) e data-integrity (FM-300, FM-303). Considerar que O deve refletir frequência com mitigações ausentes (FMEA original), não com mitigações presentes.

---

**S-20: compliance_matrix.md não está na lista `inherits_from` de nenhum canonical source — é orphan**

O framework §35.5 define que canonical sources são fontes normativas. compliance_matrix.md é explicitamente um canonical source (tem `type: compliance_matrix`), mas nenhum dos outros 10 canonical sources do Lote 4 declara `inherits_from: ["COMPLIANCE-MATRIX"]`. Ao mesmo tempo, compliance_matrix.md referencia CTRLs de security_model e privacy_model mas não declara `inherits_from` deles no seu YAML header.

**Recomendação:** adicionar `inherits_from: ["SECURITY-MODEL", "PRIVACY-MODEL"]` ao YAML de compliance_matrix.md (ele consome controles de ambos). Considerar se WI/Sprint/PRR que tocam SOC 2 ou LGPD/GDPR devem também declarar `inherits_from: ["COMPLIANCE-MATRIX"]`.

---

## Veredito

**CONDITIONAL**

Os 11 canonical sources do Lote 4 representam trabalho substancial e de alta qualidade conceitual. A cobertura de STRIDE, LINDDUN, FMEA, SLOs, GC, dedup, cross-tenant isolation, e compliance é genuinamente SOTA para um produto nessa fase. O validator de schema passa sem erros em todos os arquivos.

No entanto, há três blockers para considerar esses canonical sources "prontos para FROZEN":

1. **S-01 (EVT taxonomy)**: sem resolução, qualquer futuro `validate_evidence.py` vai rejeitar a totalidade das evidências catalogadas nos 11 docs. Isso é uma dívida técnica de processo que cresce com cada novo WI.

2. **S-02 + S-03 (TLA+ obrigatório vs opcional e audit retention 2 anos vs 7 anos)**: são contradições sobre invariantes e requisitos SOC 2 que podem gerar comportamento incorreto de implementação.

3. **S-06 (R2 key format com vs sem HMAC)**: a diferença entre `cas/{tenant_id}/...` e `<hmac(tenant_key,tenant_id)[:16]>/...` é a diferença entre tenant isolation funcionar ou não no storage layer.

Os findings HIGH e MEDIUM não bloqueiam individualmente, mas o volume de dangling references (20 PAT-XXX, 7 CTRL-XXX) indica que o processo de "spec before code" precisa de um validator cross-reference automatizado antes do GA.

---

## Observações meta

**Pontos fortes:**
- A profundidade do threat model STRIDE em `security_model.md` é genuinamente impressionante — 33 THR-XXX com asset, boundary, e múltiplos CTRLs por ameaça.
- `failure_modes.md` com FMEA completo para 52 FMs em 9 classes é uma referência rara em specs de produto early-stage.
- `remote_cache_product_profile.md` é o melhor doc do lote: cobre GC, dedup semântico, negative caching, Merkle decomposition, BWB, conformance REAPI — endereça exatamente os gaps do Lote anterior.
- O sistema de waiver template (Lote 3) é excelente: `expires_at` obrigatório, `revalidation_trigger`, checkpoints T+25/50/75/90% — muito além do padrão de mercado.
- `privacy_model.md` com LINDDUN completo + pipeline DSR concreto + tabela de bases legais é o mais maduro que vi para um produto pre-GA.

**Escopos não avaliados:**
- Não foi possível avaliar o `_audits/matrix-stride-ctrl.csv` mencionado em `security_model.md §11` (arquivo não existe).
- Não foi possível avaliar `legal/dpa/v1.md`, `legal/tia/`, `legal/sub-processors.md` (mencionados mas inexistentes — são "deps futuras" documentadas).
- Os ADRs em `specs/03_architecture/adrs/` não foram auditados (fora do escopo declarado de Lote 3+4).
- A correção técnica do TLA+ sketch em `auth_model.md §8.3` (spec TLA+ embutida) não foi verificada formalmente — a spec omite `Next` e `Init` parciais mas pode estar correta conceitualmente.
- Não foi avaliado se o `validate_specs.py` detecta cross-references faltantes (hoje só valida YAML + schema JSON).
