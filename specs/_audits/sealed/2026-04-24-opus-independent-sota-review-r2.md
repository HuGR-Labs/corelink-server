---
id: AUDIT-OPUS-INDEPENDENT-R2
type: audit
doc_status: REVIEW
audit_status: ACTIVE
version: 0.1.0
created: 2026-04-24
reviewers: [Opus 4.7 1M context — Independent SOTA reviewer]
supersedes: null
superseded_by: null
tags: [audit, sota, sprint-contracts, round-2, opus, independent]
---

# Opus Independent SOTA Review — Sprint Contracts pós-Lote 9.1+9.2

## Disclaimer

Auditoria independente do codex/GPT (não li o output do Round 2 do codex). Foco em ângulos que tipicamente escapam audit linha-a-linha:
- contradições internas dentro do mesmo spec
- realismo de prazos vs DoD multi-mês
- premissas implícitas e capabilities herdadas que outro sprint deveria entregar
- escalabilidade (10 vs 10k tenants) e custo operacional
- cobertura TLA+ vs novas invariants CRITICAL/HIGH
- contradições entre canonical sources e sprint contracts
- ambiguidades regulatórias

**Não duplico** findings já capturados pelo Round 1 do codex (lane mismatches em S-08/S-13, "10 → 14 canonical sources" em S-20, INV-OBS-CARDINALITY typo, ATOMIC-PROVISIONING em S-19, etc.) — aqueles já foram remediados em Lote 9.1+9.2.

## Scope

- 21 spec contracts (S-00..S-20) + meta-contract `_sprint_creation_contract.md`.
- Framework `00_framework.md` v1.0.0-rc1 + 14 canonical sources em `03_architecture/`.
- 53 invariants em `invariant_registry.md` (post Lote 9.1; agora 70+ contando 17 novos sprint-driven em §3.12).
- 40 runbooks (12 stubs criados Lote 9.2) + 17 ADRs (3 stubs Lote 9.2) + 4 TLA+ specs.

---

## Critical findings (delay GA)

### C-01: INV-KEY-OVERLAP é violada pelo próprio S-13/S-14 — 3 valores incompatíveis (24h vs 7d vs 30d)
- **Sprint(s)**: S-13, S-14 (canonical: `key_management.md`).
- **File:line**:
  - `specs/03_architecture/key_management.md:98` define `INV-KEY-OVERLAP: até 24h overlap`.
  - `specs/04_sprints/S13/_spec_contract.md:89` exige `TDK overlap 7d` "em conformidade com `INV-KEY-OVERLAP`".
  - `specs/04_sprints/S13/_spec_contract.md:92` exige `BYOK overlap 7d`.
  - `specs/04_sprints/S13/_spec_contract.md:132` repete "rotation overlap 7d para TDKs verified em property test".
  - `specs/04_sprints/S14/_spec_contract.md:167` exige `key rotation overlap 30d` (Ed25519 attestation key).
- **Descrição**: a invariante canônica diz **24h máximo**; S-13 declara **7d** como sustaining target em DoD e Critérios de Promoção; S-14 introduz um terceiro valor (30d) sem ADR. Isso é uma violação direta de invariante herdada — mais grave porque o sprint diz textualmente "INV-KEY-OVERLAP requirement" enquanto excede o limite por 7×.
- **Impacto**: GA gate exige INV-KEY-OVERLAP enforced; com numerologia inconsistente o property test não tem oracle; auditor SOC 2 vai pegar (NIST SP 800-57 disciplines key lifetime). Bloqueia merge de S-13 com property test razoável.
- **Sugestão**: ADR explicitando overlap distinct por asset class (TDK 7d / PAT 24h / audit 24h / BYOK 7d / Ed25519 attestation 30d) + bump de `key_management.md §3.2` para tabela com `asset_type → max_overlap`. Fixar **antes** de S-13 começar.

### C-02: INV-KEY-OVERLAP, INV-KEY-NO-SKIP referenciados por S-13/S-14 mas não estão no `invariant_registry.md`
- **Sprint(s)**: S-13, S-14.
- **File:line**:
  - `specs/03_architecture/invariant_registry.md` §3.1–3.12 — não há entrada `INV-KEY-*` em nenhum domain.
  - `specs/04_sprints/S13/_spec_contract.md:52,150` cita `INV-KEY-OVERLAP` em inherits_from "INVARIANT-REGISTRY".
  - `specs/04_sprints/S14/_spec_contract.md:151,150` cita `INV-KEY-OVERLAP`, `INV-KEY-NO-SKIP`.
  - `specs/03_architecture/key_management.md:97-98` define os IDs.
- **Descrição**: O registry §3 lista 10 domains (TENANT/CAS/AC/GC/DATA/AUDIT/CONF/AVAIL/BILLING/SUPPLY/PRODUCT). Nenhum cobre `KEY`. O meta-contract §6.2 obriga rastreabilidade INV via registry. As invariantes de key management estão em "ilha" canonical source que não respeita a regra "INV não pode ser criada fora do registry" (`invariant_registry.md:32`).
- **Impacto**: validator (`scripts/validate_references.py --json`) vai apontar dangling. Sprint S-13 não pode promover sem corrigir. Esse pattern (invariantes em fonte canonical mas não no registry) é exatamente a doença que o registry foi criado para curar (G-04 codex re-audit).
- **Sugestão**: adicionar `### 3.13 Key management (domain KEY)` ao registry com `INV-KEY-OVERLAP`, `INV-KEY-NO-SKIP`, `INV-KEY-AUDIT` antes de Lote 9.3.

