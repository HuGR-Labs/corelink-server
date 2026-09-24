---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-cf-bindings
manifest: crates/corelink-cf-bindings/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: cf-bindings-pilot-source-20260920
---

# corelink-cf-bindings — manual de manutenção

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) · [Matriz](#m04) · [Recuperação](#m05) · [Escalação](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Confirme manifesto, baseline, `wasm32-unknown-unknown`, consumidor e contrato antes de editar. Não carregue credenciais, rode `wrangler`, acesse R2/D1/KV/DO, faça deploy ou use dados de tenant. Consulte [Referência](REFERENCE.md#r01) e [Relações](BLAST_RADIUS.md#b03).

<a id="m02"></a>
## M02 — Seleção de procedimento

| Sinal | Procedimento | Limite |
|---|---|---|
| `WasmOnly:` | [PROC-001](#proc-001) | host não certifica binding |
| tenant/audit/TTL | [PROC-002](#proc-002) | usar fake/stub isolado |
| API ou reexport | [PROC-003](#proc-003) | coordenar consumidores |
| build wasm | [PROC-004](#proc-004) | não equivale a deploy |
| binding/deploy/dado real | [PROC-005](#proc-005) | operação autorizada |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Diagnosticar `WasmOnly`
**Modo:** `READ_ONLY`. **Gatilho:** erro `Backend("WasmOnly: …")`.
**Pré-condição:** nome do adapter/operação e target conhecido.
1. Localize o erro em `*_real.rs` e o `cfg` em `lib.rs`.
2. Confirme se o chamador selecionou wasm ou host.
3. Verifique se a validação/audit ocorreu antes do stub.
**Esperado:** causa é target host, não confirmação de binding ausente. **Pare:** se exigir ambiente Cloudflare. **Evidência:** arquivo, linha, target e erro sem payload.
**Registro PROC-001:** `mode=READ_ONLY`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=true`; `environment=checkout pinado`; `result=NOT_EXECUTED`; `review_evidence=REFERENCE R06/R07, src/lib.rs e *_real.rs`; `execution_evidence=[]`; `limitations=sem build/runtime Cloudflare`.
[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Alterar escopo, audit ou TTL
**Modo:** `LOCAL_ISOLATED`. **Gatilho:** mudança em prefixo, tenant, callback ou TTL.
**Pré-condição:** contrato e owner do dado identificados; fake/stub disponível.
1. Trace entrada → tipo scoped → audit → backend.
2. Escreva/seleciona teste de rejeição antes da alteração.
3. Preserve a ordem validação → audit → backend.
4. Teste erro do audit e limite TTL quando aplicável.
**Esperado:** backend/fake não é alcançado após rejeição pré-operação; em D1
`run`, registre separadamente eventual erro da emissão pós-mutação. **Pare:**
mudança de semântica do tenant. **Recuperação:** reverta bytes locais; não
presuma que rollback repara dado já escrito. **Estado:** revisado, não
executado nesta campanha.
**Registro PROC-002:** `mode=LOCAL_ISOLATED`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=true`; `environment=fake/stub local não executado`; `result=NOT_EXECUTED`; `review_evidence=INV-001..004 e relações de audit`; `execution_evidence=[]`; `limitations=sem teste Rust e sem dado externo`.
[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Mudar API pública ou reexport
**Modo:** `READ_ONLY` até plano aprovado. **Gatilho:** símbolo, erro, tipo ou path canônico muda.
1. Pesquise `corelink_cf_bindings` e `corelink_adapters_cloud::cf`.
2. Compare contratos e versões/compatibilidade dos consumidores.
3. Atualize as relações atômicas e documentação antes de implementar.
**Esperado:** todos os consumidores conhecidos têm decisão explícita. **Pare:** consumidor externo ou compatibilidade incerta. **Evidência:** censo, decisões e owner coordenado.
**Registro PROC-003:** `mode=READ_ONLY`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=true`; `environment=checkout pinado`; `result=SOURCE_CENSUS`; `review_evidence=BLAST B03/B06 e manifests consumidores`; `execution_evidence=[]`; `limitations=owners/compatibilidade externa não verificados`.
[Índice de procedimentos](#m02)

<a id="proc-004"></a>

### PROC-004 — Verificar build wasm selecionado
**Modo:** `LOCAL_ISOLATED`. **Gatilho:** dependência, cfg ou fonte wasm muda.
**Pré-condição:** toolchain/target instalado e sem segredo.
1. Registre `rustc --version` e target.
2. Rode `cargo check --locked -p corelink-cf-bindings --target wasm32-unknown-unknown`.
3. Registre comando, commit e resultado literal.
**Esperado:** verifica apenas seleção/build. **Pare:** resolver rede, lock alterado ou erro de ambiente. **Recuperação:** restaure configuração local; não publique. **Estado:** revisado-não-executado.
**Registro PROC-004:** `mode=LOCAL_ISOLATED`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=true`; `environment=target wasm não executado nesta revisão`; `result=NOT_EXECUTED`; `review_evidence=R06, REL-CF-009/016/023/024`; `execution_evidence=[]`; `limitations=sem Cargo/toolchain e sem artefato wasm`.
[Índice de procedimentos](#m02)

<a id="proc-005"></a>

### PROC-005 — Binding ou operação Cloudflare
**Modo:** `AUTHORIZED_OPERATION`. **Gatilho:** validar configuração, chamada, deploy ou dado real.
**Pré-condição:** owner operacional, escopo/tenant, credenciais, janela e rollback aprovados.
1. Congele configuração e plano de recuperação.
2. Execute somente ação autorizada com logs redigidos.
3. Verifique resultado e capture identificadores sem segredo.
**Esperado:** evidência `DEPLOYMENT`/`OBSERVED_RUNTIME` com escopo. **Pare:** qualquer autorização, custo ou dado fora do plano. **Estado:** bloqueado para esta campanha.
**Registro PROC-005:** `mode=AUTHORIZED_OPERATION`; `review_status=BLOCKED`; `execution_status=BLOCKED_FOR_OPERATION`; `required_for_acceptance=false` para este manual documental; `environment=Cloudflare não autorizado`; `result=NOT_EXECUTED`; `review_evidence=wranglers REL-CF-012/032 e ownership R02`; `execution_evidence=[]`; `limitations=sem credencial, deploy, binding, custo ou runtime`.
[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes/validação

| Mudança | Package/target/features e comando | Suite/predicado/ambiente | O que não prova |
|---|---|---|---|
| wrapper/erro | `corelink-cf-bindings`; host fake/stub; teste dirigido | rejeição pré-operação impede backend; D1 pós-run pode falhar após mutação; local isolado | API Cloudflare real |
| trait/reexport | package + consumidores; censo literal e check selecionado | símbolos/peers reconciliados no checkout | reachability runtime |
| cfg/dependência | `cargo check --locked -p corelink-cf-bindings --target wasm32-unknown-unknown` | seleção/build wasm; toolchain registrado | binding/deploy |
| binding/configuração | `corelink-clerk-cf` + Wrangler root/Clerk; operação autorizada | IDs/vars/entrypoint e resultado externo | segurança de consumidores não testados |

**Estado de aceitação desta campanha:** PROC-001–004 são requeridos, com `review_status=BLOCKED`, `execution_status=REVIEWED_NOT_EXECUTED` e `result=NOT_EXECUTED` (PROC-003 mantém `SOURCE_CENSUS` como resultado documental). Nenhum deles constitui aprovação nem execução local; a checagem Cargo wasm e os testes de fake/stub continuam pendentes.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

Mantenha `WasmOnly:` e tipos/erros como superfície compatível até decisão coordenada. Para falha antes de chamada externa, reverta a alteração local e reexecute o teste isolado. Para escrita ou schema externo, pare: `git revert` não desfaz dados, retenção, efeitos ou clientes já observados; use plano do owner operacional.

<a id="m06"></a>
## M06 — Escalação e evidências

Escalone para owner de `corelink-worker`/`corelink-cas` por trait, para `adapters-cloud` por reexport, para consumidor por wiring, segurança por tenant/audit e operação Cloudflare por binding/deploy. A rota humana verificada é `.github/CODEOWNERS` regra `* → @gmhelmold`; o arquivo registra que CODEOWNERS solicita revisão, mas não é merge gate. Capture baseline, target, relação, comando, resultado, logs redigidos e lacunas. Cada artefato requer cold review independente; aprovação documental não executa PROC-005.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01).
