# CoreLink — Roadmap canônico de remediação

> ## PARTE 0 — ÂNCORA (leia isto primeiro, sempre)
>
> Este bloco existe para que o lead — eu, ou quem assumir — retome sem perder
> julgamento acumulado. Tudo abaixo foi **medido**, não presumido. Se o contexto
> compactar, esta é a memória.

### 0.1 Autoridade

Uma única sessão detém **merge** nos 3 repos (`corelink-server`, `corelink-runners`,
`corelink-workspaces`). Todas as outras sessões e agentes **abrem PR e reportam ao
lead** — nunca mergeiam, nunca deployam, nunca escalam direto ao owner. O lead julga
o que sobe.

**Não existe branch protection em nenhum dos 3 repos** (`403 Upgrade to GitHub Pro`).
Nada do lado do GitHub impede um merge. O lead é a única trava que existe. Isso não é
retórica: o portão foi furado 4 vezes num dia, e três mensagens de commit dizem
`"Bypassing DCO + gates"` em texto puro.

### 0.2 As regras de verificação (cada uma nasceu de um erro real)

| Regra | O erro que a originou |
|---|---|
| **Conjunto, não contagem.** Compare o CONJUNTO de lanes contra um PR de mesmo escopo. | Duas lanes trocadas dão o mesmo número e escondem o que a régua existe pra pegar. |
| **`git show origin/main:<path>`, nunca o checkout.** | Contei conceitos contra um worktree 3 dias defasado e errei 8 vs 13. |
| **Nunca conclua AUSÊNCIA de saída truncada.** `head`/`cut` cabem na tela; `grep -c` decide existência. | Um `cut -c1-200` cortou a prova e produziu um "não existe" falso. |
| **Exit code nu, nunca por pipe.** `cmd \| tail` devolve o status do `tail`. | Foi assim que um PR entrou com 4 checks pendentes. |
| **Pendente é pendente**, mesmo quando o job se diz informativo. | O cabeçalho descreve a intenção do job, não o estado do check. |
| **A prosa não é evidência.** Verifique no caminho que produção toma. | Um cap de leitura foi ligado no ramo de teste; produção segue sem limite. |
| **Nenhum teste que passa antes do fix.** Reverta o fix: a suíte tem de ficar vermelha. | Um teste verificava o parâmetro, não o bug. |
| **Âncora tem de sobreviver ao squash.** Blob final ou commit já em `origin/main`. | 13 conceitos ancorados em commits órfãos. |
| **Ao estabelecer um negativo, varra o que depende dele.** | Escreveram no PR que uma proteção não vale em release e deixaram de pé, na wiki, a frase que dependia dela. |
| **Antes de despachar, verifique se o trabalho já existe.** | Um defeito foi consertado duas vezes por duas auditorias com nomes diferentes. |
| **Nunca `git add -A`.** Índice compartilhado entre worktrees. | Varreu 54 workflows e um módulo inteiro para dentro de um PR. |
| **Um worktree por WP, a partir de `origin/main`.** | O checkout raiz está divergido e carrega commit com assinatura DCO fabricada. |

### 0.3 O formato de defeito DOMINANTE

**A prosa é excelente e a implementação não corresponde.** Encontrado 4× num só dia:
um cap ligado no ramo errado; um `cargo fmt` que era o revert da formatação correta;
um "arquivei os arquivos velhos" que não arquivou quase nada e justificou com
afirmação falsa; um "movi a chave" que a deixou em dois branches remotos.

**Em todos existia teste, `verify` ou portão — e todos eram incapazes de notar.**

### 0.4 Portões estruturalmente cegos (6 confirmados)

Passam verde **enquanto o defeito existe**. Pior que não ter portão: produzem confiança.

`CITE_RE` do OKF (exige caminho, não vê citação abreviada) · C5 do OKF (valida posição,
não correspondência) · `sdk-artifacts` (filtrado pra nunca disparar no PR culpado) ·
espelho CF-1 (unidirecional) · piso de cobertura (**nunca executou**: faltava
`--coverage`) · escopo de cobertura (só conta arquivo importado por teste).

**Contra-exemplo a imitar: o C10b** — recusa seed cujo conceito não cite o arquivo, ou
seja, **recusa auto-certificação**.

> **Pergunta obrigatória em toda revisão de portão: "o que quebraria isto e NÃO seria
> pego?"** Sem resposta, o portão não foi revisado — foi lido.

### 0.5 Retratações (não repetir)

1. **"RCE sem autenticação"** no exec-server — **falso**. O binário falha fechado; o
   serviço nem sobe. É funcionalidade morta, não risco.
2. **"O documento marca tudo BROKEN"** — li a **coluna errada** de uma tabela de duas
   colunas. Os 🔴 eram da auditoria que o documento refutava.
3. **"Buraco de auth `_oci`/`_public`"** — divergência real, **não explorável**: o
   header é removido em todo forward do Worker.
4. **"F-009 é P0"** — escalei a partir de `grep` estreito. O comportamento é
   documentado, herdado do protocolo do Turborepo, e a correção sugerida é
   **impossível** (o hash cobre inputs que o servidor nunca vê).

**Padrão comum às quatro: severidade afirmada a partir de leitura parcial.** Verifique
o mecanismo antes de classificar.

### 0.6 Este documento registra DECISÃO e CONTRATO — nunca ESTADO

Estado muda e o documento apodrece (já apodreceu duas vezes). Estado se consulta:

```bash
gh pr list --state open --json number,title,mergeable
bash scripts/pre-merge-gate-check.sh <PR>          # sem pipe, sempre
```