### C-03: S-04 CAP-AC-004 (TTL 90d default) vs S-07 CAP-EVICT-002 (TTL per-tier free=7d/solo=30d/team=90d/business=365d) — semântica TTL contraditória
- **Sprint(s)**: S-04 vs S-07.
- **File:line**:
  - `specs/04_sprints/_sealed/S04/_spec_contract.md:59,68,79` define `AC TTL default 90d; refresh on hit` + R-S04-5 TTL worker expira `> expires_at`.
  - `specs/04_sprints/_sealed/S07/_spec_contract.md:64` redefine `TTL-based AC entry expiry per-tier (free=7d, solo=30d, team=90d, business=365d, enterprise=customer-configurable)`.
- **Descrição**: codex Round 1 flaggou ownership clash mas focou em quem owna. Mais grave: as semânticas TTL **diferem materialmente** — free tier passa de 90d → 7d (12.8× redução). Customer free criado em S-04 staging vai perder AC entries que o spec original prometeu durar 90d. Não há migration plan; não há ADR; não há override declaration. Cliente em GA pode entrar em estado inconsistente (cobrança baseada em S-07 mas SDK/docs S-04 prometem 90d).
- **Impacto**: customer trust + compliance (SLA published em S-18 R-S18-9 promete tier semantics). Sem reconciliação, S-07 ship efetivamente quebra contract de S-04.
- **Sugestão**: ADR formal "S-07 supersedes S-04 CAP-AC-004 TTL semantics; default tier semantics published in pricing page S-18". Atualizar S-04 com `local_deltas: ["TTL semantics overridden by S-07 CAP-EVICT-002"]`. Bump major se S-04 já SEALED.

### C-04: S-20 GA gate exige `30d sustained staging` + `pentest 2w + retest 1w` + `lighthouse 30d observation` mas duração total = 4 semanas com buffer 10d (factor crunch impossível)
- **Sprint(s)**: S-20.
- **File:line**:
  - `specs/04_sprints/_sealed/S20/_spec_contract.md:108-110` "30d sustained staging…concurrent ao S-17 chaos automation".
  - `specs/04_sprints/_sealed/S20/_spec_contract.md:99-100` "lighthouse migration plan + 30d observation".
  - `specs/04_sprints/_sealed/S20/_spec_contract.md:90-91` "pentest 2 semanas + retest 1 semana".
  - `specs/04_sprints/_sealed/S20/_spec_contract.md:218` PERT total `~213h ≈ 27 dias work`.
  - `specs/04_sprints/_sealed/S20/_spec_contract.md:232` "Duração: 4 semanas (20 dias úteis) + buffer 10 dias".
- **Descrição**: aritmética: pentest scheduling (engagement firm, NDA, scope = 1-2 semanas pre-start) + 2w teste + 1w retest = **5 semanas mínimo só pentest**. 30d staging = 30d. 30d lighthouse observation = 30d. Mesmo totalmente paralelos, são **30 dias** de wall-clock observation period. Sprint declarado 30 dias (4w + 10d buffer = 30d). Resultado: zero margem, e **engineering sprint work** (213h PERT) compete pelo mesmo wall-clock — então engineer está fazendo PRR sign-offs + lighthouse migration + 30d obs supervision em **paralelo** com 213h de novo trabalho. Realmente impossível 1 engineer.
- **Mitigação textual** ("concurrent ao S-17 chaos automation 4w") só funciona se S-17 já termina na D-30 antes do S-20 começar; S-17 timeline (S17:222 sustained chaos extends post-S-17 sprint into S-18..S-20) já consome esse parallel. Logo não há benefit de concurrency real.
- **Impacto**: S-20 vai slipar 4-8 semanas com alta probabilidade. Pior: pressure para "dispensar" 30d obs sustained pode quebrar invariant-de-confiança que o framework inteiro aspira proteger.
- **Sugestão**: ou (a) decompor S-20 em S-20a (engineering gate work, 4 semanas) + S-20b (observation gate, 6 semanas wall-clock só monitoring + retest) com gate explícito; ou (b) declarar abertamente que 30d obs período começa na D-day -30 (i.e., observação **roda durante S-19+S-20** concorrentemente, não dentro de S-20 sozinho), e adicionar requirement "S-19 SEALED → S-20 starts day 0 = observation start"; ou (c) reduzir 30d → 14d com risk acceptance ADR. Status quo é theatrical — DoD checkbox não vai ser real.

