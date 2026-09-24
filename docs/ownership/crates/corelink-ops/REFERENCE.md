---
schema: corelink-ownership/1.1
document: reference
package: corelink-ops
manifest: crates/corelink-ops/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: H
state: author_validated
evidence_set: source-inspection-6be030999
---

# corelink-ops — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) · [Contratos](#r04) · [Invariantes](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Evidências](#r08)

<a id="r01"></a>
## R01 — Identidade e função

`corelink-ops` é uma superfície de importação Rust: `lib.rs` declara módulos físicos e fachadas que reexportam crates declaradas no manifesto. A análise é SOURCE na baseline indicada; não certifica resolução, execução nem comportamento externo. Invariante falsificável: remover uma linha `pub mod` de `lib.rs` torna aquele caminho indisponível ao compilador.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | `corelink-ops` / `crates/corelink-ops/Cargo.toml` |
| Targets | biblioteca, três bins dry-run, um verificador e 26 `[[test]]` |
| Papel | agregador híbrido: módulos físicos e aliases |
| Implementado / wired / runtime | yes / unknown / unknown; somente SOURCE |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Implementação | Contrato | Escalonamento |
|---|---|---|---|
| Módulos físicos | `crates/corelink-ops/src/` | este package | revisão local |
| Fachadas `pub use` | crate alvo | crate alvo | owner do fornecedor |
| Bins dry-run | este package | manifesto e fonte do bin | revisão local |

Invariante falsificável: módulo que contém somente `pub use corelink_*::*` não ganha lógica própria. A rota canônica é o [OKF SRE operations hub](../../../knowledge/ops/sre-operations-hub.md); não há outra rota declarada aqui.

<a id="r03"></a>
## R03 — Mapa da implementação

`lib.rs` declara 19 módulos raiz. O código local organiza-se em 15 famílias
absorvidas; os caminhos externos abaixo são reexportações, não implementação
local. Este mapa agrupa os itens públicos por fronteira de revisão, sem
pretender enumerar cada declaração.

| Caminho canônico | Superfície e contrato local de referência | Classe |
|---|---|---|
| `admin::api`, `admin::dry_run` | pipeline de política/admin; classificação `DualApprovalPreflight` e interrupção para pré-checagem rejeitada | local |
| `admin::handler`, `admin::dual_approval` | APIs do handler e gate 2-of-N | facades `corelink-handler-admin`, `corelink-dual-approval` |
| `alerts` | `MultiChannelAlerter`, configuração e canais; envio usa transportes definidos pela interface local | local |
| `chaos` | agenda de experimentos | facade `corelink-chaos-scheduler` |
| `config::api`, `config::durable_object` | API/configuração local; DO reexportado | local + facade `corelink-config-do` |
| `deploy` | tipos, verificação, worker/audit; `DeployVerifier` é a fronteira de verificação | local |
| `dr::backup_verify`, `dr::drill` | verificação de backup e execução/agendamento de drills; `Region` e `ResidencyGraph` são reexportados de `corelink-failover-router` | implementação local + tipos de fornecedor |
| `drata` | registros, redaction, retry, stream, ledger e runner de sync | local |
| `dt::webhook` | webhook | facade `corelink-dt-webhook` |
| `enterprise` | inquiry | facade `corelink-enterprise-inquiry` |
| `migrations` | localização, ordenação, split/rewrite de SQL e replay | local |
| `oncall` | rotação, turnos, paging, severidade, fadiga e ledger | local |
| `rotation::worker`, `rotation::adapters` | worker local; adapters por provider reexportados | local + facade `corelink-rotation-adapters` |
| `runbook` | tracker de drill | facade `corelink-runbook-tracker` |
| `slack`, `statuspage` | clientes externos | facades `corelink-slack-real`, `corelink-statuspage-real` |
| `supply_chain::policy`, `supply_chain::verify` | policy engine e verificação SBOM/attestation, incluindo um bin | local |
| `survey` | tipos, tokens assinados, signer e recorder | local |
| `tenant_offboarding` | estado, store, auditoria e orchestrator | local |
| `terraform` | consumer de drift | facade `corelink-terraform-drift-consumer` |

