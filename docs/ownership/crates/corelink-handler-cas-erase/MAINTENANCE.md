---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-handler-cas-erase
manifest: crates/corelink-handler-cas-erase/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-cas-erase-structural-normalization-20260921
---

# corelink-handler-cas-erase — manual de manutenção

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) · [Matriz](#m04) · [Recuperação](#m05) · [Escalação](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Record index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

Confirme baseline, manifesto, módulo e relação estática antes de editar. Esta campanha permite somente leitura e checagens documentais estáticas: não execute Cargo/testes, rede, R2/D1, rota, auth, secret, tenant/dado real, deploy ou publicação. Consulte [Referência](REFERENCE.md#r01) e [Relações](BLAST_RADIUS.md#b03).

<a id="m02"></a>
## M02 — Seleção de procedimento

| Sinal | Procedimento | Limite |
|---|---|---|
| gramática ou erro do digest muda | [PROC-001](#proc-001) | não decide chave de storage externa |
| request/marker ou ordem de validação muda | [PROC-002](#proc-002) | não autentica tenant real |
| gate/outcome muda | [PROC-003](#proc-003) | não decide D1/R2/HTTP |
| API/import do container muda | [PROC-004](#proc-004) | censo reverso permanece incompleto |
| operação externa é necessária | [PROC-005](#proc-005) | bloqueado nesta campanha |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Alterar digest ou erro
**Modo:** `READ_ONLY` até plano de mudança. **Gatilho:** `MAX_DIGEST_LEN`, charset, `validate_digest` ou `CasEraseError` muda.
1. Compare admissão não-vazia, limite e charset contra API-001.
2. Preserve `#[non_exhaustive]` e a distinção entre erro local e `Transport` reservado.
3. Trace o import do container em REL-ERASE-002.
**Esperado:** todo novo valor tem decisão de compatibilidade explícita. **Pare:** a decisão exige formato/chave de storage ou consumidor não classificado. **Recuperação:** restaure bytes locais; não alegue compensação externa. **Evidência:** paths, diff, API-001/API-006. [Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Alterar request, marker ou ordem
**Modo:** `READ_ONLY`. **Gatilho:** `CasEraseRequest`, `TombstoneMarker` ou `prepare_erase` muda.
1. Confirme que tenant divergente continua a vencer digest malformado.
2. Confirme que marker válido usa `auth_tenant` e digest do request.
3. Declare `reason` como string sem limite aplicado por este código.
**Esperado:** INV-001 permanece falsificável e nenhuma função ganha I/O. **Pare:** auth, PII, persistência ou retenção for necessária. **Recuperação:** restaure o contrato puro anterior. **Evidência:** `src/handler.rs`, INV-001, REL-ERASE-003. [Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Alterar gate ou idempotência **Modo:** `READ_ONLY`. **Gatilho:** `read_gate`, `erase_outcome`, `ReadGate` ou `EraseOutcome` muda. 1. Compare os dois valores booleanos possíveis com INV-002 e INV-003. 2. Mantenha claro que os booleanos são entradas do caller, não lookups da crate. 3. Reconcile o impacto estático com o owner do container. **Esperado:** `true` continua distinguindo Gone/AlreadyErased segundo cada função. **Pare:** a mudança precisa decidir ordem/atomicidade de delete e tombstone.

**Recuperação:** restaure mapeamento anterior; `git revert` não desfaz efeitos de uma composição externa. **Evidência:** `src/handler.rs`, B03. [Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Alterar API pública ou consumidor
**Modo:** `READ_ONLY`. **Gatilho:** reexport, assinatura, enum/campo ou import muda.
1. Pesquise os símbolos na fonte não arquivada e atualize REL-ERASE-001/002.
2. Diferencie import direto do container de composição R2/D1 e de execução.
3. Escale consumidor não classificado ao owner apropriado.
**Esperado:** cada relação conhecida declara direção e limite. **Pare:** censo inverso/compatibilidade está ausente. **Recuperação:** mantenha símbolo/reexport até decisão coordenada. **Evidência:** censo estático, B03 e diff. [Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — R2, D1, rota ou runtime
**Modo:** `AUTHORIZED_OPERATION`; **estado nesta campanha:** bloqueado.
**Gatilho:** delete R2, tombstone D1, consulta, auth, rota, métricas, dado real ou deploy.
**Pré-condição:** owner da composição, autorização, escopo, credenciais, rollback e observabilidade aprovados fora desta campanha.
**Esperado:** só atividade autorizada poderia gerar evidência de integração/deployment/runtime. **Pare:** qualquer pré-condição falta. **Recuperação:** seguir runbook do owner; revert local não restaura storage. **Evidência:** não produzida aqui. [Procedure index](#m02)


<a id="m04"></a>
## M04 — Matriz de validação

| Mudança | Checagem permitida | Não prova |
|---|---|---|
| skill | arquivo, front matter, S01–S07 e tabelas decisão | correção do código ou cold review |
| reference | front matter, R01–R08, APIs/invariantes e links | R2/D1/HTTP/runtime |
| blast | front matter, B01–B06, relações direcionais/limites | consumer census completo |
| maintenance | front matter, M01–M06, modo/pré-condição/stop/recovery | operação autorizada |
| qualquer alteração documental | `git diff --check` e lista dos quatro paths permitidos | Cargo, testes, deploy ou publicação |

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

Para bytes ainda locais, reverta somente o commit/arquivos desta tarefa e restaure a versão anterior de API/invariante. Para possível delete R2, tombstone D1, auth, log ou rota, pare: uma reversão Git não desfaz estado externo nem prova compensação. Não use esta crate pura como evidência de recuperação do container.

<a id="m06"></a>
## M06 — Escalação e evidências

Escale o kernel/contrato ao owner deste package, import/rota ao owner do container, R2 à storage, D1 a persistence, auth a security/identity e runtime/deploy a operations. Registre baseline, paths, símbolo/relação, predicado, comando documental, resultado e desconhecidos. Esta documentação requer revisão fria independente; o autor não a certifica.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01).