### C-05: S-12 DoD aceita `bit-identical OU documented sources com ADR` — DoD não-binário viola meta-contract §6.3
- **Sprint(s)**: S-12.
- **File:line**:
  - `specs/04_sprints/_sealed/S12/_spec_contract.md:142,153` "Reproducible build: 2 runners produzem binário com diff ≤ 5% bytes (release builds); fontes de non-determinism documentadas".
  - `specs/04_sprints/_sealed/S12/_spec_contract.md:159` `10.s12.4 Reproducible build 2-runner diff: 100% bit-identical OU documented sources com ADR`.
  - `specs/04_sprints/_sprint_creation_contract.md:177` "DoD §7 é checklist binário (`[ ]`) — nenhuma cláusula 'best effort'".
- **Descrição**: o item DoD aceita explicitamente "OR documented sources" — o que qualquer engenheiro razoável vai escolher (documentação > 100% bit-identical em Rust+LLVM real). Mais que um wording bug, é o pattern AP-019 do meta-contract ("rigor superficial em scope creep"). Codex Round 1 catched isso só pra S-12 §10.s12.4 mas não para o §6 DoD line 142 que tem mesmo problema com `≤ 5% bytes` (o `≤ 5%` é justamente um soft-target).
- **Impacto**: SLSA L3 attestation está OK, mas reproducible build claim no `/security` (S-18) será misleading; auditor exigente bate. Pior: senta precedente para outros sprints copiarem `OU` em DoD.
- **Sugestão**: substituir DoD por "100% bit-identical OU ADR-XXXX assinado pelo Security lead com lista exhaustiva de fontes documentadas" + transformar `≤ 5% bytes` em métrica observable, não DoD checkbox. ADR-0015 pode já cobrir; verificar.

---

## High findings (worth fixing pre-GA)

### H-01: TLA+ coverage gap — 8 invariantes CRITICAL novas, zero TLA+ specs novas
- **Sprint(s)**: S-10, S-11, S-13, S-14, S-19.
- **File:line**: `specs/03_architecture/invariant_registry.md:166-178` lista 17 invariants novas (§3.12). Filtrando CRITICAL: `INV-BYOK-CRYPTO-SOVEREIGNTY` (S-14:174), `INV-REGION-NO-CROSS-LEAK` (S-14:175). Filtrando "shall have TLA+" pela §2 enforcement table: invariants distributed-systems + algorithm-non-trivial deveriam ter TLA+: `INV-BILLING-NO-LOSS`/`NO-DUP` (S-10), `INV-DSR-ATOMICITY` implicit (S-11 promise `dsr_erasure_atomicity.tla`), `INV-ONBOARD-DPA-FIRST`/`ATOMIC-PROVISIONING` (S-19). 
- TLA+ matrix `invariant_registry.md:182-195` lista 4 specs verdes; nenhuma das 17 novas.
- **Descrição**: registry diz "CRITICAL → TLA+ obrigatório" (`§2`). S-14 declara `INV-BYOK-CRYPTO-SOVEREIGNTY` CRITICAL mas seu DoD lista TLA+ apenas para `region_residency.tla` (não BYOK). S-10 promete `billing_atomicity.tla` em DoD; S-11 promete `dsr_erasure_atomicity.tla`; S-19 não promete TLA+ apesar de ter `INV-ONBOARD-ATOMIC-PROVISIONING` (atomicity = TLA+ canonical use case).
- **Impacto**: registry `§4 TLA+ coverage matrix` ficará invisivelmente furada na GA — DoD checkbox em sprint contract diz "TLA+ verde" mas TLA+ matrix do registry não atualiza. Validator em CI não pega porque referência cross-doc não existe ainda.
- **Sugestão**: bloco em meta-contract §9 invariants: cada invariante CRITICAL adicionada via sprint MUST atualizar `invariant_registry.md §4 matrix` com TLA+ filename + status. Adicionar `dsr_erasure_atomicity.tla`, `billing_atomicity.tla`, `byok_sovereignty.tla`, `onboarding_atomicity.tla` aos planejados antes de S-10.

### H-02: S-09 cardinality budget (20k/métrica) vs S-19 funnel (105 series) é coerente, mas S-08 + S-09 + S-14 explodem em interação
- **Sprint(s)**: S-08, S-09, S-14, S-19.
- **File:line**:
  - `specs/04_sprints/_sealed/S09/_spec_contract.md:91` "20k séries por métrica em produção, 100k global".
  - `specs/04_sprints/S08/_spec_contract.md:87-88` métricas com label `tenant_tier` × `region` × `result` × `reason` (4-D cartesian).
  - `specs/04_sprints/_sealed/S09/_spec_contract.md:88` métricas billing usam `region` × `tenant_tier` × `type`.
  - `specs/04_sprints/S14/_spec_contract.md:88` "Hot blob replica detector usa métrica `corelink.cas.get.bytes_total{tenant_id}`".
