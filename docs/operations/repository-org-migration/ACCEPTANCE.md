# Gates e registro de aceitação

Este documento é um contrato de execução, não um certificado preenchido. Na entrega do kit, **todos os gates de transferência permanecem não aprovados**. O destino não foi informado, o catálogo ainda não foi adotado em produção e nenhum canário pós-corte foi executado.

## Matriz de gates

| Gate | Antes/depois | Evidência mínima | Condição de parada |
| --- | --- | --- | --- |
| G01 — destination authority | Antes | Login/ID canônico, organização real, poder de receber, política de saída, ausência de conflito de nome/fork demonstrada com acesso adequado. | ID desconhecido, 404 ambíguo, conflito, autorização ausente. |
| G02 — private access and governance | Antes | Diff de acesso/plano/rulesets/teams/environment/Actions; revisão de permissões padrão e enforcement disponível. | Acesso privado ampliado sem aprovação ou controle obrigatório indisponível. |
| G03 — portability patches | Antes | Disposições M01–M12/população, PRs/head exato, testes, revisão independente e CI de preparação verde. | Literal ativo sem disposição, consumidor não testado, patch quebra a origem. |
| G04 — Apps and runner handoff | Antes | Instalação/owner/IDs e contrato com Runners; canário isolado do destino; handoff Workspaces; instâncias de runner inventariadas. | Label sem capacidade demonstrada, mapping/instalação indefinido, dependência silenciosa. |
| G05 — credentials and integrations | Antes | Nome/scope/consumidor/provider/owner/prova por credencial; integração Git e pacote/registry por fornecedor. | Apenas presença do nome, scope desconhecido, token ampliado ou fornecedor sem verificação. |
| G06 — signing and OIDC preparation | Antes | Política antiga/nova independente, issuer/audience/workflow/digests, fixtures positivas/negativas e plano de prova pós-corte. | Política depende do input sob verificação, confiança aberta, histórico reescrito. |
| G07 — freeze and recovery | Antes | FREEZE_SHA, refs/asset hashes, runtime invariants, backups recuperáveis, writers drenados, limites numéricos e recuperação aprovada. | Drift, writer inesperado, restauração sem prova ou freeze com evidência vencida. |
| G08 — explicit transfer authorization | Antes | Aprovação específica liga IDs/owner/FREEZE_SHA/hash do manifesto/janela/operador/revisor. | Aprovação só para documentação/PR, destino trocado, estado alterado. |
| G09 — post-transfer identity | Depois | Readback por ID e caminho; owner/private/name/refs/assets/settings corretos, sem mudança cloud. | Só HTTP 202/redirect, ID diferente, perda de objetos ou divergência sem explicação. |
| G10 — behavior and observation | Depois | Runs do server no novo owner, App→runner→job, claims/signature, provas API/auth/UI/CLI/irmãos e observação completa. | Apenas teste local, fila pendente, canário com efeitos de produção ou período não coberto. |

`PASS`: evidência satisfaz o critério. `FAIL`: critério violado. `UNVERIFIED`: sem prova suficiente. `N/A`: superfície demonstradamente ausente/inaplicável, com justificativa e revisor. N/A não serve para mascarar erro de permissão. G09/G10 não podem passar antes do corte. G01–G08 precisam passar antes dele, sem exigir circularmente um token do server já transferido.

## Casos de aceitação da preparação