O total de 15 crates absorvidas e 13 tenants ainda externos vem do mapa
de absorção em `src/lib.rs`: a lista inclui 11 facades de biblioteca e os bins
`corelink-dt-cli` e `corelink-dt-reconcile`, que não são reexportáveis.
`dr::drill` também reexporta `Region` e `ResidencyGraph` de
`corelink-failover-router`, uma aresta de tipo adicional fora dessa lista de
tenants. As famílias locais têm superfícies públicas próprias; API-003 seleciona
contratos para revisão, não todos os símbolos.

`admin::dry_run` classifica preflight de dupla aprovação; os quatro targets de
bin são entradas distintas. O manifesto também declara 26 alvos `[[test]]`;
nomes de alvo não provam execução.

Invariante falsificável: a listagem falha se caminho citado não existir ou a forma `pub use` indicada desaparecer. Inventário limitado à fonte local e manifesto, sem inferir chamadas.

**Âncoras contratuais locais selecionadas** (não é censo dos símbolos):

| Caminho / símbolos-âncora | Contrato observável na fonte | Invariante falsificável / prova |
|---|---|---|
| `admin::dry_run::{DualApprovalPreflight,dual_approval_preflight,is_dry_run_short_circuit}` | resultado local de pré-checagem | rejeição interrompe; `src/admin/dry_run.rs`, INV-001/002 |
| `alerts::{MultiChannelAlerter,AlerterConfig}` | política de configuração e canais | mudança de canal/config muda fan-out especificado; `src/alerts.rs`, submódulos |
| `deploy::DeployVerifier` | fronteira para verificação de deploy | mudança de contrato exige revisar tipos/verifier/audit; `src/deploy.rs` |
| `dr::{backup_verify,drill}` | snapshots/resultados e ciclo de drill | alteração de estado/outcome afeta contratos locais; subárvores `src/dr/` |
| `drata::{EvidenceRecord,SyncRunner,RetryPolicy,IdempotencyLedger}` | registro e sync de evidência | retry/ledger/redaction delimitam repetição e dados expostos; `src/drata.rs` |
| `migrations::{split_statements,rewrite_d1_to_sqlite,replay_all}` | replay local e relatório | statement split/rewrite ou ordem muda resultado do replay; `src/migrations.rs` |
| `oncall::{Rotation,Shift,PageEvent,RotationLedger}` | rotação e eventos de paging | cap de turno e janela de proteção são constantes locais; `src/oncall.rs` |
| `rotation::worker` | state machine, orchestrator e rollback | transição/rollback exige revisão conjunta; `src/rotation/worker/` |
| `supply_chain::{policy,verify}` | decisão de policy e verificação de artefatos | policy/resultado mudam aceitação local; `src/supply_chain/` |
| `survey::{SurveyLinkSigner,SurveyResponseRecorder,SurveyToken}` | token assinado e gravação de resposta | alteração do token/TTL ou recorder afeta aceite/idempotência; `src/survey/` |
| `tenant_offboarding::{TenantOffboardingState,orchestrator,store}` | estados e transições de offboarding | enum/transição alterada muda caminhos permitidos; `src/tenant_offboarding/` |

Essas âncoras cobrem famílias contratuais para orientar revisão; não atestam
integração, persistência externa, execução, ou completude dos símbolos públicos.

<a id="r04"></a>
## R04 — Contratos públicos

Índice: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003).

<a id="api-001"></a>
### API-001 — Pré-checagem de dupla aprovação

**Símbolos exatos:** `dual_approval_preflight`, `is_dry_run_short_circuit`, `DualApprovalPreflight`. **Entrada:** UUID solicitante e slice de UUIDs. **Saída:** aceite, insuficiência, duplicidade ou autoaprovação. **Pós-condição:** rejeição torna a interrupção verdadeira. **Compatibilidade:** enum `#[non_exhaustive]`. **Prova:** `src/admin/dry_run.rs`; INV-001 e alvo `admin_dry_run_prop_dual_approval_invariants`.

