# Contrato operacional e ledger schema 2

## Princípios de decisão

O template começa com G00–G19 `UNKNOWN`. Só `PASS` com evidência vigente pode satisfazer gate; `UNKNOWN`, `BLOCKED`, `FAIL`, ausência, `403`/`404` ambíguo, truncamento, erro de ferramenta ou evidência expirada bloqueiam. `N/A` só é possível em G07/G17 com prova e revisão explícita de não aplicabilidade; nunca encobre falta de acesso.

Cada gate guarda owner, status, referências de evidência com SHA-256 e scope hash, `observed_at`, `valid_until`, reviewer distinto, `reviewed_at` e razão. Gates passam em ordem; provas posteriores não podem preexistir ao evento que provam. O checker valida coerência declarada, não baixa nem autentica provas.

| Gate | Critério mínimo para PASS |
| --- | --- |
| G00 | Identidade por IDs, escopo server-only, destino elegível, autoridade e audiência privada atuais. |
| G01 | Censo schema 2 do candidato, ocorrências/opacos, branches, releases e consumidores completos ou exceções provadas. |
| G02 | Controle de acesso, teams, rulesets, environments, Actions e audiência comparados; não ampliar acesso sem aprovação. |
| G03 | Writers, producers, schedules, runners/serviços e ações com estados de pausa/restauração inventariados. |
| G04 | Handoffs de Runners/Workspaces/CLI com IDs, refs, consumers, owners, janela e compatibilidade. |
| G05 | Portabilidade, projeções, detector anti-drift e negativas integrados e testados no SHA candidato. |
| G06 | Emissão/access OIDC, signing, certificados, consumer e provenance histórica/nova separados e provados. |
| G07 | Applicability/owner/visibilidade da GitHub App reais verificados; N/A requer prova de ausência e reviewer. |
| G08 | Backup Git/LFS/assets/metadata aplicáveis; restore executado; freeze/backup/restore e manifests reconciliados; evidência privada. |
| G09 | Plano testado de recuperação para frente, acesso independente do Actions afetado; transfer-back não presumido. |
| G10 | Ensaio isolado, negativas, orçamento, abort/cleanup e limites representativos documentados. |
| G11 | FREEZE_SHA; source read-only, destination prep-only; writers drenados; zero writers residuais; parity final. |
| G12 | GO atual e explícito, posterior a G11, ligado a IDs, SHA/profile/hash, janela, autoridade, operador e revisor. |
| G13 | Depois de request única, mesmo repo ID converge; readback canônico confirma owner/ID, private/name e estado. |
| G14 | Refs, commits, tags, releases, LFS, assets e metadata reconciliados com freeze/backup. |
| G15 | Jobs reais do server no owner/SHA/fleet esperados; Actions, App e credenciais funcionam sem widening. |
| G16 | CLI/distribution e consumers aceitam artefato canário autorizado; credenciais são rotacionadas/revogadas com mínimo scope. |
| G17 | Applicability e identidade App/fabric pós-corte; se não aplicável, evidência revisada. |
| G18 | API/Auth/UI/runtime/peers aprovados observados; source writers disabled, destino single-writer, reconciliação sem split-brain. |
| G19 | Observação completa; grants temporários de human/collaborator/team/app/token reconciliados, obsoletos revogados com negativas, readers retidos owner/reason/expiry/reviewer individual, source bridge removido e readback `OBSERVED_ABSENT`. |

G13–G19 não podem ser aprovados antes da transferência/observação que comprovam. G12 não é pre-aprovação: sua validade cobre uma janela atual e o issued time vem depois de G11. Uma alteração na fronteira temporal requer nova aprovação.

O record `access_cleanup` de G19 aceita somente referências/metadados não secretos, exige inventário completo, evidência privada e negativas pós-revogação. Retenção exige expiry no futuro e reviewer independente. O source bridge precisa estar removido e lido como ausente para repo ID `1232040291`; resposta de redirect/cache não satisfaz esse readback. O auditor do kit cobre exclusivamente a árvore Git, não o plano live.

## Hashes, validade e tempo

`scope_sha256` é SHA-256 do JSON canônico de `scope`: repo ID/nome, origem/destino e IDs, visibilidade e transfer scope. Cada referência contém URI privada, hash SHA-256, o mesmo scope hash, instante de observação e expiração futura. Gate, GO e request são temporalmente ordenados; timestamps são UTC com timezone. Hash autoconsistente não prova integridade externa nem identidade do aprovador.

O checker mantém os resultados `migration_ready:false` e `authorization_verified:false` em todas as execuções. Exit 0 informa somente consistência estrutural/declarada. Exit 2 indica ledger inválido ou gates bloqueadores. Nenhum código pode transferir ou autenticar por associação a esse exit code.

## Mutação e fences

Preparação permite apenas: `create_documentation`, `create_offline_audit_report`, `create_isolated_test_fixture`, `update_profile_on_reviewed_branch`, `open_preparation_pull_request`. Proibidos: `transfer_repository`, `push_mirror`, reescrever história/assinaturas, deploy/publish, dispatch produtivo, restart amplo de runner, mudanças cloud/tenant, alterar visibilidade/nome, transferir peer, rotação ampla ou `git clean`/`reset --hard`.

Em G11, source é `FROZEN_READ_ONLY`; destination é `PREPARATION_ONLY`; writers/filas são drenados e reconciliados; `FREEZE_SHA` vincula inventário, backup e restore. Nenhum segundo writer pode assumir. A transferência futura é a única mutação de ownership, deliberada uma vez e fora das ferramentas locais do kit. Requisição ambígua não é repetida.

## Continuidade, privacidade e recuperação

Backup prova refs, objetos, LFS, releases/assets e settings aplicáveis por manifestos; restore é testado; source/backup/restore têm paridade no mesmo FREEZE_SHA. Dados de prova ficam privados; nunca exportar tokens, JWTs, chaves, secrets ou dados de tenants. Registrar apenas nome, scope e consumer.

Rollback significa recuperação para frente com credenciais/acesso independentes e procedimento testado. Não promete transfer-back nem namespace disponível na origem. Rotação é por credential/provider/consumer, scope mínimo; verificar consumer novo, revogar antigo quando seguro e não registrar valores. Após cutover, confirmar `source_writers_disabled`, `destination_single_writer`, contagem 1 e reconciliação; ambiguidade, duplicidade ou divergência mantém ambos os lados cercados até resolução aprovada.
