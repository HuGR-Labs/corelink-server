# CoreLink — Roadmap canônico de remediação

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
