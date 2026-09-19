# Pacotes de trabalho

Esta entrega implementa o kit de documentação/descoberta de WP-00. Os demais pacotes descrevem a execução futura; nenhum está implicitamente concluído. Os cinco axiomas se aplicam por pacote. Links de evidência devem apontar a SHA/run/objeto exato, não apenas à main mutável.

## WP-00 — Kit de descoberta e desenho

**Owner:** mantenedor do server. **Dependências:** acesso read ao repo e metadados; árvore isolada. **Entrega:** mapa, runbook, skill, manifesto, inventários, ferramentas de coleta/plano e testes.

| Axioma | Critério |
| --- | --- |
| Completeness criteria | Enumerar a árvore e limitações; cobrir M01–M12, fronteiras GitHub/cloud/dados e os três repos externos; entregar navegação, fontes e todos os gates. |
| Success criteria | Outro operador reproduz scan/snapshot/plano sem transferir ou alterar produção; input incompleto não produz autorização. |
| Quality standards | Evidência por commit/API, outputs redigidos, paginação, testes negativos, self-review identificado como tal. |
| Definition of Done | Testes focais e integridade documental passam; conteúdo está versionado/publicado; estado de CI/revisão e limites explicitados. |
| Invariants | Nada de transferência, deploy, valores de credenciais ou edição dos irmãos; manifesto de planejamento não vira configuração de produção. |

## WP-01 — Destino, governança e capacidade

**Owner:** administrador de origem + administrador de destino. **Dependências:** WP-00 e destino identificado. **Gates:** G01/G02.

| Axioma | Critério |
| --- | --- |
| Completeness criteria | Login/ID, plano, SSO/2FA, default access, teams, convidados, regras herdadas, environments, Actions e conflito de nome/fork documentados. |
| Success criteria | Recebimento permitido, acesso privado intencional e proteção realmente suportada; dois caminhos administrativos autorizados conhecidos. |
| Quality standards | Readback autenticado e prova do controle; 403/404 não viram ausência; comparação antes/depois planejada. |
| Definition of Done | Administradores aprovam matriz com evidências atuais, sem unknown aplicável. |
| Invariants | Nome/ID/visibilidade mantidos; sem repo vazio de reserva; sem retirar controles para compensar limitações do plano. |

## WP-02 — Portabilidade do código e da confiança

**Owner:** mantenedor server + revisor de supply chain. **Dependências:** desenho WP-00, política WP-01. **Gate:** G03, parte de G06.

| Axioma | Critério |
| --- | --- |
| Completeness criteria | Todos os consumidores M01–M12 e novas ocorrências classificados; routing, guardas, Rust, evidência, bootstrap, metadata, testes e triggers cobertos. |
| Success criteria | Simulação de owner novo funciona sem mudar CLI/cloud/Go module; continua funcionando na origem; históricos válidos permanecem verificáveis. |
| Quality standards | Catálogo pequeno, adaptadores tipados quando aplicável, testes negativos e revisão independente; política esperada não vem do input sob verificação. |
| Definition of Done | Patches mergeados pelo gate do repo com CI verde no head exato; nenhum literal ativo sem disposição revisada; rollback operacional testado. |
| Invariants | Sem tautologia, wildcard, bypass de ref/event/proteção, reescrita de assinatura/histórico ou mudança silenciosa de trust. |

## WP-03 — Handoffs e credenciais

**Owner:** operador server, responsável Runners/Workspaces e owners das integrações. **Dependências:** WP-01; pode avançar em paralelo a WP-02. **Gates:** G04/G05.

| Axioma | Critério |
| --- | --- |
| Completeness criteria | App/instalação, routing, labels, leases, serviço local, tokens por scope, Git integrations, pacote/registry e contratos inbound/outbound inventariados. |
| Success criteria | Canário isolado prova capacidade do destino; acesso às dependências está resolvido sem transferi-las nesta frente. |
| Quality standards | Prova por consumidor, menor privilégio e apenas nomes/metadados; ausência declarada depende de evidência, não de silêncio. |
| Definition of Done | Handoffs assinados, recoverability de credenciais confirmada e nenhum scope desconhecido no caminho de corte. |
| Invariants | Sem ampliar token da CLI, recriar App, redefinir tenants ou transferir repos irmãos; instalação atual não é tratada como installation ID futuro. |