- **Descrição**: `tenant_id` em label é red-flag explícito (`14.s09.4 Tracing cardinality from trace_id labels`). S-14 R-S14-3 propõe `corelink.cas.get.bytes_total{tenant_id}` para hot-blob detection — em GA com 1k tenants × 4 regiões × dimensões existentes do S-09, **passa de 100k global em 1 dimensão**. Cardinality budget validator do S-09 (`R-S09-2`) provavelmente vai bloquear PR mas nenhum spec declara o conflito. Codex flaggou cardinality budget mas não detectou que S-14 introduz tenant_id-em-label.
- **Impacto**: S-14 worker `hot blob detector` vai falhar em CI cardinality validator. Workaround silencioso (e.g., agregar offline) reduz fidelity de detecção. Não há plano.
- **Sugestão**: S-14 R-S14-3 mudar pra "exemplar trace_id" ou "`tenant_tier` bucketing" (não `tenant_id`). ADR documentando "tenant_id NUNCA em labels Prom; tenant_tier OK; tenant_id apenas em log/exemplar".

### H-03: Customer-facing UX silenciado — sprints CRITICAL não declaram como customer percebe a falha
- **Sprint(s)**: S-02, S-06, S-10, S-11, S-13, S-14.
- **File:line**: 
  - S-06 R-S06-3: "soft-delete; grace 72h; reversible 24h" — customer não vê erro mas vê latência? Sem spec.
  - S-10 §6 quota state machine: `100% → hard_block (429 over_quota)` — `Retry-After: days-until-month-reset` (`S-10:107` cross-ref `S-08:90`); cliente em workflow CI/CD vai ver quê? Build falha? Cache miss? Não consta.
  - S-13 R-S13-9: "rotation rollback se downstream errors > 1%" — customer vê *qual* error? 503? 401? "Service degraded"? Spec só diz "alert + auto-rollback".
  - S-14 R-S14-8: "CMK revoked → degrade tenant read-only" — customer SDK vai retornar erro tipado? Mensagem? 503? Sem `error_code` semantic catalog.
- **Descrição**: o framework valoriza Working Backwards (`§3`) mas spec contracts não traduzem para error message catalog. Customer/SRE de cliente não tem como debug.
- **Impacto**: docs `/security` (S-18 R-S18-7) vai ter páginas que dizem "BYOK kill switch ≤ 5 min" mas não informam ao desenvolvedor *o que ele vê* na sua build pipeline durante esses 5 min. Boilerplate enterprise pergunta isso no Security Questionnaire.
- **Sugestão**: criar `specs/03_architecture/error_taxonomy.md` Nível 3 com `error_code → http_status → SDK exception class → customer-visible message → next-action` for ≥ 50 error scenarios. Sprints adicionam `error codes new` em local delta. Sem isso, S-15 SDK e S-18 docs vão divergir.

### H-04: S-09 (4 weeks chaos) + S-17 (4 weeks chaos) + S-20 (30d staging) = 90+ dias de gating cumulativos não-articulados
- **Sprint(s)**: S-09, S-17, S-20.
- **File:line**:
  - `specs/04_sprints/_sealed/S09/_spec_contract.md:137` "Synthetic canary 24/7 sustentado 72h".
  - `specs/04_sprints/_sealed/S17/_spec_contract.md:133,210,222` "4 weeks chaos tests …concurrent post-sprint observation".
  - `specs/04_sprints/_sealed/S20/_spec_contract.md:128` "30d sustained staging".
- **Descrição**: cada sprint declara observation period concurrent com sprints subsequentes. Mas timeline real depende de S-X actually running para concurrent observation count. Se S-17 começa T0 (chaos D+1), 4 semanas de chaos terminam em T+28d. S-20 começa após S-19 SEALED — vamos supor T+50d. Então no D+50 a 4-week chaos já terminou; só restam 0 dias dos "4 weeks chaos" reaproveitáveis. S-20 30d obs começa do zero. Cumulativo: ~58 dias adicionais pós-S-19 SEALED. Mas roadmap (S-00 R-S00-1) pretende 12 meses total — não há 58 dias livres.
- **Impacto**: roadmap mainline ou desliza GA, ou viola DoD declarado, ou faz "observation paralela com on-going dev work" que não é real (não há reset de baseline em rolling staging).
- **Sugestão**: declarar explicit em S-20 §13 "observation period D-30 to D+0 = D requires S-17 chaos + S-19 SEALED concurrent — gate deslocado **não compute** observation cumulativo de pre-D-30". E criar `specs/04_sprints/timing-gates-cumulative.md` calendarizando observation gates.

### H-05: S-11 R-S11-7 consent ledger schema permite `wording_id` e `notice_text_hash` mas não define `consent_revocation_proof` — DSR consent_revoke (R-S11-1) não tem proof of revocation
- **Sprint(s)**: S-11, S-19.
- **File:line**:
  - `specs/04_sprints/S11/_spec_contract.md:103-105` schema consent_ledger captura granted=true; revoke não captura mesma estrutura.
  - `specs/04_sprints/S11/_spec_contract.md:84` "DSR consent_revoke" só "revoga consents granted; cascade unsubscribe".
  - `specs/04_sprints/_sealed/S19/_spec_contract.md:65-66` DPA acceptance segue mesmo padrão; revoke (DPA re-acceptance fail) silently degrade read-only sem proof event.