Se uma tabela deste arquivo discordar do comando, **o comando ganha**.

---


**Baseline:** `origin/main` @ `1fc1092d` · **Lead:** sessão guardiã de merge · **Data:** 2026-08-29

> **Autoridade.** Uma única sessão detém a decisão de merge nos 3 repos
> (`corelink-server`, `corelink-runners`, `corelink-workspaces`). Todas as demais sessões e
> agentes **abrem PR e reportam ao lead** — nunca mergeiam, nunca deployam, nunca escalam
> direto ao owner. O lead julga o que sobe.

---

## 0. Go/No-Go — a decisão de paralelizar

Respondido antes de qualquer fanout, conforme a disciplina de decomposição.

**1. O trabalho é decomponível em fatias disjuntas?** Sim, **depois** do WP-0. Hoje não é.

**2. Qual o arquivo único que todo agente tocaria?** `CHANGELOG.md`.
**10 dos 13 PRs abertos o editam** (#1389, #1393, #1395, #1396, #1398, #1399, #1400, #1402,
#1410, #1412). É a causa única da serialização: cada rebase invalida o próximo. Enquanto ele
estiver no caminho de edição, **paralelismo real é impossível** — só se produz fila com passos extras.

**3. Há dependências entre fatias?** Sim, três cadeias (§3).

**4. Teto de concorrência:** ≤6 agentes simultâneos (prompts não-triviais).

**Veredito: HÍBRIDO.** WP-0 primeiro, sequencial, para remover o arquivo compartilhado.
Depois disso, 5 WPs em paralelo real. A fila de merge (WP-1) corre em paralelo o tempo todo,
mas é **do lead e não é delegável**.

---

## 0.9 — PRÉ-VOO OBRIGATÓRIO (antes de despachar QUALQUER WP)

> Este documento já falhou nisto. Três dos oito WPs mandavam criar branches que já
> existiam — dois **mergeados**, um em voo — com o nome exato prescrito aqui. A causa
> é a mesma que o §III.0 nomeia: **estado codificado dentro do que se apresenta como
> contrato atemporal.** Um WP é uma definição de trabalho; se o trabalho já existe, o
> WP está obsoleto e despachá-lo produz PR duplicado.

Rode os DOIS comandos. Um só não basta:

```bash
WP_BRANCH="<branch do WP>"; WP_SLUG="<palavra-chave do WP>"

git ls-remote --heads origin "$WP_BRANCH"          # branch ainda vivo?
gh pr list --state merged --search "$WP_SLUG" \
   --json number,title,mergedAt                    # já mergeado?
```

**Por que os dois:** o portão de merge **deleta o branch remoto** ao mergear. Um
`ls-remote` sozinho devolve "livre" para trabalho já concluído — testado: dos três
branches que colidiram, dois apareciam livres porque já haviam sido mergeados e
deletados. Consultar só branch vivo é o mesmo erro de "concluir ausência de leitura
parcial" que o §0.2 proíbe.

**Se qualquer um dos dois acertar: PARE e reporte ao lead. Não despache.**

Vale também entre campanhas: um mesmo defeito já foi consertado **duas vezes** por duas
auditorias que lhe deram nomes diferentes (§III.3). Busque pelo **sintoma**, não só
pelo identificador do WP.

## 1. Regra de isolamento (INVIOLÁVEL)

**Cada WP tem seu próprio worktree, criado a partir de `origin/main`, e ninguém entra no
worktree de outro.**

```bash
git worktree add /tmp/wt-<WP-ID> origin/main -b <branch-do-WP>
```

| Proibido | Motivo — todos já causaram dano nesta campanha |
|---|---|
| Trabalhar no checkout raiz | A `main` local dele está divergida e carrega um commit com **assinatura DCO fabricada** (`M3 Audit Agent`). Um `push` distraído o publica. |
| Reusar worktree alheio | Índice e stash compartilhados: `git add -A` rouba WIP de terceiro. |
| Confiar no checkout sem checar o commit | Um agente contou conceitos contra checkout 3 dias defasado e errou o número. Verificação de conteúdo usa **`git show origin/main:<path>`**, nunca o disco. |
| Concluir ausência de saída truncada | `cut -c1-200` cortou a prova e produziu um "não existe" falso. Ausência se afirma com `grep -c`, sem truncar. |

---

## 2. Padrões de qualidade (valem para TODO WP)

Derivados de defeitos reais desta campanha. Não são conselhos — são condições de aceite.

| # | Padrão | Defeito real que o originou |
|---|---|---|
| Q1 | **A prosa não é evidência.** Toda afirmação do PR é verificada contra o código no caminho que **produção** toma. | Um cap de leitura foi ligado no ramo de teste; produção segue sem limite. |
| Q2 | **Nenhum teste que passa antes do fix.** Reverta o fix: a suíte tem de ficar vermelha. | Teste que verificava o parâmetro, não o bug. Reverter o fix deixava tudo verde. |
| Q3 | **Nenhuma âncora que o squash orfana.** Referencie blob do conteúdo final ou commit já em `origin/main` — nunca o HEAD do próprio branch. | 13 conceitos ancorados em commits órfãos. |
| Q4 | **Ao estabelecer um negativo, varra o que depende dele.** "Compilado fora", "não wired", "sem consumidor" → procure quem afirma o contrário, **no momento do achado**. | Sessão escreveu no corpo do PR que uma proteção não vale em release e deixou de pé, na wiki, a frase que dependia dela. |
| Q5 | **Contagem ≠ conjunto.** Verificar CI compara o **conjunto** de lanes contra um PR de mesmo escopo, não o número. | Duas lanes trocadas dão a mesma contagem e escondem o que a régua existe para pegar. |
| Q6 | **`verify` de item concluído é guarda de regressão.** Passa enquanto a asserção existe; DRIFTED se alguém a apagar. Nunca escrito na polaridade "aberto". | Item `done` com `verify` na polaridade errada quebra o portão no merge do PR **seguinte**. |
| Q7 | **Sem pipe em portão.** `cmd \| tail` devolve o status do `tail`. Rode sem filtro e leia `$?`. | Um PR mergeou com 4 checks pendentes exatamente assim. |

---

## 3. Mapa de conflitos e dependências

```
WP-0 (changelog fragments)  ──┬─► libera paralelismo real de TODOS os WPs de código
                              │
WP-1 (fila de merge, LEAD) ───┼─► #1412 → #1410 → #1411 → #519 → #518
                              │
                    ┌─────────┴─────────┬──────────┬──────────┐
                  WP-2       WP-3     WP-4       WP-8      (paralelos, disjuntos)
                (artefatos)(cobertura)(CI)     (SDK)
                              │
                    #1410 ────┴─► WP-5 (OKF: 13 âncoras + 2 cegueiras)
                              │
       fila drenada ──────────┴─► WP-6 (re-corte devenv)  ─► WP-7 (série WP-*)
```

**Cadeias forçadas:**
1. `#1395 → #1396 → #1398` são uma **pilha acidental** — o diff de #1398 contém os commits dos outros dois. Mergear um mergeia os três.
2. WP-5 depende de #1410 (mesmos arquivos OKF).
3. WP-6 depende da fila drenada (re-corte parte de `main` limpa).

---

## 4. Work Packages

### WP-0 — Fragmentos de changelog (DESBLOQUEADOR, sequencial, primeiro)

**Problema:** `CHANGELOG.md` é editado por 10 dos 13 PRs abertos. Serializa tudo por construção.

**Mudança:** migrar para fragmentos — cada PR cria um arquivo **novo** em `changelog.d/`
(ex.: `changelog.d/1410-dsr-phantom-table.md`); um script monta o `CHANGELOG.md` na release.
Arquivo novo por PR ⇒ **zero conflito por construção**.

- **Worktree:** `/tmp/wt-wp0` · **Branch:** `feat/changelog-fragments`
- **Arquivos:** `changelog.d/`, `scripts/assemble_changelog.py`, `.github/workflows/changelog-validate.yml`
- **Contrato congelado:** o gate passa a exigir **≥1 arquivo novo em `changelog.d/`** em commits
  `feat:`/`fix:`, em vez de exigir entrada em `CHANGELOG.md`. Nome do fragmento:
  `<pr-number>-<slug>.md`. Formato: uma linha `### Added|Changed|Fixed|Removed` seguida do texto.
- **DoD:** gate verde num PR de teste que só cria fragmento; gate **vermelho** num PR `fix:` sem fragmento.
- **Invariants:** nenhum PR existente quebra (o gate aceita as duas formas durante a transição);
  o `CHANGELOG.md` montado é byte-idêntico ao atual para o conteúdo já existente.
- **Completeness:** os 10 PRs abertos são convertidos, **ou** a compatibilidade dupla é mantida
  até a fila drenar. Documentar qual das duas.

### WP-1 — Fila de merge · **LEAD, NÃO DELEGÁVEL**

Ordem: `#1412` → `#1410` → `#1411` → `#519` → `#518`.
Cada merge exige: conjunto de lanes conferido (Q5), zero pendentes (Q7), zero falhas
não-herdadas. Falha herdada é nomeada e rastreada, nunca ignorada.

### WP-2 — Preservar artefatos do handoff

- **Worktree:** `/tmp/wt-wp2` · **Branch:** `docs/preserve-m3-artifacts`
- **Arquivos:** `docs/campaigns/devenv/**` — disjunto de tudo
- **Contexto:** o TODO de 25 itens existe **só no disco, não commitado**; 6 pareceres em `/tmp`
  **já se perderam**; 30 arquivos legítimos vivem apenas no commit solto `b01c7cac`.
- **DoD:** `b01c7cac` cherry-pickado por **nome de arquivo** (nunca `git add -A`);
  `TODO.md` commitado; working tree limpo.
- **Invariants:** não traz código, só documentação; não traz `wrangler.toml`, `Dockerfile`,
  `main.rs` nem `REMEDIATION_PLAN.md` (esses pertencem ao commit em quarentena).
- **Completeness:** `git status --porcelain` vazio no worktree raiz para `docs/campaigns/`.

### WP-3 — Portão de cobertura

- **Worktree:** `/tmp/wt-wp3` · **Branch:** `ci/coverage-threshold`
- **Arquivos:** `vitest.config.ts` (2 repos) · **DoD:** CI falha abaixo do piso; provado com PR de teste que baixa cobertura.
- **Invariants:** piso na cobertura **atual medida menos folga**, nunca acima — portão que já nasce vermelho é ignorado por todos.
- **Completeness:** os 3 repos cobertos ou justificativa escrita de por que um ficou fora.

### WP-4 — Higiene de CI

- **Worktree:** `/tmp/wt-wp4` · **Branch:** `ci/hygiene-sweep` · **Depende de:** #1402 resolver (mesmos arquivos)
- **Escopo:** aposentar 5 lanes com **1.057 execuções e zero sucessos**; remover 5 crons redundantes;
  `cosign-sign.yml` (gatilho `push`, **0 execuções na vida**); `cas-canary` (job hosted `skipped`
  ⇒ workflow verde **sem testar** o caminho off-fabric).
- **DoD:** cada lane ou volta a funcionar, ou é removida com item de backlog explicando o que falta. **Nenhuma fica vermelha crônica.**
- **Invariants:** zero `ubuntu-latest` introduzido; nenhum canário passa a rodar dentro da frota que vigia.
- **Completeness:** `grep -c "ubuntu-latest" .github/workflows/*` documentado antes/depois.

### WP-5 — OKF: 13 âncoras órfãs + 2 cegueiras de portão

- **Worktree:** `/tmp/wt-wp5` · **Branch:** `docs/okf-anchor-integrity` · **Depende de:** #1410
- **Escopo:** re-ancorar 13 conceitos (Q3); corrigir a citação `main.rs:661-681` em
  `planes/container.md:132` (aponta para a rota de quota; o mount real do archive é 622-639);
  fechar **B-059** (`CITE_RE` cego a citação sem caminho — o portão passa verde com citação errada)
  e **B-060** (gate espelho registry→migrations).
- **DoD:** `validate_okf` 0 stale / 0 drift **e** os dois `verify` na polaridade de regressão (Q6).
- **Invariants:** nenhuma âncora avançada sem reler a claim (avançar sem reler **mascara** drift futuro).
- **Completeness:** `git grep -c` das duas SHAs órfãs em `origin/main` = 0.

### WP-6 — Re-corte do devenv

- **Worktrees:** `/tmp/wt-wp6a` (server) e `/tmp/wt-wp6b` (runners) — **separados**
- **Depende de:** fila drenada
- **Fato que dimensiona:** o #1397 tem **28.642 linhas; a feature são 1.377 (4,8%)**.
  O resto é 21k de documentação gerada, 2.272 de código morto nunca declarado, 2.208 de
  workflows varridos por `git add -A` a partir de base 4 semanas velha, e um commit em quarentena.
- **Contrato congelado — DESCARTE (não negociável):** o commit `0a5e3349` inteiro (seed Ed25519
  em plaintext + 5 variáveis que viram condição de `exit(1)` no boot de produção + flip de feature
  + 3 crons novos); `crates/corelink-container/src/gc_worker/**`; as 54 mudanças de workflow;
  a camada de testes mockados.
- **Contrato congelado — SALVAR:** apenas por **nome de arquivo**, `git add -A` **proibido**.
- **DoD:** cada PR re-cortado compila, tem teste que falha se revertido (Q2), e o serviço
  **inicia** (o defeito raiz é um nome de variável trocado que impede o boot).
- **Invariants:** nada de `:latest` — imagem fixada por digest; nenhuma vinculação a DO cuja
  classe não esteja publicada (derruba o deploy inteiro, não só o devenv).
- **Completeness:** o conjunto salvo + descartado = 173 arquivos. Sem "sobrou um".

### WP-7 — Série WP-* (6 PRs)

- **Worktree:** `/tmp/wt-wp7` · **Depende de:** WP-0 (senão é fila, não paralelo)
- **Estado auditado + refutado:** #1395 (teste **nunca executou**, asserção morta), #1396
  (recria o mapa sem limite que diz consertar), #1398 (atinge **todo cliente do plano gratuito**
  em toda requisição), #1399 (**nem compila**; e o consumidor nunca lê o campo corrigido),
  #1400 (**nem compila**; o fix faz o evento sumir da fila de recuperação), #1402
  (dois workflows ficam **estruturalmente inválidos**).