## WP-04 — Ensaio, identidade e freeze pack

**Owner:** operador de mudança + revisor. **Dependências:** WP-02/WP-03. **Gates:** G06/G07.

| Axioma | Critério |
| --- | --- |
| Completeness criteria | Fixtures/assinaturas, política nova e histórica, canários, plano de pausa, backup não-Git, refs/asset hashes, runtime invariants e recuperação definidos. |
| Success criteria | Ensaio demonstra fluxo seguro; writers drenados; freeze representa o estado real após preparação. |
| Quality standards | Limites numéricos de janela, tempo/filas/erro e custo; metadados frescos; distinção entre canário isolado e server no destino. |
| Definition of Done | Freeze pack íntegro e revisado, produção preservada, nenhum writer inesperado; limite de frescor e rollback operacional aceitos. |
| Invariants | Sem usar tag/release/deploy como smoke; sem cancelamento em massa; monitoramento e backups não são desligados por conveniência. |

## WP-05 — Corte único e readback

**Owner:** operador autorizado e administrador do destino. **Dependências:** WP-04 e autorização específica. **Gates:** G08/G09.

| Axioma | Critério |
| --- | --- |
| Completeness criteria | Registro liga origem/destino/IDs/FREEZE_SHA/manifesto/janela; resposta de transferência e readback por ID/caminhos capturados. |
| Success criteria | Mesmo repo privado no owner correto; refs/objetos/assets correspondem ao freeze, sem alteração de runtime. |
| Quality standards | Uma solicitação, diagnóstico por leitura em estado incerto e comparação de população, não amostras convenientes. |
| Definition of Done | G09 aprovado e estado inequívoco; ainda não significa migração encerrada. |
| Invariants | HTTP 202 não é conclusão; sem retry cego, repos substitutos ou promessa de transfer-back. |

## WP-06 — Comportamento e retomada

**Owner:** operador server + responsáveis por CI e produto. **Dependências:** WP-05. **Gate:** G10.

| Axioma | Critério |
| --- | --- |
| Completeness criteria | Server real no destino: runner classes, App event→job, OIDC/assinatura, histórico, API/auth/UI/CLI e contratos dos irmãos exercitados. |
| Success criteria | Casos válidos passam e identidades inválidas são negadas; produto mantém recursos/estado; retomar somente produtores pausados. |
| Quality standards | Canários sem publish/deploy, semântica de rota identificada, tenant de teste autorizado quando necessário e cleanup comprovado. |
| Definition of Done | Runs e verificações no owner novo aceitos, diferenças resolvidas, retomada registrada e observação iniciada com owner. |
| Invariants | Teste local/HTTP 200/runner online não substituem comportamento; primeiro release real exige autorização própria. |

## WP-07 — Observação e encerramento

**Owner:** responsável pela mudança + revisor independente. **Dependências:** WP-06 e janela definida.

| Axioma | Critério |
| --- | --- |
| Completeness criteria | Todos os gates, jobs periódicos relevantes, exceções, links, remotes, handoffs e evidências reconciliados; backlog e PRs atualizados. |
| Success criteria | Estado sustentado na janela aprovada sem filas/erro/regressão atribuível ao corte e sem acesso privado ampliado involuntariamente. |
| Quality standards | N/A exige justificativa verificável; evidência de head antigo ou ensaio não se passa por prova final. |
| Definition of Done | Aceite independente com UTC/SHA/runs; nenhum unknown aplicável; repositório e worktrees limpos e trabalho publicado. |
| Invariants | Não declarar conclusão enquanto houver prova pendente; histórico preservado; encerramento do kit não é encerramento da transferência. |