- **Descrição**: GDPR Art. 7 requer "withdrawal as easy as giving consent". Sem schema simétrico (revoke proof = ts + wording_id + signed receipt), customer não pode provar quando revogou. Enforcement asymmetry: granting é cryptographically attested, revoking é apenas DELETE + cascade — não defensível em court se data continued processed por bug.
- **Impacto**: regulatory finding em ANPD/DPC inspection. Pior: contradição inter-spec — S-19 INV-CONSENT-PROOF-VERIFIABLE inherits a S-11 (`S-19:159`) que fornece proof apenas para grant, não revoke.
- **Sugestão**: S-11 R-S11-8 adicionar `consent_revocation` table com mesma 6-field structure; INV-CONSENT-REVOCATION-PROOF-VERIFIABLE (HIGH) novo. Cadastro no registry §3.12.

### H-06: S-08 abuse score (R-S08-7) pode constituir automated decision sob LGPD Art. 20 / GDPR Art. 22 — sem direito-à-revisão
- **Sprint(s)**: S-08, S-11.
- **File:line**:
  - `specs/04_sprints/S08/_spec_contract.md:74` features = `{cpu_wallclock_ratio, egress_bytes_per_min, action_digest_entropy, concurrent_exec_count}`; weighted sum + threshold; outcome = "downgrade silencioso, admin review trigger, suspend".
  - `specs/04_sprints/S08/_spec_contract.md:127` "Automated tenant suspend sem human-in-the-loop = anti-scope" → mas "downgrade silencioso" e "throttle" continuam sendo automated decisions.
  - `specs/04_sprints/S11/_spec_contract.md:206` LGPD Art. 20 / GDPR Art. 22 NOT em CAP-PRIV.
- **Descrição**: LGPD Art. 20 dá direito à revisão de decisões automatizadas que afetem interesse do titular. "Silent downgrade" sem direito-à-revisão = violação. Spec não declara mecanismo de appeal. Mais subtilly: scoring inclui `action_digest_entropy` — derivada de conteúdo customer = potential PII surface.
- **Impacto**: regulatory exposure. Enterprise procurement vai bater nessa página.
- **Sugestão**: S-08 §10 anti-scope ou §6 DoD: "Automated decisão (downgrade/throttle) DEVE expor `GET /v1/admin/abuse_score` self-service + appeal endpoint roteia para human reviewer". Coordenar com S-11 CAP-PRIV-XXX (right to explain).

### H-07: S-15 ADR-0016 escolhe "FFI wrappers" (Rust truth) mas não trata performance overhead Python (pyO3 GIL) e bundle size JS (WASM ≥ 1MB risk em §15) — escolha pode regredir UX
- **Sprint(s)**: S-15.
- **File:line**:
  - `specs/04_sprints/_sealed/S15/_spec_contract.md:111` "rejected porque duplica client-verify logic" — único rationale.
  - `specs/04_sprints/_sealed/S15/_spec_contract.md:239` Risco "JS WASM bundle size > 1MB | M | L | LOW".
  - `specs/03_architecture/adrs/ADR-0016-ffi-wrappers-vs-native-http.md` (66 lines, stub).
- **Descrição**: o trade-off real entre FFI vs native HTTP-thin é maturo em ML community: pyO3 + GIL release pattern é OK pra I/O bound mas custo de import (`corelink-py` package size) e cold-start (Python interpreter loading native lib) são significativos pra serverless workloads. ADR-0016 stub não cobre. JS WASM ≥ 1MB é pior — bundle size em Bazel/Buck2 CI é constraint.
- **Impacto**: time-to-first-cache-hit ≤ 5 min (R-S15-2) talvez não atinja em JS/TS/Python; competitive disadvantage vs NativeLink CLI (Linux-only mas binary é < 30MB).
- **Sugestão**: ADR-0016 expand: tabela `language → cold_start_p99 → bundle_size → install_size` com numbers. Se WASM > 500KB, fallback a thin-HTTP client (ADR provides). Codex SOTA enrichment já sugeriu; reforço.

### H-08: Dual-approval (S-13 INV-ADMIN-DUAL-APPROVAL) define caller≠approver mas não define approver-quorum-rotation (collusion-via-rotation)
- **Sprint(s)**: S-13.
- **File:line**: `specs/04_sprints/S13/_spec_contract.md:84,156` "Approver não pode ser o caller (separation of duties); enforced via D1 check"; risco R-S13-008 "Approver collusion (caller + approver same person via privilege escalation)" mitigation = "Separation of duties enforced D1; quarterly access review".
- **Descrição**: D1 check só verifica `caller_id ≠ approver_id` — não impede que entre 2 admins com role admin, eles **se aprovem mutuamente em sequence** (A approves B's destructive op; B approves A's). Para infrastructure críticas, NIST SP 800-53 AC-2(7) recomenda **rotation-based approver pool de N≥3** com last-N-distinct-approvers tracking. Spec não considera.
- **Impacto**: 2-engineer collusion = trivial bypass. Property test "10k attempts" não testa esse padrão (caller signs A→approve B, then B→approve A em loop).
- **Sugestão**: INV-ADMIN-DUAL-APPROVAL deveria adicionar "last 3 ops in 24h must have ≥ 3 distinct approvers" + property test cobrindo collusion-rotation. Sub-pattern PAT-DUAL-APPROVAL-001 estendido.