- **DoD:** cada um compila, tem teste que falha se revertido, e os 3 empilhados são
  desempilhados ou mergeados na ordem forçada.

### WP-8 — Reconstruir artefato SDK

- **Worktree:** `/tmp/wt-wp8` · **Branch:** `fix/sdk-artifact-rebuild`
- **Problema:** `sdks/js/src/client.ts` mudou (`GET`→`HEAD`) em 26/08 **sem republicar** o pacote.
  O `.tgz` servido do nosso domínio está **defasado** — defeito voltado ao cliente. E trava
  #1356/#1376.
- **DoD:** tarball reconstruído contém a mudança; gate `published SDK artifacts` verde.
- **Invariants:** só reconstrói, não altera fonte.

---

## 5. Itens que exigem o owner

| # | Assunto | Por quê |
|---|---|---|
| 1 | **Não existe branch protection nos 3 repos** (`403 Upgrade to GitHub Pro`) — nada do lado do GitHub impede merge | Decisão de plano/custo. Hoje o lead é a única trava. |
| 2 | Commit com **assinatura DCO fabricada** na `main` local do checkout raiz | Já decidido: descartar. Pendente executar sem tocar WIP de terceiro. |
| 3 | Questionários de compliance já enviados a clientes tiveram respostas revertidas (`Y→N`) | Exposição contratual — decisão de counsel. |
| 4 | Add-on de BYOK precificado a $99/mês para funcionalidade que retorna **HTTP 501** | Decisão comercial/legal. |

