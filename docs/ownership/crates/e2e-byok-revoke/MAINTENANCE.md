---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-byok-revoke
manifest: tests/e2e-byok-revoke/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-byok-revoke-static-source-20260921
---

# e2e-byok-revoke — manual de manutenção

Procedimentos documentais limitados ao harness. Este manual não autoriza Cargo,
provider real, rede, GitHub, CI disparado, deploy ou produção. Fakes locais não
conferem autoridade sobre BYOK/Ops. [OKF verificado](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) é somente rota.

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) · [Testes](#m04) · [Recuperação](#m05) · [Escalonamento](#m06)

<a id="m01"></a>
## M01 — Preparação

**Checkout/ferramentas:** leia o baseline no frontmatter e use o worktree explicitamente designado. Para checker documental, defina CHECKER no ambiente de chamada a um check_docs.py provisionado; não persista caminho temporário da máquina. Python: /usr/local/bin/python3. Nenhum Rust toolchain é necessário para as verificações documentais.

**Pré-condições:** confira git rev-parse HEAD, git status --short, package name/manifest e quatro paths autorizados. Pare se baseline divergir, houver mudança não atribuída, ou se for preciso target resolvido/provider externo.

**Fixtures/limites:** fonte, manifest, checker Python e Git; sem secrets, provider env, fixtures reais, rede ou recurso pago. Leitura local apenas; checker/diff não compilam nem agendam CI.

**Proveniência:** o pin histórico `cb94e251c0f17382565bf863f517945cbb2a84d6` é equivalente para os arquivos do package examinados; use `1177dad2ca2a9f21c29b5a118aa7944b77147798` para lockfile/workflows.

<a id="m02"></a>
## M02 — Registro e seleção de procedimentos

Este é o conjunto exato de PROC-ID do manual. Estados abaixo são status documentais atuais, não aprovação independente nem evidência de comportamento.

| PROC-ID | Situação | mode | review_status | execution_status | required_for_acceptance |
|---|---|---|---|---|---|
| [PROC-001](#proc-001) | identidade e população estática | READ_ONLY | BLOCKED | REVIEWED_NOT_EXECUTED | false |
| [PROC-002](#proc-002) | ordem de estado/erro do runner | READ_ONLY | BLOCKED | REVIEWED_NOT_EXECUTED | false |
| [PROC-003](#proc-003) | checker documental e escopo | READ_ONLY | BLOCKED | EXECUTED_LOCAL | true |
| [PROC-004](#proc-004) | candidato de teste local por target | LOCAL_ISOLATED | BLOCKED | BLOCKED_FOR_OPERATION | false |

Todos mantêm review_status BLOCKED até cold review externa. PROC-003 tem resultado
local registrado abaixo; PROC-001, PROC-002 e PROC-004 seguem NOT_EXECUTED.
Não há registro externo de review integrado. Checker green não aprova conteúdo.

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Conferir identidade e census estático

**Gatilho/objetivo:** mudança no manifest, target, root export ou import; confirmar declaração e paths, não resolução.
**mode:** READ_ONLY.
**review_status:** BLOCKED. **execution_status:** REVIEWED_NOT_EXECUTED. **required_for_acceptance:** false.
**environment:** checkout local no baseline aprovado; leitura filesystem/Git; sem rede, Cargo, secrets ou alterações.
**Entradas/pré-condições:** tests/e2e-byok-revoke/Cargo.toml, Cargo.toml, src/{lib,helpers}.rs e oito test paths; git status sem path alheio.

1. Rode git rev-parse HEAD e git status --short; compare baseline e anote dirty paths.
2. Rode sed -n '1,120p' tests/e2e-byok-revoke/Cargo.toml e rg -n 'e2e_byok_revoke|corelink_byok|corelink_ops' tests/e2e-byok-revoke/src tests/e2e-byok-revoke/tests.
3. Compare cada target/import e direct dependency com [R01/R06](REFERENCE.md#r01) e [B02/B03](BLAST_RADIUS.md#b02).

**Predicado esperado:** package identity, sete declarações diretas e oito target paths concordam; desvio vira REL/falsifier novo ou UNKNOWN.
**Falha/parada:** baseline, import ou path não confere; não inferir graph; pare e registre o diff concreto.
**Recuperação:** nenhuma mutação a desfazer; repita leitura em checkout correto ou entregue ao lead.

**result:** NOT_EXECUTED. **review_evidence:** ["manifest/source cited above; author static inspection only"]. **execution_evidence:** []. **limitations:** ["cold review pending; does not prove resolution"].
[Índice de procedimentos](#m02)

<a id="proc-002"></a>

### PROC-002 — Rastrear falha parcial do runner

**Gatilho/objetivo:** mudança de branch, erro ou ordem; falsificar FLOW-001/002 lendo implementação local.
**mode:** READ_ONLY.
**review_status:** BLOCKED. **execution_status:** REVIEWED_NOT_EXECUTED. **required_for_acceptance:** false.
**environment:** source checkout legível; sem Cargo/build/test, provider, store real ou permissão de produção.
**Entradas/pré-condições:** helpers e contratos peer em [R04/R05](REFERENCE.md#r04); escopo é o runner duplicado local.

1. Rode rg -n 'check_access|evict_all_for_key|mark_degraded|SlaViolated|audit.record|alert_recovery|restore_active' tests/e2e-byok-revoke/src/helpers.rs.
2. Leia o intervalo com sed -n '751,861p' tests/e2e-byok-revoke/src/helpers.rs.
3. Compare ordem e cada ? com FLOW-001/002, INV-003/004 e [REL-009](BLAST_RADIUS.md#rel-009); anote efeito que permanece antes do erro.

**Predicado esperado:** SLA falha após eviction/degrade e antes de audit/alert; alert falha após audit; recovery pode restaurar active antes de audit/alert subsequente.
**Falha/parada:** fonte mudou sem recuperar estado parcial, ou conclusão requer observação/durabilidade externa; não chame partial state de rollback.
**Recuperação:** leitura apenas; altere só documento sob escopo separado; encaminhe store/cache/API peer ao owner.

**result:** NOT_EXECUTED. **review_evidence:** ["helpers.rs source intervals cited above; author static inspection only"]. **execution_evidence:** []. **limitations:** ["cold review pending; source is not runtime behavior"].
[Índice de procedimentos](#m02)

<a id="proc-003"></a>

### PROC-003 — Rodar gate documental local

**Gatilho/objetivo:** após preparar os quatro bytes; checar estrutura, links, budgets, whitespace e escopo.
**mode:** READ_ONLY.
**review_status:** BLOCKED. **execution_status:** EXECUTED_LOCAL. **required_for_acceptance:** true.
**environment:** checkout local, Python provisionado e Git; sem acesso externo nem escrita fora dos paths atribuídos.
**Entradas/pré-condições:** CHECKER fornecido pelo caller e existente; baseline 1177dad2ca2a9f21c29b5a118aa7944b77147798; quatro paths exatos. Não grave path temporário.

1. Rode test -n "$CHECKER" e test -f "$CHECKER"; pare se indisponível.
2. Rode /usr/local/bin/python3 "$CHECKER" --kind skill --profile S --root . .claude/skills/own-e2e-byok-revoke/SKILL.md.
3. Repita com --kind reference, --kind blast_radius e --kind maintenance para os três docs/ownership/crates/e2e-byok-revoke/*.md.
4. Rode git diff --check 1177dad2ca2a9f21c29b5a118aa7944b77147798 HEAD e git diff --name-only 1177dad2ca2a9f21c29b5a118aa7944b77147798 HEAD; reconcilie só os quatro paths.

**Predicado esperado:** quatro checks sem erros, diff check exit 0, path list contém só os quatro arquivos; resultado estrutural apenas.
**Falha/parada:** checker, link, budget, whitespace ou escopo falha; corrija só paths autorizados e repita todos os checks; não alegue cold approval.
**Recuperação:** não há estado de runtime a compensar; preserve saída; não reverta mudança de terceiro.

**result:** PASS. **review_evidence:** ["CO-1 v1.1-candidate and checker input inspected; author only"]. **execution_evidence:** ["handoff:w014-e2e-byok-fix-1a30ee01; four S-profile checker outputs and diff check"]. **limitations:** ["structural gates only; no external package-record evidence or cold approval"].
[Índice de procedimentos](#m02)

<a id="proc-004"></a>

### PROC-004 — Candidato isolado de teste por target

**Gatilho/objetivo:** somente em trabalho futuro, com pedido explícito e gate local autorizado; validar predicado do target, não produção.
**mode:** LOCAL_ISOLATED.
**review_status:** BLOCKED. **execution_status:** BLOCKED_FOR_OPERATION. **required_for_acceptance:** false.
**environment:** worktree descartável, Cargo/cache provisionados, rede desabilitada, sem provider env/credentials e sem --ignored; este WP não autoriza execução.
**Entradas/pré-condições:** escolha um target de M04, confirme comando offline, orçamento e parada com responsável daquele trabalho.

1. Antes de futura execução, obtenha autorização explícita; confirme ausência de credentials/endpoints AWS/GCP/Azure/Vault.
2. Copie só comando exato do target em M04; não amplie para --workspace, --all-features ou --ignored.
3. Se alteração afeta peer/API, pare e obtenha revisão do owner; passe local não é contrato de produção.

**Predicado esperado:** apenas target escolhido conclui e assertions observáveis correspondem ao source; nenhum provider contatado.
**Falha/parada:** cache offline ausente, ambiente não isolado, credential/endpoint presente, target desconhecido ou assertion falha; não retry live.
**Recuperação:** capture saída local sem secrets e descarte worktree próprio; nenhum rollback externo autorizado.

**result:** NOT_EXECUTED. **review_evidence:** ["manifest and target declarations cited in M04"]. **execution_evidence:** []. **limitations:** ["candidate only; requires separately authorized work and cannot certify operation"].
[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Testes e validação

Manifesto não declara [features]; comandos usam a seleção default (nenhuma feature de package declarada). Comandos a seguir são candidatos, não executados. Cada um escolhe package/target explicitamente; não há --ignored, --all-features, rede ou provider real. Todos NOT_EXECUTED nesta autoria.

| Mudança / target | package / features | Comando candidato exato | Predicado | Ambiente / evidência |
|---|---|---|---|---|
| revoke local | e2e-byok-revoke; happy_revoke_flow | cargo test --offline -p e2e-byok-revoke --test happy_revoke_flow | assertions fonte do fluxo local | isolado; NOT_EXECUTED |
| recovery local | e2e-byok-revoke; recovery_flow | cargo test --offline -p e2e-byok-revoke --test recovery_flow | status/evento fixture esperado | isolado; NOT_EXECUTED |
| cache fake | e2e-byok-revoke; adversarial_stampede | cargo test --offline -p e2e-byok-revoke --test adversarial_stampede | predicados declarados no target | isolado; NOT_EXECUTED |
| corrida fake | e2e-byok-revoke; adversarial_race_condition | cargo test --offline -p e2e-byok-revoke --test adversarial_race_condition | assertion source da corrida local | isolado; NOT_EXECUTED |
| erro transitório | e2e-byok-revoke; adversarial_transient_api_error | cargo test --offline -p e2e-byok-revoke --test adversarial_transient_api_error | braço transient source local | isolado; NOT_EXECUTED |
| property fake | e2e-byok-revoke; prop_fail_closed | PROPTEST_CASES=100 cargo test --offline -p e2e-byok-revoke --test prop_fail_closed | propriedade em casos delimitados; não universal | isolado; NOT_EXECUTED |
| labels multi-provider | e2e-byok-revoke; multi_provider_matrix | cargo test --offline -p e2e-byok-revoke --test multi_provider_matrix | assertions labels/fake, não adapters | isolado; NOT_EXECUTED |
| env-gated source | e2e-byok-revoke; live_provider_gated | cargo test --offline -p e2e-byok-revoke --test live_provider_gated | somente itens não ignored; ignored permanece ignorado | isolado/sem secrets; NOT_EXECUTED |

**Negativos:** erros antes/depois de escrita, estado parcial, default empty detector source e env gate falso requerem leitura de assertions; não foram executados. Corpos ignored nunca entram nos candidatos. cas_foundation.yml, workspace-lint e CodeQL são gates workspace declarados, não repetidos nem disparados aqui. Offline não prova deps cacheadas ou seleção resolvida.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversibilidade / estado | Recuperação segura | Prova exigida |
|---|---|---|---|
| Código/helpers/manifests locais | source reversível em branch; sem schema/wire/dado próprio observado | coordenar e reverter/avançar somente commit próprio; preservar mudanças alheias | diff limpo e testes locais autorizados, sem claim de produção |
| Fakes: store/cache/audit/alerts | memória de processo fixture; restart encerra instância, não desfaz efeitos externos | descartar fixture/worktree próprio; sem restore real demonstrado | estado local antes/depois em execução isolada futura |
| Contracts BYOK/Ops importados | compatibilidade compile/API N/N-1 UNKNOWN; peer-owned | corrigir no owner e atualizar consumidor/harness coordenadamente | peer review e validação de seleção provider/consumer |
| Artifact/CI externo | execução/retention UNKNOWN | não deletar ou alterar; escalar ao owner pipeline | run ID/artifact/retention verificado pelo operador |
| Provider/status/audit produção | fora da library; atomicidade/irreversibilidade UNKNOWN | parar; composition root/owner operacional fornece runbook e aprovação próprios | evidência de estado via rota autorizada, nunca fixture |

Este package não declara schema, migration, durable store, wire endpoint, release/deploy workflow ou composition root de produto. Assim, não há recovery produtiva para prescrever aqui. Git revert não desfaz estado externo, mensagens enviadas, billing ou artifacts consumidos. Não inferir compatibilidade N/N-1 ou transação atômica de trait/fake.

<a id="m06"></a>
## M06 — Escalonamento e registro

| Condição | Responsável/rota verificada | Handoff mínimo | Ação vedada |
|---|---|---|---|
| BYOK provider/cache/detector/trait | corelink-byok; CODEOWNERS /crates/corelink-byok/ @gmhelmold | contratos/APIs e RELs, path source, risco/estado parcial, pergunta, rota | editar implementação peer sob esta skill |
| Ops alerter/config/canais | corelink-ops; sem regra específica, fallback * @gmhelmold | contrato/REL, erro após audit, risco, owner verificado | assumir entrega ou acionar canal real |
| Provider/storage/customer/produção | composition root e owner operacional UNKNOWN; parar e solicitar rota autorizada | serviço/ambiente sem secret, evento/tempo, efeito, risco, reversibilidade, owner por confirmar | KMS/provider call, credenciais, deploy, mutação/rollback produtivo |
| Mudança local/docs | rota CODEOWNERS e revisão independente necessária | baseline, paths, API/INV/FLOW/REL/PROC, gates/outputs, unknowns e owner | chamar checker green de cold approval |

Após tarefa autorizada, registre baseline, paths, contratos/RELs, risco, comandos e resultados reais separados de candidatos, estado parcial, procedimento e próximo responsável. Owner/rota não é aprovação ou disponibilidade. Envie hashes finais para revisão fria; mudança posterior exige reavaliação.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Skill](../../../../.claude/skills/own-e2e-byok-revoke/SKILL.md#s01) · [Início](#m01)
