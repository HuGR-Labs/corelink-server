# Runbook de preparação e execução futura

Este runbook descreve controles; não executa operação externa. Nomear operador, autoridade, revisor independente, owner de observação, janela UTC, limites de impacto/custo, TTL das evidências e decisor de recuperação antes de qualquer GO.

## Modos

| Modo | Entrada | Saída | Mutação permitida |
| --- | --- | --- | --- |
| audit | Commit SHA e perfil | Relatório com cobertura/opaque explícitos | Nenhuma |
| plan | Perfil; destino pode ser null | Plano não autorizado, gates UNKNOWN | Nenhuma |
| prepare | Censo, owners e política aprovados | PR de preparo, projeções e provas | Apenas allowlist abaixo |
| rehearse | Topologia isolada, orçamento e cleanup | Resultado e negativas | Somente fixtures autorizadas e isoladas |
| cutover | G00–G11 atuais e GO explícito G12 | Solicitação única e estado `UNKNOWN` ou readback | Uma transferência explicitamente autorizada fora destas ferramentas |
| verify | Repo ID, refs, artefatos, runs e janela | G13–G19 após observação | Reads; retomada seletiva sob autorização própria |

`cutover`/`verify` são procedimentos humanos futuros, não CLI implementada. Uma solicitação com timeout/202 sem readback resulta em `UNKNOWN_AFTER_REQUEST`: conter, ler por ID e caminhos, não repetir.

## Preparação permitida e proibida

Allowlist preparatória: documentação; relatório de auditoria offline; fixture de teste isolada; atualização de perfil numa branch revisada; PR preparatório. Toda alteração permanece reversível e sem tocar produção.

Proibido: transferir repo; mirror push; reescrever refs, tags, assinaturas ou histórico; deploy/publicar; dispatch de workflows produtivos; restart de fleet por wildcard; tocar cloud/dados/tenants; mudar nome/visibilidade; transferir peers; rotação ampla de credenciais; `git clean` ou `reset --hard`. O checker rejeita divergência da allowlist fixa e qualquer mutação proibida registrada.

## Matriz de control-surface (estado de entrada)

Inventariar individualmente cada linha antes de G02–G03. Todas começam `UNKNOWN`, owner `TBD`, evidência `null`; substituir por owner nomeado, leitura live autenticada/paginada, timestamp, identidade e evidência privada sem segredos. `source-inventory.json` cobre somente objetos do Git tree e não cobre nenhuma superfície live/control plane desta matriz.

| Superfície | Status | Owner | Evidência/readback requerida |
| --- | --- | --- | --- |
| CODEOWNERS, branch protection/rulesets, bypass, required checks | `UNKNOWN` | `TBD` | `null`; conteúdo efetivo e settings source/destination |
| Base permission, collaborators, teams, reviewers e acesso efetivo | `UNKNOWN` | `TBD` | `null`; principal IDs, grants completos e negativas |
| Org/repo tokens, PAT policies, SSO enforcement/approval | `UNKNOWN` | `TBD` | `null`; scopes por metadata, sem valores de credenciais |
| GitHub Apps, installations, permissions, webhooks e OAuth/Git integrations | `UNKNOWN` | `TBD` | `null`; App/installation IDs, owners e endpoint consumers |
| Actions/workflows, environments, approvals, secrets/vars names/scopes | `UNKNOWN` | `TBD` | `null`; censo por ref/job, sem exportar valores |
| Hosted/self-hosted runners, groups, labels, images, services e fleet | `UNKNOWN` | `TBD` | `null`; runner→repo/job, owner, canário isolado |
| Actions schedules, cron, launchd, timers, queues e producers externos | `UNKNOWN` | `TBD` | `null`; owner/host, pause seletivo, drain e readback |
| Pages, wiki, Projects, Discussions, domains, redirects | `UNKNOWN` | `TBD` | `null`; settings, audience e conteúdo/redirect por ID |
| GHCR/container/npm/PyPI/Go registries, packages e caches | `UNKNOWN` | `TBD` | `null`; package IDs, ACLs, digest e consumer negatives |
| Git refs/branches/tags/signatures, releases/assets/LFS/archives | `UNKNOWN` | `TBD` | `null`; manifests por ID/digest, parity e restore |
| OIDC claims/policies, signing, certs, attestations e SLSA provenance | `UNKNOWN` | `TBD` | `null`; claims reais sem JWT, policies e bundles |
| Cloudflare account/zones, DNS, Workers, R2, D1, KV, Durable Objects/runtime | `UNKNOWN` | `TBD` | `null`; resource IDs, versions/digests e read-only probes |
| API/Auth/UI, Clerk issuer, Stripe/billing, tenants/customer data | `UNKNOWN` | `TBD` | `null`; contracts/issuer IDs, isolamento e zero export |
| Webhook/event consumers, CI clients, peer repos e callbacks | `UNKNOWN` | `TBD` | `null`; consumer owners, handoffs e topology proofs |
| Credentials/read grants, rotation consumers, source bridge, backup/DR | `UNKNOWN` | `TBD` | `null`; principal inventory, revocation probes, bridge readback e restore proof |