### H-09: Cost regression gate ausente em S-07/S-08/S-09/S-10/S-14 (codex flaggou; ainda gap)
- **Sprint(s)**: S-07, S-08, S-09, S-10, S-14.
- **File:line**: codex Round 1 sugeriu `_audits/2026-04-24-codex…:282-284`; nenhum dos 5 sprints introduziu cost regression test em DoD §6. S-14 é o pior: 4 regiões × 4 KMS providers + replication + DEK cache TTL 5 min — cost overhead está em risk register R-S14-012 ("multi-region replication storm cost") mas DoD não exige benchmark de cost.
- **Descrição**: S-14 §14.s14.8 declara "BYOK adds < 15% overhead em CAS path; matched em benchmark CI" — mas só BYOK, não region replication. S-09 cardinality budget é cost-adjacent mas só Grafana side. S-10 Stripe fees são revenue-side, sem expense-side counterpart.
- **Impacto**: GA gate `S-20 §6` não exige cost SLO. Pos-GA, customer enterprise olha unit economics e reclama.
- **Sugestão**: meta-contract §10 Quality Standards: novo `14.10 Cost regression gate (HIGH_RISK only)`: "criterion benchmark com per-op cost estimate; PR > 10% regression bloqueia merge". S-14 §6 incluir "monthly cost projection per tenant tier sustained 7d staging".

### H-10: S-11 erasure cross-backend lista 7 backends mas não inclui Cloudflare Analytics Engine, Loki long-term R2 cold archive, Stripe events log permanent, e R2 audit Object Lock 7y (que é WORM = não pode erase)
- **Sprint(s)**: S-11, S-09, S-10.
- **File:line**:
  - `specs/04_sprints/S11/_spec_contract.md:90-97` 7 backends (D1 / Neon / R2 / KV / DO / Loki / Stripe). 
  - `specs/04_sprints/_sealed/S09/_spec_contract.md:69` audit R2 com Object Lock Governance Mode 7y.
  - `specs/04_sprints/_sealed/S09/_spec_contract.md:99` log lifecycle "cold 400d (R2 Glacier-equivalent)".
  - `specs/04_sprints/S10/_spec_contract.md:64,103` billing-events R2 Object Lock 7y.
- **Descrição**: GDPR Art. 17 vs Object Lock = conflito famoso. Spec S-11 R-S11-4 enumera Loki (warm 90d) — mas log retention é 400d cold (S-09 R-S09-5). Audit R2 Object Lock 7y NÃO PODE ser deleted by erasure (legal hold). Solução é pseudonymization (S-10 §14.s10.4 mencion). Mas S-11 §6 DoD diz "0 records cross-backend"; não declara pseudonymization escape valve. Audit log + billing log retention = 7y immutable PII. Customer DSR erasure = 30d "erasure complete" claim.
- **Impacto**: claim "erasure complete em 30d" é falso para audit trail; customer/auditor pegam. Regulatory exposure se promete e não cumpre.
- **Sugestão**: S-11 R-S11-4 explicit "10 backends: D1 + Neon + R2 (mutable) + R2 (Object Lock — pseudonymize) + KV + DO + Loki + R2-cold-logs + Stripe + Analytics Engine". Adicionar `INV-DATA-ERASURE-COMPLETE-OR-PSEUDONYMIZED` (CRITICAL) ou bifurcate INV em ERASURE-EFFECTIVE (mutable) + ERASURE-PSEUDONYMIZED (Object Lock).

---

## Medium findings (nice-to-have)

### M-01: S-00 lane STANDARD vs codex score sugerido
`specs/04_sprints/S00/_spec_contract.md:38-40` — meta-sprint de planning é STANDARD mas Codex flaggou (Round 1 §S-00) gap_score=6. Independent finding: Working Backwards (PRFAQ) influencia GTM + customer perception; bad output = 12+ months rework. Por scoring `§33.5.2` blast=organization (4) + reversibility=hybrid (1) = 5 → STANDARD top. Mas se PRFAQ inclui pricing claims, FF-HR-009 ativa. Codex menção light; recomendar revisitar com scrutiny pós-roadmap created.

### M-02: S-02 §15 risk register tem 3 cols (`Prob | Impacto`) — meta §10 obriga 6 cols para HIGH_RISK
`specs/04_sprints/S02/_spec_contract.md:131-138`. Já capturado por codex como pattern global; reforço explícito S-02 não foi remediado em Lote 9.1.

### M-03: S-15 doctor diagnostic 8 checks (`R-S15-5`) lista 6 não 8 (network/auth/storage/BYOK/region/quota = 6); inconsistência interna
`specs/04_sprints/_sealed/S15/_spec_contract.md:86-92` enumera 6 checks; `S15:81-83` diz "8 checks" e DoD em `:134` diz "8/8 checks pass". Falta 2.

