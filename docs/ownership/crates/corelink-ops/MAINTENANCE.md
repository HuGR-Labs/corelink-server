---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-ops
manifest: crates/corelink-ops/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: H
state: author_validated
evidence_set: source-inspection-6be030999
---

# corelink-ops — manual de manutenção

[Preparação](#m01) · [Escolha](#m02) · [Procedimentos](#m03) · [Matriz](#m04) · [Recuperação](#m05) · [Escalonamento](#m06)

<a id="m01"></a>
## M01 — Preparação segura

Use baseline `6be030999`, manifesto, `src/lib.rs` e fonte afetada. O modo desta entrega é `REVIEWED_NOT_EXECUTED`: inspeção SOURCE, sem seleção de compilação, execução de bin, rede ou ambiente externo. Invariante falsificável: conclusão marcada executada sem saída preservada é inválida. Pare se baseline ou caminho citado não existir.

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito permitido | Autorização extra |
|---|---|---|---|---|
| entender namespace, contrato local ou alias | [PROC-001](#proc-001) | READ_ONLY_LOCAL | leitura de R03/API-003 e fonte | não |
| alterar predicado | [PROC-002](#proc-002) | LOCAL_CHANGE | patch aprovado | revisão |
| alterar um dos três bins dry-run, o verificador, target ou feature | [PROC-003](#proc-003) | LOCAL_CHANGE | fonte local | revisão |
| escolher validação | [PROC-004](#proc-004) | REVIEWED_NOT_EXECUTED | registrar seleção | execução |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Classificar módulo

**Gatilho:** dúvida sobre owner ou contrato de caminho. **Modo:** READ_ONLY_LOCAL. **Entradas:** `src/lib.rs`, módulo local ou facade. **Pré-condição:** baseline disponível.

1. Localize `pub mod` em `src/lib.rs`.
2. Consulte R03/API-003 para a família local e âncoras contratuais; leia a fonte afetada.
3. Classifique `pub use` puro como fachada e confirme o crate fornecedor.
4. Registre API-002/API-003 e REL-001 ou REL-008; preserve os desconhecidos de runtime.

**Falhas e parada:** módulo ausente ou ambíguo. **Recuperação:** sem alteração. **Evidência:** paths e símbolos. **Certificação:** REVIEWED_NOT_EXECUTED.

[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Alterar pré-checagem local

**Gatilho:** mudança aprovada no predicado. **Modo:** LOCAL_CHANGE. **Entradas:** contraexemplo definido. **Pré-condição:** INV-001/002 lidos.

1. Defina entrada que muda classificação.
2. Atualize função e contrato juntos.
3. Confirme que rejeições interrompem.
4. Selecione alvo de propriedade para validação autorizada posterior.

**Falhas e parada:** sem predicado falsificável. **Recuperação:** reverta patch. **Evidência:** diff e alvo. **Certificação:** revisão antes de execução.

[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Alterar alvo declarado

**Gatilho:** name, path, feature ou target muda. **Modo:** LOCAL_CHANGE. **Entradas:** par name/path. **Pré-condição:** R06 e REL lidos.

1. Identifique bloco exato no manifesto.
2. Compare path com fonte local.
3. Preserve classificação dry-run de `rb_fm_201_dry_run`, `rb_fm_205_dry_run` e `rb_fm_206_dry_run`.
4. Trate `corelink-supply-verify` como bin verificador distinto; registre feature opt-in sem inferir execução.

**Falhas e parada:** path ausente, feature não declarada ou alvo não suportado. **Recuperação:** restaure bloco. **Evidência:** diff e path. **Certificação:** SOURCE até validação.

[Índice de procedimentos](#m02)

<a id="proc-004"></a>

### PROC-004 — Registrar validação futura

**Gatilho:** mudança requer verificação. **Modo:** REVIEWED_NOT_EXECUTED. **Entradas:** mudança e alvo. **Pré-condição:** sem autorização para executar.

1. Relacione mudança a API-001 ou API-003, INV e REL correspondente.
2. Escolha alvo declarado sem alegar resultado.
3. Registre target, feature e variável relevante.
4. Entregue seleção ao revisor autorizado.

**Falhas e parada:** alvo não declarado ou seleção ambígua. **Recuperação:** nenhuma. **Evidência:** M04. **Certificação:** não executado.

[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

| Mudança | Seleção | Predicado | Modo / evidência |
|---|---|---|---|
| pré-checagem | `admin_dry_run_prop_dual_approval_invariants` | rejeição interrompe | REVIEWED_NOT_EXECUTED / SOURCE |
| API/policy admin | alvos admin em B06 | interface e resultado local | REVIEWED_NOT_EXECUTED / SOURCE |
| alertas, deploy ou DR | alvos da família em B06 | canais, verificação ou transições | REVIEWED_NOT_EXECUTED / SOURCE |
| Drata, oncall ou rotação | alvos da família em B06 | retry/ledger, paging ou rollback | REVIEWED_NOT_EXECUTED / SOURCE |
| migrations ou supply chain | alvos da família em B06 | replay ou decisão local | REVIEWED_NOT_EXECUTED / SOURCE |
| survey ou offboarding | alvos da família em B06 | token/recorder ou transições | REVIEWED_NOT_EXECUTED / SOURCE |
| módulo público | biblioteca e path | namespace preservado | REVIEWED_NOT_EXECUTED / diff |
| bin dry-run | um dos três `rb_fm_*` | name/path e classificação pareados | REVIEWED_NOT_EXECUTED / SOURCE |
| bin verificador | `corelink-supply-verify` | name/path pareados | REVIEWED_NOT_EXECUTED / SOURCE |
| target wasm32 | condição declarada | `uuid` inclui `js` | REVIEWED_NOT_EXECUTED / manifesto |
| feature | `default` ou opt-in | seleção explícita | REVIEWED_NOT_EXECUTED / manifesto |

Cinco axiomas locais: (1) SOURCE não prova execução; (2) target declarado não prova suporte observado; (3) feature opt-in não prova comportamento; (4) alias não muda owner; (5) resultado só existe com saída preservada. Negativos do preflight: UUID curto, duplicado e autoaprovador. B06 enumera os 26 nomes de teste do manifesto por família; nenhum teste foi executado.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversível | Condição | Ação segura | Prova |
|---|---|---|---|---|
| predicado | sim | patch isolado | reverter patch | INV-001/002 anterior |
| path de bin | sim | sem artefato externo | restaurar `name`/`path` | manifesto coincide |
| alias | condicional | fornecedor não mudou | restaurar `pub use` | API-002 preservada |

O package não demonstra estado persistente próprio. Limite: reversão local não recupera mudança no fornecedor; encaminhe REL-008 ao owner correto.

<a id="m06"></a>
## M06 — Escalonamento e registro de manutenção

| Condição | Rota | Evidência mínima | Ação vedada |
|---|---|---|---|
| fornecedor reexportado muda | owner via REL-008 | alias e símbolo | editar como local |
| runtime alegado | revisão independente | ambiente e saída | inferir de SOURCE |
| target/feature ambíguo | revisão de manifesto | bloco e seleção | supor escolha |
| bin dry-run ou verificador muda de papel | owner e revisor | diff e escopo | executar bin |

Desconhecidos: matriz por plataforma, consumidores inversos, resultados dos 26 alvos, comportamento externo e contratos dos símbolos não selecionados em API-003. Registre baseline, modo, evidência e risco residual; apenas temporários criados pela tarefa podem ser limpos.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