| Caso | Resultado esperado |
| --- | --- |
| Destino sintético válido em plano offline | JSON `PLANNING_ONLY_NOT_AUTHORIZED`; todos os gates UNVERIFIED; nenhuma chamada GitHub. |
| Owner vazio, malformado, mesma org por caixa ou owner ID repetido | Erro; nenhum plano de transferência válido. |
| Transfer scope inclui runners/workspaces/CLI ou visibilidade pública | Erro de contrato. |
| Repo ID diferente, URL antiga redireciona para outro owner | Coletor interrompe antes de consultar settings da organização errada. |
| Página de API retorna 403/404/409/500, timeout ou schema inválido | UNVERIFIED, não lista vazia e não aprovação. |
| Mais de uma página de secrets/refs/workflows | Retenção de todas as páginas; comparar conjuntos, não apenas primeira página. |
| Variável com valor sensível, webhook config ou key material | Relatório contém somente campos permitidos; valor não aparece. |
| Checkout sujo ou arquivo não rastreado com segredo | Scan usa commit indicado e não lê ou altera os arquivos locais. |
| Symlink para fora do checkout | Hash do blob link, sem seguir o alvo. |
| Binary, LFS pointer, gitlink ou blob acima do limite | Cobertura parcial/skip explicitado, nunca alegação de conteúdo examinado. |
| Mudança de arquivo entre dois commits | Delta identifica o caminho; resultado continua REVIEW_REQUIRED. |

## Casos de aceitação da transferência futura

| Caso | Resultado esperado |
| --- | --- |
| Server vai ao destino, irmãos continuam na origem | Contratos de acesso/runner/API funcionam; nenhum owner externo muda implicitamente. |
| Release pipeline monta origem nova | Assets continuam na CLI configurada; token mantém escopo mínimo. |
| Bundle histórico inventariado | Verificação com política histórica vinculada a refs/digests aprovados; bytes iguais. |
| Artefato canário novo | Identidade nova, issuer/audience/workflow/ref/digests válidos e verificação independente. |
| Mesmo nome com repo ID diferente; owner/workflow/ref não autorizado | Negação explícita; nenhum fallback para confiança mais ampla. |
| Transferência responde 202 ou timeout | Readback determina estado; não há retry cego. |
| Job canário se mantém queued | G10 não passa; investigar App/fabric/label sem mudar para hosted pago. |
| Retomada com secret de mesmo nome, valor/escopo inválido | Teste do consumidor detecta falha; presença de nome não aprova G05/G10. |
| Endpoint retorna 200 com semântica/versão errada | Falha; resultado do contrato importa, não só status HTTP. |
| Shell/script gera tag, publish, deploy ou migração durante canário | Ensaio inválido; suspender e tratar efeito operacional, sem considerá-lo sucesso. |
| Namespace antigo não pode ser reutilizado | Recuperação operacional no destino; transfer-back não é presumido. |
| Evidência pertence a SHA anterior ao patch final | Reexecutar provas afetadas antes do aceite. |

## Registro de execução

Criar uma cópia em diretório de evidências da campanha, sem sobrescrever o snapshot desta entrega. Não preencher valores fictícios para tornar o registro verde. O template abaixo é documental: nenhuma ferramenta o interpreta como autorização automática.

```yaml
schema_version: 1
campaign: null
status: not_started
source_repository: HuGR-Labs/corelink-server
repository_id: 1232040291
source_owner_id: 311862110
target_owner: null
target_owner_id: null
freeze_sha: null
manifest_sha256: null
operator: null
independent_reviewer: null
approval_reference: null
approval_utc: null
window_start_utc: null
window_end_utc: null
max_ci_control_plane_unavailability_minutes: null
max_canary_queue_minutes: null
observation_hours: null
allowed_cost: 0
canary_workflow_path_and_sha: null
runtime_read_probes: []
authorized_test_tenant: null
writers_paused_with_previous_state: []
ref_and_asset_manifest_reference: null
backup_and_restore_proof_reference: null
app_runner_workspaces_handoff_references: []
gates: [] # Uma entrada para cada G01..G10: status, owner, UTC, evidence, reason.
transfer_request_utc: null
transfer_response_reference: null
canonical_readback_reference: null
post_transfer_run_references: []
observation_reference: null
exceptions: [] # Justificativa, evidência, owner e decisão do revisor.
restored_workflow_states: []
closed_utc: null
```

## Encerramento

O responsável deve revisar explicitamente todos os dez gates e as exceções. Ausência de uma entrada equivale a UNVERIFIED. Fechar apenas depois de reconciliar inventários, comprovar comportamento e período de observação, restaurar estados pausados, publicar evidências e limpar somente o trabalho da campanha. O registro B-374 do kit não é autorização ou atestado de transferência.