---

## 6. Triagem do handoff M3 (25 itens)

| Destino | Itens | Motivo |
|---|---|---|
| ✅ Já feito, melhor que o pedido | 1, 14 | O item 1 pedia "validar CI"; o lead achou 2 merges com verificação vermelha, um deles quebrando apagamento GDPR. |
| 🔴 Entra agora | 13→WP-3, 16+22→WP-2 | Valor real, independente do devenv. |
| 🟡 Bloqueado até o devenv funcionar | 4-12 (8 itens) | **Miram código que não roda:** `devenv_guard.ts` nem existe na `main`; `runner_dev_env.ts` é o serviço que não inicia. Blindar antes é desperdício garantido. |
| ⚪ Backlog | 2, 3, 15, 17-21, 23-25 | Mutation testing, latência, inconsistências de doc. |

**Leitura do lead:** o valor do handoff não é a lista — é a confissão final,
*"os 3 PRs mergeados podem ter regressões"*. Podiam e tinham. A ordem correta é
**consertar o que quebrou → re-cortar → então blindar**, nunca o inverso.

---

# PARTE II — Inventário completo (consolidado após 3 relatórios)

> A Parte I foi escrita antes de dois relatórios chegarem. Esta parte consolida
> **tudo** que está aberto, de **todas** as fontes, verificado contra `origin/main`.
> Regra: nada some. Todo item tem destino explícito.