### M-04: S-13 secret rotation lista 4 asset types mas S-09 audit chain key (`R-S09-10`) = "per-region" implica multi-key (e.g., 4 regions × audit = 4 keys); rotation schedule é "overlap 24h global" ou "per-region staggered"?
`specs/04_sprints/S13/_spec_contract.md:91`. Per-region staggered é menos disruptive; non-staggered = 24h all-region simultaneous = blast risk. Spec ambíguo.

### M-05: S-07 R-S07-3 `last_accessed_at update on every GET` em hot path — escala para 10k QPS é problem (D1 row write storm)
`specs/04_sprints/_sealed/S07/_spec_contract.md:72`. Spec mitiga com "atomic D1 UPDATE ou DO singleton batch" mas não declara batch window. Codex SOTA enrichment "open-loop wrk2/k6" sugere; reforço — explicit batch SLO em DoD.

### M-06: S-09 PII redaction `redact!` macro tipo-driven em Rust — ótimo para Rust workers, mas Logpush (R-S09-5) emite raw worker logs que NÃO passam por `redact!` se logado via println/eprintln
`specs/04_sprints/_sealed/S09/_spec_contract.md:99-103`. Risk R-S09-Risk-002 captura "PII leak em log" mas não distingue redact! macro path vs raw stderr path. CSP-style "no-stderr-in-prod" enforcement não declarado.

### M-07: S-11 §10 anti-scope "Schrems II TIA templates pós-GA se EU tenants materializarem" — mas S-14 R-S14-5 entrega Schrems II TIA template. Cross-sprint contradição
`specs/04_sprints/S11/_spec_contract.md:200` vs `specs/04_sprints/S14/_spec_contract.md:90`. S-11 anti-scope diz "TIA pós-GA"; S-14 entrega TIA pré-GA. Resolver pela narrativa: S-14 é o owner correto (S-14 = enterprise residency); S-11 §10 anti-scope deveria dizer "TIA owned by S-14".

### M-08: S-12 SLSA L3 declara `slsa-github-generator/generator_generic_slsa3.yml@v1.10.0` versão pinada — pinning a tag versão exata sem `@sha256:...` é supply-chain hole (mesmo problema que cargo-deny resolve para deps)
`specs/04_sprints/_sealed/S12/_spec_contract.md:71`. Action pinned by tag = mutable; deve ser pinned by commit SHA per OpenSSF Scorecard.

### M-09: S-16 anti-scope `❌ Admin panel operacional interno` (`S16:177`) cita `admin.corelink.humangr.com` separado mas nenhum sprint cobre `admin.corelink.humangr.com` — orphan ownership
`specs/04_sprints/_sealed/S16/_spec_contract.md:177`. Internal admin tooling é gap visível pré-GA — incident response oncall (S-17) precisa de quê pra ack/manage?

### M-10: S-19 `R-S19-9` first-run renders "CLI install command" + quickstart docs link — spec assume `corelink.humangr.com/cli` e `docs.corelink.humangr.com/quickstart` existem; S-15 + S-18 entregam, mas S-19 não declara dep. CLI install URL deveria vir de config (env-aware staging vs prod)
`specs/04_sprints/_sealed/S19/_spec_contract.md:110-112`.

### M-11: S-17 chaos catalog `≥ 8 FMs` (`S17:84`) cobre 8 de 26 P0/P1 FMs — 18 FMs sem chaos test pré-GA. Spec não justifica seleção dos 8 nem promete "remaining 18 covered post-GA"
`specs/04_sprints/_sealed/S17/_spec_contract.md:84,148`. Codex SOTA enrichment "cargo-mutants" para kill-rate é adjacente; aqui é cobertura de FM space.

---

## Strategic recommendations

### S-1: Promover framework `00_framework.md` v1.0.0-rc1 → v1.0.0 antes de S-02 implementation começar

**Atual**: framework está `DRAFT (FROZEN staffing-blocked)` com **3217 linhas** consumindo 21 sprint contracts downstream. Meta-contract idem (`_sprint_creation_contract.md`).

**Risk**: codex Round 1 já flaggou. Independent reinforcement: framework define lane forcing factors, EVT taxonomy, gate matrices. Sprints v1.1 já começam a usar `EVT-022/EVT-024/EVT-049` (ver S-09/S-11/S-19). Se framework muda no Round 2 codex audit (ex: ajusta EVT taxonomy), 21 sprints precisam re-validate. **Antes de S-02 começar implementation**, framework precisa estar `FROZEN` real, não "staffing-blocked".

**Sugestão concreta**: nomear 2 reviewers (mesmo de fora — colleague em projeto adjacente) para framework + meta-contract. Promote v1.0.0-rc1 → v1.0.0. **Bloqueia S-02 implementation start**.

### S-2: Elevar S-00..S-06 (v1.0 compactos) a v1.1 ANTES de seu sprint começar implementation

**Atual**: S-00..S-06 = v1.0 (média 145 linhas); S-07..S-20 = v1.1 (média 295 linhas).

**Risk**: S-02 implementation começa "qualquer momento" com base em spec contract v1.0 que não tem PERT, risk register 6 cols, SOTA benchmark, EVT typed em DoD. WIs derivados desse spec serão de qualidade <SOTA porque herdam de spec compacto. Rework custo = ~60h por sprint upgrade post-WI-creation.

