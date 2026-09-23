---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-dsr
manifest: crates/corelink-dsr/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-dsr-structural-normalization-20260921
---

# corelink-dsr — manual de manutenção

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) · [Matriz](#m04) · [Recuperação](#m05) · [Escalação](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

Confirme baseline, manifesto, módulo e consumidor estático antes de editar. Esta campanha permite somente leitura/checagens documentais estáticas: não execute Cargo ou testes e não use rede, segredo, key/KMS, WebAuthn, Neon, Email, rota/binding CF, tenant/dado real ou deploy. Consulte [Referência](REFERENCE.md#r01) e [Relações](BLAST_RADIUS.md#b03).

<a id="m02"></a>
## M02 — Seleção de procedimento

| Sinal | Procedimento | Limite |
|---|---|---|
| direito, decisão, calendário ou SLA muda | [PROC-001](#proc-001) | não interpreta obrigação jurídica |
| MFA destrutiva muda | [PROC-002](#proc-002) | token real exige owner de identidade |
| audit, store ou recibo muda | [PROC-003](#proc-003) | fake não certifica backend/key real |
| import/reexport/consumidor muda | [PROC-004](#proc-004) | referência adjacente não é import |
| operação externa é necessária | [PROC-005](#proc-005) | bloqueado nesta campanha |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Alterar taxonomia ou calendário
**Objetivo/gatilho:** enum, `sla_for`, status ou decisão mudou. **Modo:** `READ_ONLY` até plano aprovado.
**Pré-condições:** `event.rs`/`calendar.rs`, consumidores e predicado estão identificados.
1. Liste valores canônicos e reexports afetados.
2. Trace container, privacy e harnesses em B03.
3. Declare que o predicado é cálculo tipado, não prazo legal concluído.
**Esperado:** cada consumidor e valor alterado tem decisão explícita. **Pare:** faltar regra jurídica/calendário externo.
**Recuperação:** reverta bytes locais e restaure o predicado anterior. **Evidência:** paths, diff estático e desconhecidos. [Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Alterar MFA destrutiva **Modo:** `READ_ONLY` até plano de mudança. **Gatilho:** `is_destructive`, token ou verifier muda. **Pré-condição:** INV-001 e os seis braços foram comparados na fonte. 1. Compare Erasure/Rectification contra os quatro braços restantes. 2. Confirme que ausência de token não alcança `store.insert`. 3. Classifique WebAuthn real como dependência externa diferida. **Esperado:** o predicado de INV-001 permanece falsificável. **Pare:** identidade, token real ou decisão de risco forem

necessários. **Recuperação:** restaure o gate anterior; não suponha que isso revoga uma ação externa. **Evidência:** `event.rs`, `endpoint.rs`, B03. [Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Alterar audit, store ou recibo **Modo:** `READ_ONLY` até plano de mudança. **Gatilho:** sink, `insert`, issuer ou erro muda. **Pré-condição:** ordem do pipeline e trait owner identificados. 1. Trace `submit` da entrada até cada `emit` e `DsrRequestStore::insert`. 2. Preserve o predicado INV-002 para a mutação do ticket. 3. Separe o emissor em memória de RS256/KMS e store Neon reais. **Esperado:** falha do sink antes do insert impede

esse insert no orquestrador. **Pare:** persistência, chave ou audit externo forem exigidos. **Recuperação:** reverta alteração local e reclassifique qualquer efeito externo junto ao owner. **Evidência:** `endpoint.rs`, `audit.rs`, `store.rs`, `receipt.rs`. [Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Alterar API pública ou reexport
**Modo:** `READ_ONLY`. **Gatilho:** trait, tipo, erro ou caminho público muda.
**Pré-condição:** B03 atualizada e direção dependência/impacto identificada.
1. Pesquise `corelink_dsr` e `corelink_privacy::dsr` em fontes não arquivadas.
2. Diferencie manifest/import direto de comentário ou adjacência CF/clerk/replication.
3. Pare com consumidor não classificado e escale ao owner.
**Esperado:** nenhum consumidor estático conhecido fica sem classificação. **Recuperação:** mantenha reexport/símbolo até decisão coordenada. **Evidência:** censo, paths e desconhecidos. [Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Rota ou integração de produção
**Modo:** `AUTHORIZED_OPERATION`; **estado nesta campanha:** bloqueado.
**Gatilho:** CF route, KMS/RS256, WebAuthn, Neon, Email, tenant/dado real ou deploy.
**Pré-condição:** owner operacional, autorização, escopo, credenciais, rollback e observabilidade aprovados fora desta campanha.
**Esperado:** somente uma atividade autorizada poderia gerar evidência de deployment/runtime. **Pare:** qualquer pré-condição ausente. **Recuperação:** seguir runbook do owner; revert local não desfaz dados, email ou efeitos externos. **Evidência:** não produzida aqui. [Procedure index](#m02)


<a id="m04"></a>
## M04 — Matriz de validação

| Mudança | Checagem permitida | Não prova |
|---|---|---|
| documento de contrato | cabeçalhos, âncoras, IDs e links estáticos | semântica Rust/runtime |
| enum/trait | busca de símbolo e assinatura fonte | compatibilidade de todos os clientes |
| ordem audit/store | leitura da sequência em `endpoint.rs` | persistência/durabilidade externa |
| MFA/recibo | predicado fonte e taxonomia de erro | WebAuthn/KMS/RS256 real |
| consumidor | manifesto/import estático | reachability/deploy |

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

Para mudança não publicada, reverta somente os bytes locais e restaure o contrato/reexport anterior. Para possível dado, token, audit, email ou mutação externa, pare: `git revert` não restaura backend nem prova compensação. Não use o fake para concluir que a recuperação de Neon/CF/KMS funcionará.

<a id="m06"></a>
## M06 — Escalação e evidências

Escalone privacy por reexport, container por portal, erasure por lifecycle, CF/clerk por wiring, replication por adjacência, identidade por WebAuthn, segurança/keys por RS256/KMS e persistence/operations por Neon/Email/rota. Registre baseline, paths, relação, predicado, comando documental, resultado e desconhecidos. Esta documentação ainda requer revisão fria independente; nenhum autor a certifica nesta campanha.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01).