[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Caminhos canônicos de módulo

**Símbolos exatos:** módulos públicos de `src/lib.rs`, incluindo `admin`, `oncall`, `rotation`, `survey`, `supply_chain` e `tenant_offboarding`. **Entrada:** import Rust. **Saída:** nome resolve ou falha. **Efeito:** nenhum demonstrado pelo contrato de namespace. **Compatibilidade:** aliases mantêm nome do fornecedor. **Prova:** `src/lib.rs`; REL-001 e REL-008.

[Índice de contratos](#r04)
[↩](#r01)

<a id="api-003"></a>
### API-003 — Contratos locais por família

Agrupa por família os contratos locais selecionados no inventário R03, incluindo
tipos, traits, decisões e invariantes source-grounded. A seleção é uma rota de
revisão, não um censo. Não comprova integração, persistência externa ou execução.
Para facades, ver API-002 e REL-008.

[Índice de contratos](#r04) · [Mapa de módulos](#r03)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

Índice: [INV-001](#inv-001) · [INV-002](#inv-002) · [FLOW-001](#flow-001).

<a id="inv-001"></a>
### INV-001 — Rejeição sempre interrompe o plano local

**Regra:** todo resultado diferente de aceite torna `is_dry_run_short_circuit` verdadeiro. **Imposição:** negação de `matches!(Accepted)`. **Violação falsificável:** rejeição com retorno falso. **Prova:** `src/admin/dry_run.rs`; alvo declarado cobre o predicado. Execução: não realizada.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Aceite exige dois UUIDs distintos e não autoaprovadores

**Regra:** aceite só ocorre com os dois primeiros UUIDs distintos e diferentes do solicitante. **Imposição:** verificações ordenadas da função. **Violação falsificável:** slice que aceita zero, um, duplicado ou solicitante. **Prova:** `src/admin/dry_run.rs`; alvo declarado. Estado persistente próprio: nenhum nessa função.

[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Classificação local de pré-checagem

1. Recebe solicitante e slice.
2. Menos de dois retorna insuficiência.
3. Dois primeiros iguais retornam duplicidade.
4. Solicitante entre eles retorna autoaprovação.
5. Caso contrário retorna aceite.
6. O predicado classifica o resultado.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Fonte | Default | Leitura | Condição | Efeito verificável |
|---|---|---|---|---|
| feature `default` | lista vazia | seleção Cargo | padrão | não ativa feature própria |
| feature opt-in `production` | lista vazia | seleção Cargo | explícita | declaração, sem execução afirmada |
| `cfg(target_arch = "wasm32")` | `uuid` com `js` | resolução | alvo wasm32 | dependência adicional declarada |
| `PROPTEST_CASES` | `2048` em um alvo | execução de teste | variável parseável | substitui contagem |

Invariante falsificável: manifesto lista exatamente quatro bins e não atribui `required-features` a eles. Seleções de target e feature são contrato de manifesto; não foram resolvidas nesta entrega.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal | Causa limitada | Estado após falha | Diagnóstico |
|---|---|---|---|
| `InsufficientApprovers` | slice curto | nenhuma mutação nesta função | API-001 |
| `DuplicateApprovers` | dois primeiros iguais | nenhuma mutação nesta função | API-001 |
| `SelfApproval` | solicitante coincide | nenhuma mutação nesta função | API-001 |
| erro de import | caminho ausente | não há resolução de símbolo | API-002 |

Invariante falsificável: os três resultados precedem aceite nas condições descritas. Não há telemetria ou observação de runtime afirmada por esta referência.

<a id="r08"></a>
## R08 — Verificação e evidências

| Item | Fonte baseline | Método | Resultado / limite |
|---|---|---|---|
| API-001 / INV-001 | `src/admin/dry_run.rs` | SOURCE | implementado; não executado |
| API-002 | `src/lib.rs` | SOURCE | módulos declarados; não resolvido |
| targets / features | `Cargo.toml` | SOURCE | declarados; não selecionados |
| contrato de teste | manifesto e alvo de propriedade | SOURCE | declarado; não executado |

Invariante falsificável: mudar caminhos ou símbolos invalida a evidência SOURCE. Desconhecidos: resolução por plataforma, execução dos bins, consumidores inversos e comportamento fora do checkout. [Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).

[Voltar ao início](#r01)