**Sugestão concreta**: bloco "Lote 9.3 — S00-06 v1.1 elevation" antes de S-02 sprint.md ser criado. Prioridade: S-02 (HIGH_RISK + iminent) > S-06 (HIGH_RISK + GC FF-HR-011) > S-03..S-05 > S-00..S-01.

### S-3: Fazer "TLA+ obligation matrix" canonical e linkar como CI gate

**Atual**: `invariant_registry.md §4` lista 4 specs verdes mas 17 invariants novas adicionadas Lote 9.1 sem TLA+ entry. Validation in CI fica frouxo (ver H-01).

**Sugestão concreta**: 
1. Atualizar `invariant_registry.md §4` matrix com 17 invariants novas + status (`PLANNED | DRAFT | GREEN`).
2. CI gate `scripts/check_tla_obligations.py`: para cada `INV-*` com severity=CRITICAL, falha CI se status ≠ GREEN.
3. Sprints S-10/S-11/S-13/S-14/S-19 obrigatoriamente abrem WI separado para TLA+ spec, não inline em outro WI.

### S-4: Customer-facing error taxonomy como Nível 3 canonical source

**Atual**: ver H-03. 14 canonical sources cobrem domínios técnicos, nenhuma cobre `customer-visible behavior`.

**Sugestão concreta**: criar `specs/03_architecture/error_taxonomy.md` Nível 3 ANTES de S-15 SDK começar. Sprints S-02/S-04/S-06/S-08/S-10/S-11/S-13/S-14 fazem PR adicionando entries. Acaba sendo 50-100 errors ao GA — quality signal forte para enterprise.

### S-5: Decompor S-20 — Engineering Gate (binary, 4w) vs Observation Gate (sustained, 4-6w wall-clock paralelo) — ver C-04

Já coberto em C-04 sugestão. Reinforço strategic: como S-20 está, é o sprint maior risk de slip do programa; desbalanceia roadmap inteiro.

### S-6: Adicionar "Cost regression gate" como universal §14.10 quality standard

Ver H-09. Sem disciplina de cost, GA monetiza ruim e enterprise pricing fica reactive.

---

## Veredito

- **Robustez SOTA real (não cosmetic)**: **7.4/10** (codex Round 1 = 6.8/10 antes de Lote 9.1+9.2; agora um pouco maior). 
  - Pontos fortes: framework rico; canonical sources sólidos; S-09/S-10/S-11/S-13/S-14 com benchmarks SOTA externos cited; INV-* registry §3.12 + 17 sprint-driven; PERT tables; risk register 6 cols em v1.1. Esse é trabalho top-tier vs industry baseline.
  - Pontos fracos persistentes: 5 critical findings (sobretudo C-01 INV-KEY-OVERLAP triple-value e C-04 S-20 timing crunch); TLA+ matrix desatualizada; S-00..S-06 v1.0 vs v1.1 inequality continua; cost regression gate ausente; customer-facing error taxonomy ausente; cross-backend erasure não trata Object Lock (H-10).

- **Implementação pronta para começar S-02?**: **NÃO — com ressalvas curáveis em 5-10 dias**.
  - Não-cura: S-02 é v1.0 compacto, sem PERT, sem evidence tipada, sem SOTA framing — herda quality < S-07/08/09. Implementação derivada vai custar rework.
  - **Pré-condição mínima** antes de S-02 implementation start: 
    1. C-01 fix (INV-KEY-OVERLAP triple-value resolved, ADR);
    2. C-02 fix (INV-KEY-* in registry);
    3. S-2 strategic (S-00..S-06 elevation a v1.1, ao menos S-02);
    4. S-1 strategic (framework v1.0.0-rc1 → v1.0.0).
  - Após pré-condições, S-02 pode começar. C-04 S-20 timing pode ser tratado depois (entre S-19 e S-20 wall-clock).

- **Próximo lote prioritário**: **Lote 9.3 — Pre-implementation Hardening**:
  1. INV-KEY-* registry entry (1d).
  2. ADR sobre overlap-by-asset-type, atualizar S-13/S-14 (1d).
  3. S-00..S-06 v1.1 elevation (5-7d, prioritize S-02 + S-06 first).
  4. TLA+ obligation matrix update + 4 new TLA+ specs planned (`dsr_erasure_atomicity.tla`, `billing_atomicity.tla`, `byok_sovereignty.tla`, `onboarding_atomicity.tla`) (1-2d planning).
  5. Framework + meta-contract `DRAFT (FROZEN staffing-blocked)` → `FROZEN` (sign-offs nomeação) (1d).
  6. Decisão estratégica strategic-rec S-5 (S-20 decomposition) (0.5d decisão; impl em S-20 sprint-time).

  **Total**: ~9-12 dias úteis. Após Lote 9.3, S-02 implementation pode safely começar.

---

**Fim Opus Independent Round 2 Audit.** Findings independentes — minimum overlap with codex Round 1 já remediado em Lote 9.1+9.2. Recomendação executável: Lote 9.3 antes de S-02.
