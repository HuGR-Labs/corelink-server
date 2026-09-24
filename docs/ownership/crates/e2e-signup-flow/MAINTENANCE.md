---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-signup-flow
manifest: tests/e2e-signup-flow/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w015-signup-flow-source-1177dad2
---

# e2e-signup-flow — manual de manutenção

[Preparação](#m01) · [Seleção](#m02) · [Procedimentos](#m03) ·
[Matriz](#m04) · [Recuperação](#m05) · [Escalonamento](#m06).

<a id="m01"></a>
## M01 — Preparação segura

Registre baseline, package e paths antes de mudar. Use somente checkout local,
fixture sintética e os gates selecionados aqui. Não leia `.env`, provider config,
credential store, dado de cliente, checklist fora do escopo ou ambiente live.
Não rode a variante `#[ignore]`, use `--ignored`, teste workspace inteiro ou
workflow. O alvo Starter declara caminho ignorado; configuração e efeitos externos
permanecem desconhecidos.

Ambiente de teste ainda não foi certificado nesta revisão: valide toolchain/caches
offline em uma sessão de manutenção própria. Não altere lockfile nem habilite rede
para contornar falha de cache. O procedimento desta autoria executou somente Python
estático, comparação Git local e checagem de diff documental.

<a id="m02"></a>
## M02 — Seleção de procedimentos

| Situação | Procedimento | Modo | Efeito permitido |
|---|---|---|---|
| Auditar o escopo antes da mudança | [PROC-001](#proc-001) | `READ_ONLY` | leitura de source/manifests |
| Alterar fixture/cenário local | [PROC-002](#proc-002) | `LOCAL_ISOLATED` | arquivos locais e testes sintéticos |
| Verificar cenário default | [PROC-003](#proc-003) | `LOCAL_ISOLATED` | target declarado, sem `--ignored` |
| Pedido alcança Stripe ignorado | [PROC-004](#proc-004) | `READ_ONLY` | registrar bloqueio; sem provider |
| Mudança de API upstream | [PROC-005](#proc-005) | `READ_ONLY` até coordenação | análise e seleção de target |

<a id="m03"></a>
## M03 — Procedimentos

<a id="proc-001"></a>
### PROC-001 — Conferir package e relação afetada

**Gatilho/predicado:** antes de alterar harness; baseline e target devem corresponder ao package do pedido. **Modo/ambiente/permissão:** `READ_ONLY`, Git local; sem Cargo ou serviço. **Entradas:** commit base do checkout e `tests/e2e-signup-flow/Cargo.toml`. **Pré-condição:** checkout identificado; não ler arquivos de provider/segredo. 1. Registre `git rev-parse HEAD` e a lista de paths modificados. 2. Compare manifesto, fontes e workspace membership com a revisão de fonte aplicável; faça busca literal de consumidores em manifests/fontes. 3. Abra

o REL e contrato upstream correspondente; marque resolução/runtime como `UNKNOWN` se não medidos. **Esperado:** mudança e owner estão mapeados sem ampliar a alegação. **Pare/recupere:** package, caminho ou contrato divergiu; não ajuste policy por suposição. Restaure apenas edições próprias por patch revisado. **Certificação/evidência:** revisão `SOURCE`; `REVIEWED_NOT_EXECUTED` para grafo/executor. Registrar commits, caminhos, query literal e resultado.

[Índice](#m02)

<a id="proc-002"></a>

### PROC-002 — Alterar fixture ou assertion in-memory

**Gatilho/predicado:** mudança de IDs, contexto, estado fake ou assert; deve preservar o predicado indicado no API/INV. **Modo/ambiente/permissão:** `LOCAL_ISOLATED`; somente dados sintéticos e arquivos deste harness. **Entradas:** fixture determinística, target escolhido em M04 e contrato do owner. **Pré-condição:** manter gate DPA e checagens negativas; identificar como um diff pode reverter. 1. Altere o fixture/caso local sem alterar serviço upstream ou estado externo. 2. Atualize o API/INV/REL afetado e identifique assertion que

falharia com a regressão. 3. Execute somente o comando seletivo de M04 quando esse gate estiver aprovado para a sessão. **Esperado:** assertion cobre o predicado sem depender de relógio, rede ou tenant real. **Pare/recupere:** necessidade de provider, segredo ou dado real bloqueia; reverta somente o diff local e retenha a saída do gate. **Certificação/evidência:** `REVIEWED_NOT_EXECUTED` nesta autoria; execução futura requer status/saída/ambiente reais, não inferidos.

[Índice](#m02)

<a id="proc-003"></a>

### PROC-003 — Validar alvo in-memory seletivo

**Gatilho/predicado:** alteração local precisa demonstrar seu contrato do harness. **Modo/ambiente/permissão:** `LOCAL_ISOLATED`, checkout isolado e cache offline; não é CI nem produção. **Entradas:** package/target exato da linha M04; `--locked --offline` impede buscar dependências. **Pré-condição:** conferir que o comando não contém `--ignored`; para o target Starter, reconhecer que o binary também compila o caso ignorado. 1. Registre commit, versão de toolchain e estado do checkout. 2. Execute somente o target/seleção da linha

correspondente; preserve stdout, stderr e exit code. 3. Compare o resultado ao predicado e confirme que não ocorreu alteração fora do checkout isolado. **Esperado:** passa apenas a suite selecionada; uma falha permanece registrada. **Pare/recupere:** cache offline ausente, resolução alteraria lock ou target requer provider; pare sem habilitar rede e restaure só temporários criados. **Certificação/evidência:** `REVIEWED_NOT_EXECUTED` nesta revisão; procedure reviewed, operação não certificada.

[Índice](#m02)

<a id="proc-004"></a>

### PROC-004 — Pedido de Stripe/provider no alvo ignorado

**Gatilho/predicado:** pedido envolve variante `#[ignore]`, credencial, configuração ou endpoint externo. **Modo/ambiente/permissão:** `READ_ONLY`; sem abrir provider/secret config, sem carregar env e sem executar/buildar o caminho ignorado. **Entradas:** path do alvo e limite documentado em REL-004/022. **Pré-condição:** nenhuma credencial presente no ambiente de manutenção. 1. Registre qual target e mudança foram pedidos, sem copiar valores secretos. 2. Marque detalhes de configuração, owner de operação, alcance e efeitos como `UNKNOWN`. 3. Encaminhe a

decisão a um owner verificado antes de qualquer nova revisão operacional. **Esperado:** pedido fica bloqueado; esta procedure não prepara comando de provider. **Pare/recupere:** se segredo/config foi exposto, pare e use rota de segurança já verificada; a rota não foi identificada aqui. **Certificação/evidência:** `BLOCKED_FOR_OPERATION`; nenhuma execução aceita.

[Índice](#m02)

<a id="proc-005"></a>

### PROC-005 — Coordenar alteração de API upstream

**Gatilho/predicado:** assinatura, tipo, erro ou semântica de dependência mudou. **Modo/ambiente/permissão:** `READ_ONLY` até contrato e owner aceitarem o plano. **Entradas:** API upstream, REL e todos os alvos que importam a superfície. **Pré-condição:** distinguir compatibilidade da API do comportamento do fake. 1. Identifique o crate dono da implementação/contrato, sem atribuir a operação ao harness. 2. Atualize somente o cenário correspondente e mantenha o caso negativo que falsifica regressão. 3. Coordene lock/policy/peer se

o manifesto ou edge mudar; selecione validação M04. **Esperado:** ownership e consumers estáticos estão conciliados antes de executar. **Pare/recupere:** owner, wire contract ou rollback de dados não aplicável/desconhecido; aguarde definição no owner upstream. **Certificação/evidência:** `REVIEWED_NOT_EXECUTED`; registrar source, peers e decisão externa sem alegar aprovação.

[Índice de procedimentos](#m02)


<a id="m04"></a>
## M04 — Matriz de testes e validação

Os comandos abaixo são prescrições para sessão futura e **não foram executados
nesta autoria**. Não use `--ignored`, `--all-targets`, `--workspace` ou workflow.
Todos selecionam dependências offline e travadas; falha de cache é bloqueio, não
convite para rede. A linha Starter executa somente casos não ignorados por padrão,
mas ainda compila o test binary que contém a variante não revisada; exige revisão
de escopo antes do uso.

| Tipo de mudança | Package / target / features | Comando ou alvo | Predicado | Ambiente / evidência |
|---|---|---|---|---|
| API/fixture library ou `r2.rs` | `e2e-signup-flow --lib`, sem features próprias | `cargo test --locked --offline -p e2e-signup-flow --lib` | casos unitários declarados de R2 satisfazem autorização/round-trip/contadores | target local; não executado nesta autoria |
| jornada Free ou CAS | `--test happy_path_free` | `cargo test --locked --offline -p e2e-signup-flow --test happy_path_free` | tier Free, sem sessão Stripe; PUT/GET e stats conforme assertions | fixture in-memory; não executado |
| Starter in-memory | `--test happy_path_starter_stripe_test_mode` | `cargo test --locked --offline -p e2e-signup-flow --test happy_path_starter_stripe_test_mode` | receipt, criação fake e replay conforme assertions não ignoradas | compila alvo com variante ignorada; executar somente após revisão isolada; não executar nesta autoria |
| bloqueio sem DPA | `--test adversarial_dpa_not_accepted` | `cargo test --locked --offline -p e2e-signup-flow --test adversarial_dpa_not_accepted` | Free e Starter retornam `DpaRequired`; fake Stripe vazio | fixture in-memory; não executado |
| assinatura fixture inválida | `--test adversarial_webhook_signature_invalid` | `cargo test --locked --offline -p e2e-signup-flow --test adversarial_webhook_signature_invalid` | rejeição não promove estado antes da asserção negativa | sem endpoint/provider; não executado |
| replay de signup | `--test idempotency_replay` | `cargo test --locked --offline -p e2e-signup-flow --test idempotency_replay` | mesmo `signup_id`, um commit e um `Started` conforme source | fixture in-memory; não executado |
| propriedades atômicas | `--test prop_atomic_invariants`, sem features próprias | `PROPTEST_CASES=1000 cargo test --locked --offline -p e2e-signup-flow --test prop_atomic_invariants` | casos confirmam `Provisioned`, commit único, audit inicial e replay | `1000` replica default da fonte; não executado |
| relação/build completa do package | não selecionar | `BLOCKED` — sem comando neste manual | target não pode puxar escopo ignorado para a sessão atual | resolver apenas em revisão separada; não executar `--all-targets` |

**Gates globais:** não avaliados aqui. Workflow workspace amplo não prova o
resultado deste package sem run e seleção registrados.

<a id="m05"></a>
## M05 — Recuperação e compatibilidade

| Superfície | Reversível? | Condição / dado existente | Ação segura | Prova de recuperação |
|---|---|---|---|---|
| fixture, assertion, fake | sim, no checkout | estado é local/in-memory na fonte lida | revert revisado do diff local | comparar `git diff`; assertion retorna ao source anterior |
| source de API/target deste package | em geral sim no Git | package `publish=false`; consumers resolvidos não foram medidos | coordene consumidores do M04 antes de remover API | compilar/testar seleção autorizada em outra sessão |
| store/audit DPA/signup | não aplica como dado persistido aqui | fakes são in-memory; persistência externa não pertence ao harness | nenhuma ação de dados; escalar ao owner do store real | não há prova de recuperação externa |
| webhook/provider/wire externo | `UNKNOWN` | caminho Stripe ignorado não foi inspecionado nem executado | pare; não trate `git revert` como compensação externa | requer owner, estado inicial/final e procedimento autorizado |
| dependency, lock ou policy | condicional | `Cargo.lock`/`deny.toml` são arquivos compartilhados | concilie mudança com owner de manifest/policy e peers | diff do manifest/lock/policy e gates escolhidos após decisão |

Não há migration ou artifact de produção declarado para este package. Reverter
um teste restaura detecção local; não desfaz uma eventual chamada externa. Não há
versão publicada deste harness; compatibilidade de APIs upstream continua sob
os respectivos owners. Nenhuma janela N/N-1 de produto foi identificada.

<a id="m06"></a>
## M06 — Escalonamento e registro

| Condição | Owner/rota | Evidência mínima | Ação vedada |
|---|---|---|---|
| Contrato de signup | implementação/contrato: `corelink-signup`; rota humana `UNKNOWN` | API/REL, caller e assertion | transferir política ao harness |
| DPA/consent | `corelink-dpa-acceptance`; rota humana `UNKNOWN` | proof, receipt e caso adversarial sintético | mudar conteúdo legal ou chave |
| Tier | `corelink-tier-selection`; rota humana `UNKNOWN` | gate, receipt e cenário de negação | bypassar DPA-first |
| Hash/CAS | `corelink-hash` para digest; backend real `UNKNOWN` | digest fake e comparação | alegar storage/R2 de produção |
| Stripe ignorado | owner operacional/config `UNKNOWN` | somente identificação do alvo; sem secret value | ler config, usar credencial, chamar provider ou executar `--ignored` |
| Aprovação documental | revisor independente a designar | quatro hashes finais e veredito por artefato | autoaprovar ou inventar assignee |

Após a mudança, registre baseline, paths, PROC, gates realmente executados,
saídas e desconhecidos. Mantenha `state: draft` até revisão independente; o
autor não altera índices globais nem registra operação que não observou.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Skill](../../../../.claude/skills/own-e2e-signup-flow/SKILL.md#s01) · [Início](#m01)