Negativa, ausência no Git tree, nome de secret ou status HTTP isolado não prova configuração live; qualquer lacuna mantém gate `UNKNOWN`.

## Sequência

1. Confirmar instruções do repo, issue #1702, repo ID estável, source/destination IDs, audiência privada, autoridade e escopo `server` apenas. Erro, `UNKNOWN`, conflito ou identidade parcial bloqueia.
2. Fixar candidate SHA/tree; rodar auditoria em commit exato e revisar todos os achados/opacos. Completar censo de branches/releases suportadas, metadata paginada e consumidores fora do repo. Classificar ocorrências individualmente; negar acesso não significa ausência.
3. Inventariar controls, environments, Apps, runners, refs, releases/assets/LFS, peers, packages, Git integrations e credenciais por nome/scope/consumer/provider. Não exportar valores. Obter handoffs dos owners de Runners, Workspaces e CLI.
4. Implementar preparação em PRs revisados: projeções explícitas, guardas, compatibilidade de workflows, signers, consumers Rust/Python/shell e testes negativos. Provar origem e as três topologias; conservar check contexts e artefatos históricos.
5. Ensaiar em sistema/tenant descartável, com efeitos, custo, tempo, negatives, abort e cleanup aprovados por escrito. Testar API/Auth/UI/CLI conforme contrato; nenhum smoke pode produzir release, deploy, tag ou dado de cliente.
6. Preparar backup que cubra Git, refs, LFS, assets e settings aplicáveis; executar restore; comparar manifests/IDs/digests. Definir recuperação para frente acessível fora do Actions afetado. Transfer-back nunca é presumido.
7. Antes do GO, congelar writers, drenar jobs/filas, reconciliar branches/tags/assets/LFS/backup/restore, confirmar FREEZE_SHA, fences de source e destination, rotação/scope de credenciais e controles. Qualquer writer residual ou drift invalida o snapshot.
8. Recoletar condições atuais; G12 GO é emitido só após G00–G11, para escopo/hash/janela/operador/revisor exatos, e dentro da janela vigente. Mudança de SHA, refs, permissões, credencial, peer, decisão ou janela exige novo aceite. Não pré-aprovar.
9. Caso a autoridade realize uma transferência autorizada, enviar uma solicitação deliberada. Ler repo por ID até convergência com limite definido; não retry cego. Verificar owner/private/name/ref/objeto por leituras independentes.
10. Retomar somente writers registrados, em ordem revisada. Provar execução real do server no destino, runner/App/job, claims e signing, CLI, API/auth/UI e peers. Observar todas as lanes/jobs e duração/limites aprovados antes de G19.

## Pausa e recuperação

`UNKNOWN`, resultado conflitante, evidência vencida, dados expostos, writer inesperado, parity quebrada, identity errada, queue acima do limite, autorização inválida ou split-brain: interromper, manter fences e não avançar. Se a requisição já foi enviada, consultar identidade/estado por repo ID sem reenviar.

Antes do corte, corrigir preparo por PR e restaurar somente controles pausados que possam ser restaurados com segurança. Após transferência, preservar runtime e dados existentes; recuperar para frente por procedimento aprovado sem abrir trust ou recriar namespaces. Reversão exige nova decisão, prova de destino/origem e plano de continuidade; retorno automático não existe.

G19 fecha somente com inventário completo e reconciliação de grants de leitura temporários de humans/collaborators/teams/apps/tokens; cada grant obsoleto de source/destination revogado com probe negativo e evidência; cada reader retido individualmente identificado com owner, motivo, expiry futuro e reviewer distinto; nenhum secret value armazenado; e source bridge removido com readback `OBSERVED_ABSENT` para o mesmo repo ID. Também exige gates/exceções com owner/reviewer/evidência atuais, observação concluída, writers reconciliados, controles pausados restaurados seletivamente e recibo privado/versionado lido. Fechamento deste kit não fecha #1702.
