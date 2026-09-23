---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-ops
manifest: crates/corelink-ops/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: H
state: author_validated
evidence_set: source-inspection-6be030999
---

# corelink-ops — blast radius

[Escopo](#b01) · [Método](#b02) · [Relações](#b03) · [Propagação](#b04) · [Mudanças](#b05) · [Cobertura](#b06)

<a id="b01"></a>
## B01 — Leitura rápida e escopo

O raio é estático e local: manifesto, namespace, três bins dry-run, um bin verificador e testes declarados. Risco principal: tratar fachada como implementação própria ou alvo declarado como execução. Esta é uma seleção de relações prioritárias, não censo completo dos 98 arquivos e 26 alvos de teste. Invariante falsificável: cada relação nomeia origem, destino, ativação e efeito; ausência de um campo invalida a ficha.

**Builds avaliados:** nenhum. **Ambientes não observados:** todos fora da inspeção SOURCE.

<a id="b02"></a>
## B02 — Inventário e método

| População | Fonte | Seleção | Limite |
|---|---|---|---|
| Namespace | `src/lib.rs` | módulos públicos | não prova import por consumidor |
| Bins | `Cargo.toml` | três dry-run e um verificador | não prova execução |
| Testes | `Cargo.toml` | 26 `[[test]]` | não prova resultado |
| Fachadas | módulos com `pub use` | aliases locais | não prova chamada no fornecedor |

Invariante falsificável: quinta entrada `[[bin]]`, ou menos de 26 `[[test]]`, contradiz esta baseline. O método não enumera relações internas de todos os 98 arquivos; essa lacuna permanece explícita em B06.

<a id="b03"></a>
## B03 — Relações diretas selecionadas

Índice selecionado: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008). Estas oito fichas priorizam namespace, predicado, três bins dry-run, um verificador e a fronteira de fachada. A população pública local está agrupada por caminho e contrato em [API-003](REFERENCE.md#api-003); a tabela seguinte relaciona famílias para revisão sem alegar um grafo runtime completo.

| Grupo de caminhos locais | Origem → destino estático | Mudança local que propaga | Contrato de referência |
|---|---|---|---|
| `admin::api`, `admin::dry_run` | manifesto → implementação local → consumidores do módulo | policy, resposta ou preflight | API-003; `admin_dry_run_prop_dual_approval_invariants` |
| `alerts`, `deploy`, `dr::*` | módulo raiz → submódulos locais | tipo, interface ou transição | API-003; alvos deploy e DR |
| `drata`, `oncall`, `rotation::worker` | módulo raiz → runner/ledger/state machine | idempotência, retry, turno ou rollback | API-003; alvos drata, oncall e rotation |
| `migrations`, `supply_chain::*` | módulo raiz → replay/policy/verifier | ordenação/rewrite ou regra de aceitação | API-003; alvos migrations e supply-chain |
| `survey`, `tenant_offboarding` | módulo raiz → signer/recorder ou state/store/orchestrator | token/gravação ou transição de tenant | API-003; alvos survey e tenant-offboarding |
| 11 facades Option-A e tipos `dr::drill` | módulo local → crate fornecedora | reexport ou dependência | API-002; REL-008; `corelink-failover-router` fornece `Region`/`ResidencyGraph` |
| `dt-cli`, `dt-reconcile` | manifesto workspace → bin externo | name/path do bin | `src/dt.rs`; não entram na biblioteca |

Invariante falsificável: alteração de uma das famílias muda ou preserva os
contratos locais indicados, verificável por leitura da fonte e alvo declarado;
nenhuma linha afirma execução ou completude de consumidores.

<a id="rel-001"></a>
### REL-001 — Manifesto → biblioteca

**Tipo:** build, consumidor→provedor. **Superfície:** package e `src/lib.rs`. **Ativação:** seleção da biblioteca. **Contrato:** módulos públicos formam namespace. **Efeito:** remoção quebra import correspondente. **Falha:** caminho ausente. **Contenção:** compilação futura. **Validação:** SOURCE, não executada. **Coordenação:** owner local. [Relation index](#b03)


<a id="rel-002"></a>
### REL-002 — Biblioteca → pré-checagem dry-run

**Tipo:** API, consumidor→provedor. **Superfície:** `admin::dry_run::{dual_approval_preflight,is_dry_run_short_circuit}`. **Ativação:** import. **Contrato:** rejeição interrompe. **Efeito:** somente valor local. **Falha:** mudança altera classificação. **Contenção:** INV-001. **Validação:** alvo declarado. **Coordenação:** owner local. [Relation index](#b03)


<a id="rel-003"></a>
### REL-003 — Pré-checagem → alvo de propriedade

**Tipo:** test, consumidor→provedor. **Superfície:** `admin_dry_run_prop_dual_approval_invariants`. **Ativação:** seleção explícita. **Contrato:** cobre insuficiência, duplicidade e autoaprovação. **Efeito:** falha é resultado do alvo. **Contenção:** não muda fonte. **Validação:** SOURCE somente. **Coordenação:** owner local. [Relation index](#b03)


<a id="rel-004"></a>
### REL-004 — Manifesto → bin RB-FM-201

**Tipo:** build, consumidor→provedor. **Superfície:** `rb_fm_201_dry_run` e path. **Ativação:** seleção explícita. **Contrato:** name/path existem no manifesto. **Efeito:** troca de path impede seleção. **Contenção:** revisar `[[bin]]`. **Validação:** SOURCE. **Coordenação:** owner local. [Relation index](#b03)


<a id="rel-005"></a>
### REL-005 — Manifesto → bin RB-FM-205

**Tipo:** build, consumidor→provedor. **Superfície:** `rb_fm_205_dry_run` e path. **Ativação:** seleção explícita. **Contrato:** name/path existem no manifesto. **Efeito:** troca de path impede seleção. **Contenção:** revisar `[[bin]]`. **Validação:** SOURCE. **Coordenação:** owner local. [Relation index](#b03)


<a id="rel-006"></a>
### REL-006 — Manifesto → bin RB-FM-206

**Tipo:** build, consumidor→provedor. **Superfície:** `rb_fm_206_dry_run` e path. **Ativação:** seleção explícita. **Contrato:** name/path existem no manifesto. **Efeito:** troca de path impede seleção. **Contenção:** revisar `[[bin]]`. **Validação:** SOURCE. **Coordenação:** owner local. [Relation index](#b03)


<a id="rel-007"></a>
### REL-007 — Manifesto → verificador CLI

**Tipo:** build, consumidor→provedor. **Superfície:** `corelink-supply-verify` e `src/supply_chain/verify/bin/cli.rs`. **Ativação:** seleção explícita. **Contrato:** name/path são pareados. **Efeito:** path inválido bloqueia alvo. **Contenção:** revisão SOURCE. **Validação:** não executada. **Coordenação:** owner local. [Relation index](#b03)


<a id="rel-008"></a>
### REL-008 — Fachada → fornecedor reexportado

**Tipo:** reexport, consumidor→provedor. **Superfície:** `pub use corelink_*::*`. **Ativação:** import pela fachada. **Contrato:** símbolos pertencem ao fornecedor. **Efeito:** mudança do fornecedor pode alterar alias. **Contenção:** editar no owner do fornecedor. **Validação:** SOURCE. **Coordenação:** owner externo à crate. [Relation index](#b03)


<a id="b04"></a>
## B04 — Propagação transitiva

| Destino | Caminho | Condição | Efeito | Contenção |
|---|---|---|---|---|
| import local | REL-001 | módulo selecionado | nome não resolve | API-002 |
| predicado | REL-002 → REL-003 | alvo escolhido | contraexemplo falha | INV-001 |
| fonte de bin | REL-004, 005, 006 ou 007 | bin escolhido | seleção aponta a path | manifesto |
| fornecedor | REL-008 | alias importado | mudança chega ao alias | owner correto |

Invariante falsificável: não se afirma caminho runtime; os caminhos terminam em namespace, alvo ou fronteira de fornecedor.

<a id="b05"></a>
## B05 — Mudança → impacto → validação

| Mudança | IDs | Impacto | Validação | Recuperação |
|---|---|---|---|---|
| mover módulo | API-002, API-003, REL-001 | imports e contrato da família | diff de `lib.rs` e submódulo | reverter patch |
| mudar contrato local | API-003 e família em B03 | tipo, estado ou fluxo local | alvo declarado correspondente | reverter patch |
| mudar predicado | API-001, INV-001/002, REL-002/003 | classificação | selecionar alvo | reverter patch |
| renomear bin | REL-004..007 | seleção | comparar name/path | restaurar bloco |
| alterar alias | REL-008 | fornecedor | revisão coordenada | restaurar alias |

<a id="b06"></a>
## B06 — Cobertura e desconhecidos

| População | Descobertos | Documentados | Excluídos | Desconhecidos |
|---|---:|---:|---:|---:|
| bins | 4 | 4 | 0 | 0 estáticos |
| testes | 26 `[[test]]` | 26 nomes agrupados abaixo; 1 relação detalhada | 0 nomes | resultados |
| relações prioritárias | 8 | 8 | 0 | censo dos 98 arquivos |
| famílias locais | 15 crates absorvidas | 11 grupos contratuais em API-003 | detalhes de outros símbolos | usos completos |
| tenants externos | 13 | 11 facades + 2 bins reconhecidos em R03 | implementação externa | resolução e chamadas |

Targets declarados por família (manifesto; seleção declarada, não resultado):

| Família | Nomes `[[test]]` |
|---|---|
| admin | `admin_api_e2e_admin_dual_approval`, `admin_api_cross_wi_integration_s13`, `admin_dry_run_prop_dual_approval_invariants` |
| config | `config_api_adversarial`, `config_api_prop_mfa_freshness` |
| deploy | `deploy_prop_verify`, `deploy_adversarial`, `deploy_chaos` |
| DR | `dr_drill_prop_dr_drill`, `dr_backup_verify_prop_backup_verify`, `dr_backup_verify_sli_binding` |
| Drata | `drata_proptest_idempotency`, `drata_wiremock_drata_client` |
| migrations | `migrations_d1_migration_integration`, `migrations_prop_migration_additivity` |
| oncall | `oncall_pagerduty_events_http`, `oncall_prop_oncall` |
| rotation | `rotation_worker_prop_rotation`, `rotation_worker_adversarial` |
| supply chain | `supply_chain_policy_prop_cargo_deny`, `supply_chain_policy_adversarial_dep_policy`, `supply_chain_policy_e2e_dependabot`, `supply_chain_verify_prop_verify`, `supply_chain_verify_adversarial` |
| survey | `survey_prop_survey` |
| tenant offboarding | `tenant_offboarding_prop_tenant_offboarding` |

Limitação material: não há enumeração de relações para os demais arquivos do
conjunto de 98. Desconhecidos: grafo resolvido, consumidores inversos e
execução. Contagens e nomes não provam completude nem resultado.

[Referência](REFERENCE.md#r01) · [Manutenção](MAINTENANCE.md#m01) · [Início](#b01)