## II.1 — O briefing original de 15 WPs: status medido

Medido pela convenção de branch do próprio briefing (`claude/fix-<WP>`).

| Estado | WPs | Observação |
|---|---|---|
| ✅ Mergeados | A, B, C, D1, E, F1, K | 7 de 15 |
| ✅ Mergeado por OUTRA campanha | **G** (#1379) | Duplicação: a auditoria Edition-2.0 chamou o mesmo defeito de "F-020". Dois nomes, um defeito, trabalho duplicado não contabilizado. |
| 🔴 Abertos e **TODOS defeituosos** | F2, H, I, J, L, M | 6 de 6. Ver II.2. |
| ⚪ Nunca iniciado | **D2** (replay de webhook) | Dependia do D1, que **está mergeado** desde então. Desbloqueado, não feito. |

## II.2 — Os 6 WPs abertos: defeito nomeado por PR

Cada um auditado **e refutado** (dupla verificação independente).

| PR | WP | Defeito confirmado |
|---|---|---|
| #1399 | L | **Não compila** (`too_many_arguments`). Além disso o consumidor (`audit_drain_cron.ts`) **nunca lê** o campo corrigido — o fix é invisível em produção. |
| #1400 | J | **Não compila** (`single_component_path_imports`). Conserta um fail-open e **abre outro**: `deleted` sem `status` retorna `Ok(())`, some da fila de recuperação, e a Stripe nunca reenvia. |
| #1402 | H | Deixa **dois workflows estruturalmente inválidos** — `workflow_dispatch` des-indentado para fora de `on:`. `actionlint` vermelho. Os 19 grupos de concorrência são estáticos, não chaveados por ref: dois PRs cancelam o gate um do outro. |
| #1396 | M | **Viola instrução explícita do contrato.** O briefing exigia PROVA antes de conserto e "PARE se não provar". O teste existe mas **passa vaziamente** — nunca afirma que a chave-vítima foi removida. E o fix recria o mapa sem limite que diz consertar. |
| #1398 | I | I.1 atinge **todo tenant sem tier resolvido** — o estado normal do plano gratuito — com ida a D1 em **toda** requisição, num hot path medido em ~95 ms. |
| #1395 | F2 | O teste **nunca executou** (6 checks, PR conflitante). Contém asserção morta (`.length` sobre booleano → sempre passa). Risco de deploy: chave com newline vira 503 duro em todo o plano PAT. |

**Nenhum entra sem conserto.** Ordem forçada: `#1395 → #1396 → #1398` é pilha acidental — mergear um mergeia os três.

## II.3 — Cegueira estrutural de portões (o padrão dominante)

Quatro portões que **não conseguem ver aquilo que existem para pegar**. Passam verde
enquanto o defeito existe — pior que não ter portão, porque produzem confiança.

| Portão | Cegueira | Destino |
|---|---|---|
| OKF `CITE_RE` | Regex exige caminho ⇒ nunca casa citação abreviada `:N-M` | **B-059** — ⚠️ ainda NÃO existe em `origin/main`; vive em ramo não mergeado. Precisa ser **criado**, não fechado. |
| OKF C5 | Valida **posição**, não correspondência semântica — citação pode apontar para rota errada e ficar verde para sempre | **novo item** |
| `sdk-artifacts` | Filtrado por `apps/docs/**` ⇒ **nunca dispara no PR que causa** a defasagem | **novo item** |
| Espelho CF-1 | Unidirecional (migrations→registry) ⇒ tabela fantasma no registro é invisível | **B-060** — ⚠️ idem: não existe em `origin/main` ainda. |

**Contra-exemplo a imitar:** o C10b **recusa** seed cujo conceito não cite o arquivo —
ou seja, recusa auto-certificação. É o desenho correto; usar como referência ao
consertar os outros quatro.

## II.4 — Integridade do OKF

| Item | Medido |
|---|---|
| **63 de 164 conceitos (38%)** com `checkpoint_sha` **inalcançável** | Medido em `origin/main`: para cada conceito com `checkpoint_sha`, `git merge-base --is-ancestor <sha> origin/main` falha em 63. **Correção de escopo:** eu havia registrado "13", que era só a contagem de **dois SHAs específicos** vindos do #1408. A população real é 4,8× maior, e o item de backlog B-049 já a tinha medido em 57/161 **antes** deste roadmap existir. Não é acidente pontual — é **defeito sistêmico de squash-orphaning**, e o conserto tem de incluir a prevenção (falhar `validate_okf` em âncora inalcançável), não só a re-ancoragem. |
| `index.md` gerado afirma **162**; disco tem **165** | Defasado em 3; é o índice que uma pessoa navega |
| `planes/container.md:132` cita `main.rs:661-681` como mount do archive | É a **rota de quota**; o archive mounta em 622-639 |

## II.5 — Decisões que são do owner (não implementar)

| # | Item | Estado verificado |
|---|---|---|
| OWNER-1 | GC nunca executa; `blob_meta` sem escritor no binário deployado | Confirmado; já documentado em ADR no próprio HEAD |
| OWNER-2 | 13 arquivos `gc_worker/` "sem backup em lugar nenhum" | ⚠️ **RESOLVIDO por acidente** — sumiram do disco mas estão em `c98ca0f4`, varridos pelo `git add -A` do #1397. **O defeito preservou o trabalho.** Arquivar antes de descartar o #1397. |
| OWNER-3 | Herança de `[triggers]` põe cron de chaos em prod | **NÃO adicione handler `scheduled()`** — hoje é inerte só por isso |
| OWNER-4 | Reembolso/disputa não revogam acesso | Decisão de política de produto |
| OWNER-5 | Sem reconciliação de entitlement Stripe↔D1 | Trabalho novo, custo recorrente |
| — | **Branch protection ausente nos 3 repos** (`403 Upgrade to GitHub Pro`) | Nada do lado do GitHub impede merge. O portão é inteiramente disciplina. |
| — | Respostas de compliance revertidas (`Y→N`) em questionários já enviados a clientes | Exposição contratual — counsel |
| — | Add-on BYOK a $99/mês para funcionalidade que retorna **501** | Comercial/legal |
| — | Commit com assinatura DCO fabricada na `main` local do checkout raiz | Contido (não está em ref remota); descartar |

## II.6 — Declarados NÃO VERIFICADOS (verificar, não implementar)

O briefing original os deixou de fora **de propósito**. Continuam abertos como
desconhecidos declarados — não como resolvidos.

- **MED-3** — pad de timing duplicado entre middleware e verifier
- **MED-4** — propagação de scope/`find_only` com até 60 s de staleness, sem ADR
- **PERF-1** — alegação de 2 inserts D1 síncronos inline por operação de cache
  (~100–400 ms). Mesmo se verdadeira, o trade-off é arquitetural: fire-and-forget
  enfraquece o invariante audit-before-mutation. **Não é conserto óbvio.**
- **Toda a lista LOW** — dado o índice de erro do relatório original (base errada,
  2 linhas de tabela falsas, 2 achados apontando para arquivos inexistentes), tratar
  cada item como não verificado.

## II.7 — Higiene de CI (medida)

| Item | Número |
|---|---|
| Lanes com **zero sucesso na vida inteira** | 5 workflows, **1.057 execuções** |
| `cosign-sign.yml` — assinatura de release | **0 execuções na vida**, apesar de gatilho `push` |
| `cas-canary` | Job hosted `skipped` ⇒ workflow **verde sem testar** o caminho off-fabric |
| Crons redundantes (já cobertos por PR/push) | 5 |
| `docs/internal/sdk-publishing.md` | **Não existe** — mas é o único doc que a mensagem de erro do portão manda ler |

## II.8 — O que ainda NÃO tem dono

1. **WP-D2** (replay de webhook) — desbloqueado, nunca iniciado
2. **Re-corte do devenv** — 1.377 linhas úteis de 28.642
3. Os **4 consertos de cegueira de portão**
4. **Verificação** de MED-3, MED-4, PERF-1 e da lista LOW
5. ~~Um quarto relatório ainda por chegar~~ — **ele já estava no repo.**
   `reports/audits/2026-08-26-tail-verification.md` está em `origin/main` e sua base
   (`8cd0f920`) é **ancestral** da base deste roadmap. Ele resolve parte do balde
   "não verificados" do §II.6 — confirma **MED-4** como real (exige ADR), onde este
   documento ainda o chamava de não-verificado. E produz um item que vira trabalho e
   **não tem dono**: **P-A7** — `worker/src/index.ts` provisiona uma sessão de réplica
   de leitura D1 usada só no lookup de auth, nunca repassada às leituras de tier/quota
   (`:3532`, `:4054`), custando uma ida a região distante por request autenticado.
   O relatório o destina a "WP-F2+", que **não existe** neste roadmap.
6. **Itens de `BACKLOG.md` abertos sem referência aqui:** B-057 (fluxo de SLI de CAS/AC
   sem consumidor, sem latência real, sem limite — mesma família das cegueiras do §II.3),
   B-055 (F2 na borda não emite SLI), B-048 (`mutation-pr.yml` com fetch depth-1 alimenta
   diff de três pontos errado — o portão pode passar sobre base velha), B-053 (piso de 50
   amostras torna região de baixo tráfego **não-failoverável**), B-056 (sem orçamento de
   leitura CAS por processo).
7. **Dois `TODO(owner)` em `worker/src/replication_coordinator_do.ts`** (`:446` auditoria
   de promoção só em stdout, precisa binding D1; `:589` `/_repl/*` compartilha a chave
   interna com mint de PAT admin) — ausentes da tabela de decisões do owner.

---

# PARTE III — Os achados da auditoria Edition-2.0, e por que este documento não deve envelhecer

## III.0 — Como ler este documento (a correção de desenho)

As Partes I e II ficaram **desatualizadas duas vezes em poucas horas**. Não por descuido:
por desenho. Elas repetiam **estado** (quais PRs abertos, o que já mergeou), e estado
muda. É o mesmo defeito do índice OKF (afirma 162, disco tem 165), do `CLAUDE.md`
(afirma ~73 crates, são 75) e do briefing original (medido contra base velha).

**Regra a partir daqui: este documento registra DECISÃO e CONTRATO, nunca estado.**
Decisão e contrato não mudam sozinhos. Estado se consulta:

```bash
gh pr list --state open --json number,title,mergeable   # o que está aberto
gh pr list --state merged --search "<termo>"            # o que já entrou
bash scripts/pre-merge-gate-check.sh <PR>               # se pode entrar
```

Se você encontrar uma tabela de estado numa parte anterior deste arquivo e ela
discordar do comando acima, **o comando ganha**.

## III.1 — Rastreabilidade da Edition-2.0

A Parte II mediu o briefing de 15 WPs. Mas **uma segunda auditoria** (Edition-2.0,
`reports/audits/2026-08-26-go-live-readiness.md`) produziu achados `F-001..F-020` que
não estavam registrados aqui — o trabalho existia, o registro não. Cada um agora tem
destino.

| Achado | Assunto | Destino |
|---|---|---|
| F-001 | Claims de WORM/Object-Lock na cadeia de auditoria | **Fechado** — claims rebaixados; keyed-hash deferido como epoch cutover (B-054) |
| F-002/003 | gRPC e BYOK anunciados vs 501 | Em PR de claims (docs) |
| F-004 | Hostnames mortos no corpo do 429 | **Fechado** |
| F-005 | "4+3 regiões" vs 2 vivas | Em PR de claims (docs) |
| **F-006** | **Taxonomia de pricing divergente** | ⚠️ **SEM DONO** — medido: `tier.rs` tem 6 tiers, `ratelimit/tier.rs` tem 5. Mesmo rótulo, faturamento diferente. |
| F-007/008 | `_public` sem cap + cap velho após downgrade | Metade npm feita; a metade `_public` é redesenho de accounting (ver II.8) |
| **F-009** | Turborepo sem verificação de conteúdo | **NÃO É DEFEITO** — ver III.2 |
| **F-010** | **Reembolso não revoga acesso** | ⚠️ **SEM DONO** — mesma decisão que OWNER-4 |
| F-011 | DLQ volátil | **Fechado** (WP-D1) |
| F-012 | Chaves privadas em `~/Downloads` | Owner |
| F-013 | Drift da matriz de secrets | **Refutado** no briefing (`code_only=0`) |
| F-014 | Feeds de CVE sem cron | **Fechado** |
| F-015 | Janela de revogação de 65 s | Parcial (WP-F1); o piso de TTL do KV é 60 s, não dá pra baixar |
| F-016 | Enum de região 4 vs 6 | **Fechado** |
| F-017 | Deploy sem drain (SIGTERM) | Em PR, **reprovado** por auditoria independente (afirmação falsa de fallback + fila de billing não drenada) |
| F-018 | Hash do DPA forjável | **Bloqueado** — exige decisão de arquitetura (fonte canônica do texto) |
| **F-019** | **Ticket de credencial 2 h reusável** | ⚠️ **SEM DONO** — o pedido real é TTL 7200→600 + `destroy`, não uso único |
| F-020 | Flood de `kid` no JWKS | **Fechado** — e ver III.3 |

## III.2 — F-009: retratação registrada

Escalei este achado a P0 com base num `grep` por três palavras retornando zero.
**Estava errado, e o erro é o mesmo que este documento condena:** concluir ausência a
partir de busca estreita.

Verificação correta: o módulo **documenta o comportamento** nas próprias linhas 41-43 —
*"Turbo's artifact hash is OPAQUE. It is stored verbatim as the KV key without any
hash-integrity verification."* A camada de armazenamento **tem** verificação
(`verify_content_hash`, 24 ocorrências); o turbo deliberadamente não a usa.

E não pode usar: no protocolo do Turborepo o hash é dos **inputs da tarefa**, não do
artefato. O servidor não tem os inputs, logo não recomputa. A correção sugerida pela
auditoria (`blake3(bytes) verify`) **não é implementável** — não há contra o que comparar.

Severidade real: exige credencial de escrita do tenant **e** pertencer ao mesmo time.
Não é quebra cross-tenant. O serviço hospedado do próprio Turborepo tem a mesma
propriedade. **É suposição de confiança, não bug.** O que sobra é dever de informar o
cliente; mitigar de verdade seria escopo de escrita por usuário — trabalho novo.

## III.3 — Trabalho duplicado, medido

**F-020 e WP-G são o mesmo defeito.** Duas auditorias independentes, dois nomes, duas
sessões consertando, nenhuma contabilização. O custo só apareceu quando alguém cruzou
as duas listas.

Isso torna a rastreabilidade cruzada obrigatória: **antes de despachar, consulte se o
trabalho já existe.** Aconteceu de novo hoje — duas instruções pediram consertos
(`chacha20` no runners; republicar o pacote SDK) que já estavam prontos e verdes em PR.
Verificar antes custou dois comandos; duplicar teria custado dois PRs e duas revisões.

## III.4 — Portões estruturalmente cegos: agora são seis

Atualiza a tabela de II.3. É o formato de defeito **dominante** desta casa.

| Portão | Cegueira |
|---|---|
| OKF `CITE_RE` | Regex exige caminho ⇒ citação abreviada é invisível |
| OKF C5 | Valida posição, não correspondência semântica |
| `sdk-artifacts` | Filtrado por `apps/docs/**` ⇒ nunca dispara no PR que causa a defasagem |
| Espelho CF-1 | Unidirecional ⇒ tabela fantasma é invisível |
| **Piso de cobertura** | O bloco `thresholds` existia; o workflow chamava `vitest run` **sem `--coverage`** ⇒ **nunca executou uma vez**. Custo medido: um arquivo documentado a 90% mede 77,13% |
| **Escopo de cobertura** | O provider só reporta arquivo **importado** por algum teste ⇒ arquivo novo sem teste não move o número. Medido: 88,23% vs 79,09% |

**Contra-exemplo a imitar: o C10b.** Ele **recusa** seed cujo conceito não cite o
arquivo — recusa auto-certificação. É o único desenho correto encontrado, e deve ser o
modelo ao consertar os outros.

Pergunta obrigatória em toda revisão de portão daqui pra frente:
**"o que quebraria isto e NÃO seria pego?"** Se não houver resposta, o portão não foi
revisado — foi lido.

---

# PARTE IV — O quarto briefing (dispatch ox alpha), e a sétima cegueira

## IV.1 — Status: NÃO executável como escrito, e **2 de 14 feitos**

O quarto documento recebido é um runbook de dispatch (`OX-ALPHA-FIX-PLAN.md` +
`OX-ALPHA-DISPATCH.md`), com 14 WPs numerados atravessando os três repos. Três
impedimentos, todos medidos:

| # | Impedimento | Medição |
|---|---|---|
| 1 | **A spec não existe** | Os dois arquivos viviam em `/var/folders/.../T/opencode/audit/` — diretório temporário do sistema, **apagado**. O briefing manda entregar os pacotes PART-B **verbatim**; sem os arquivos, executá-lo exigiria **inventar** o conteúdo, que é o que ele proíbe. |
| 2 | **A base fixa está 54 commits atrás** | Manda ramificar de `8cd0f920`; a `main` estava em `dc0e5e0e`. Ramificar dali reverteria tudo que entrou desde então. |
| 3 | **Só 2 de 14 existem** | Branches sob `go-live/wp-*`: **WP-3 mergeado** (#1345), **WP-2 aberto** (#1356). Os outros 12 não têm branch. `WP-9` (runners) e `WP-13` (workspaces) — buscados nos repos respectivos — **nunca foram feitos**. |

### ⚠️ Retificação registrada

Eu havia reportado que o trabalho deste briefing estava *"majoritariamente feito"*.
**Estava errado.** Concluí isso a partir de **duas branches**, e confundi este briefing
(14 WPs numerados, 3 repos) com o **terceiro** (15 WPs por letra, `claude/fix-WP-A..M`).
Os PRs #1347-1352 e #1364 pertencem ao **terceiro**, não a este.

É o mesmo erro que este documento condena — **conclusão a partir de leitura parcial** —
e é a quinta vez que ele aparece nesta campanha. Peguei ao duvidar de mim antes de
escrever, não durante a análise original.

**Status real deste briefing: 1 mergeado, 1 aberto, 12 sem branch.**

### Contradição de papel — não resolvida por mim

O briefing diz *"Você é o tech lead… e **NÃO mergeia nada**"*. O owner nomeou este lead
**guardião único de merge dos 3 repos**. São instruções opostas.

**Leitura adotada:** aquele "não mergeia" era instrução ao **ox alpha**, e o documento
chegou como material histórico, não como ordem nova. **O lead segue mergeando.** Fica
registrado como pressuposto explícito, não como decisão inventada — se estiver errado,
é o owner quem corrige.

### Item deste briefing que virou ação real

**#1356 (WP-2, purga de claims falsos)** e **#1376** estavam travados no portão
`published SDK artifacts`. Esse portão foi consertado (o pacote npm servido do próprio
domínio estava defasado). Os dois precisam apenas de **rebase** para destravar — parados
há dias por dívida que não era deles.

## IV.2 — A SÉTIMA cegueira, e por que ela explica os 63/164

Atualiza §III.4. Esta é a mais cara encontrada.

**`validate_okf` fica VERDE localmente enquanto a CI o vê vermelho.** Cada rebase
reescreve os commits e órfã todo `checkpoint_sha` — mas os objetos órfãos **continuam
existindo no clone local**, então a checagem local os alcança e passa. A CI clona limpo,
não os alcança, e reporta stale.

**O único check local que concorda com a CI:**

```bash
git merge-base --is-ancestor <checkpoint_sha> origin/main   # por conceito
```

**Consequência medida:** 63 de 164 conceitos (38%) com âncora inalcançável em
`origin/main`. Não é acidente de um merge — é **toda pilha rebaseada de toda campanha**,
apodrecendo sem nunca ficar vermelho onde alguém olhasse. Uma única sessão pagou 3
rebases num dia e órfã 10 conceitos.

**Isso muda o WP-5.** Re-ancorar os 63 sem prevenção recria o problema na próxima rodada
de rebases. O conserto tem de incluir:
1. `validate_okf` **reprova** âncora inalcançável (hoje ele cai em silêncio para base-ref);
2. **preferir blob anchor** — ele é content-addressed e **sobrevive ao rebase**; só o
   commit anchor move. Tratar commit anchor como fallback, não como padrão.

## IV.3 — Régua de merge ajustada

`git merge-tree --write-tree` retornou **exit 0 e uma tree** enquanto o GitHub reportava
`CONFLICTING` no mesmo par. Não sei ainda qual estava certo sobre o quê — a hipótese de
renames é minha, **não medição**.

**Regra: `merge-tree` serve para "conflita?" barato e não-destrutivo, ANTES de mexer.
Não decide merge.** Consultar os dois e crer no **mais pessimista**.
