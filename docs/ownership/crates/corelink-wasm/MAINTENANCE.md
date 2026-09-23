---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-wasm
manifest: crates/corelink-wasm/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-wasm-structural-normalization-20260921
---

# corelink-wasm — manutenção de ownership

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) · [Validação](#m04) · [Recuperação](#m05) · [Escalação](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

Use checkout isolado no SHA registrado e leia `Cargo.toml`, `package.json` e `src/lib.rs` antes de qualquer conclusão. Esta campanha permite apenas SOURCE e checagem estrutural documental; não autoriza Cargo, wasm-pack, wasm-opt, teste, Node, browser, npm, rede, publicação, segredo/PAT, CAS ou deploy.

<a id="m02"></a>
## M02 — Seleção de procedimento

| Sinal | Procedimento | Limite |
|---|---|---|
| método/annotation/forma de dados muda | [PROC-001](#proc-001) | fonte não revela binding gerado |
| digest, opt-out ou erro muda | [PROC-002](#proc-002) | política canônica é externa |
| crate type/deps/package metadata muda | [PROC-003](#proc-003) | declaração não é artefato/publicação |
| ownership docs prontas | [PROC-004](#proc-004) | checks não são build/review |
| pedido exige npm, JS, browser ou artefato | [PROC-005](#proc-005) | requer escopo/owner autorizado |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Alterar ponte pública
**Objetivo/gatilho:** método, tipo ou annotation muda. **Pré-condições/entradas:** SHA, diff e símbolos de R04. **Ambiente/permissões:** `READ_ONLY`; fonte local, sem build ou execução.

**Passos:** 1. Compare `CoreLinkClient::new`, getter, `get`, `put`, `stat`. 2. Classifique cada símbolo como Rust público, wasm-bindgen anotado ou serde-only. 3. Atualize API, INV e REL atingidos.

**Predicado esperado:** alegações citam assinatura e annotation sem prometer binding JS gerado. **Falha/parada:** pare se binding produzido, consumidor ou compatibilidade JS for necessário. **Recuperação:** remova alegação não suportada; nenhum artefato gerado deve ser inferido. **Evidência:** source paths e IDs atualizados. **Revisão:** REVIEWED. **Execução:** REVIEWED_NOT_EXECUTED; nenhum build/runtime executado.
[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Alterar integridade ou opt-out
**Objetivo/gatilho:** verifier, default ou erro muda. **Pré-condições/entradas:** mudança de contrato identificada e R04/R05 lidos. **Ambiente/permissões:** `READ_ONLY`; fonte local.

**Passos:** 1. Trace config → `VerifyConfig` → `ClientVerifier` → helper. 2. Preserve `true` como default ou registre mudança explícita. 3. Compare os códigos de erro e REL-001.

**Predicado esperado:** cada erro pertence ao caminho identificado na fonte. **Falha/parada:** pare se precisar afirmar algoritmo, feature resolvida, telemetria entregue ou comportamento cross-language. **Recuperação:** mantenha desconhecido e roteie ao owner `corelink-client-verify`. **Evidência:** paths e diff. **Revisão:** REVIEWED. **Execução:** REVIEWED_NOT_EXECUTED.
[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Alterar configuração WASM ou pacote
**Objetivo/gatilho:** crate type, dependência, metadata ou lista de arquivos muda. **Pré-condições/entradas:** diff de manifesto/metadata e REL-004/005. **Ambiente/permissões:** `READ_ONLY`; sem wasm-pack/npm/rede.

**Passos:** 1. Compare `cdylib`, `publish=false`, dependências e metadata. 2. Separe cfg/dev-dependency de build executado. 3. Registre a colisão nominal `@corelink/client` sem inferir identidade.

**Predicado esperado:** declaração e distribuição continuam distintas. **Falha/parada:** pare se build, pacote, registry ou browser for requerido. **Recuperação:** uma reversão local não desfaz publicação; escale a release owner. **Evidência:** manifests, arquivos e IDs. **Revisão:** REVIEWED. **Execução:** REVIEWED_NOT_EXECUTED.
[Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Validar entrega documental
**Objetivo/gatilho:** os quatro artifacts estão prontos para review. **Pré-condições/entradas:** checkout root, checker e quatro paths. **Ambiente/permissões:** `LOCAL_ISOLATED`; Markdown/Python local.

**Passos:** 1. Rode quatro comandos do M06. 2. Rode `git diff --check`. 3. Compare paths alterados com o escopo.

**Predicado esperado:** quatro checks passam e diff é whitespace-clean. **Falha/parada:** pare em checker falho ou path extra. **Recuperação:** corrija apenas artifacts atribuídos e repita checks. **Evidência:** JSON e path list. **Revisão:** REVIEWED. **Execução:** REVIEWED_NOT_EXECUTED; registrar resultado real no handoff.
[Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Operação de build ou publicação
**Objetivo/gatilho:** solicitação futura para gerar ou publicar pacote. **Pré-condições/entradas:** owner de release, artefato/ambiente, autorização, compatibilidade e recuperação documentados. **Ambiente/permissões:** `AUTHORIZED_OPERATION`; nenhuma permissão é concedida por este manual.

**Passos:** este documento não prescreve comando operacional; solicitar plano específico e revisar identidade `@corelink/client` antes da execução.

**Predicado esperado:** definido no plano autorizado antes da operação. **Falha/parada:** qualquer pré-condição ausente, segredo ou identidade de pacote ambígua. **Recuperação:** plano de release cobre artefatos externos; `git revert` não os desfaz. **Evidência:** autorização e resultado do owner. **Revisão:** BLOCKED. **Execução:** BLOCKED_FOR_AUTHORIZED_OPERATION.
[Procedure index](#m02)


<a id="m04"></a>
## M04 — Matriz de testes/validação

| Checagem | Evidência que produz | O que não prova |
|---|---|---|
| S-SKILL | S01–S07 e decisão presentes | correção semântica/cold review |
| S-REFERENCE | R01–R08, APIs e invariantes presentes | binding/JS/runtime |
| S-BLAST | B01–B06 e limites presentes | censo de consumidores resolvido |
| S-MAINTENANCE | M01–M06 e procedimentos presentes | build, publicação ou recuperação externa |
| `git diff --check` | ausência de erro de whitespace | escopo, compilação ou execução |

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

Mantenha explícita a distinção entre assinatura Rust, annotation wasm-bindgen e binding gerado. Antes de artefato externo, uma mudança local pode ser revertida. Depois de gerar, publicar ou distribuir, um revert de Git não remove arquivos, caches, versões ou clientes; pare e siga o plano do owner de release. Não trate o `publish = false` de Cargo como controle de npm, nem `package.json` como autorização de publicação.

<a id="m06"></a>
## M06 — Escalação e evidências

Escale client-verify para digest/config/código, JS/documentação para a colisão `@corelink/client`, release para artefato e npm, e segurança para PAT/segredo. O handoff inclui baseline, arquivos lidos/alterados, API/invariante/relação afetadas, quatro resultados S, `git diff --check` e lacunas. Ficam obrigatoriamente explícitos: nenhum Cargo/build/teste, WASM gerado, JS/browser/Node, npm, rede ou publicação foi executado; cold review ainda é independente.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-wasm/SKILL.md#s01).
