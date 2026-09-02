# Repo-global `concurrency.group` cancelava runs entre PRs irmãos

**Status: PARCIALMENTE corrigido.** O #1503 (`1715651c`) tirou 14 dos 17 casos. **Três
continuam vivos na `main`** (§5), invisíveis porque o instrumento — o scanner deste
relatório — falhava aberto sobre eles. Este documento é o registro do diagnóstico, a revisão
fria retroativa do #1503, e a correção do próprio instrumento.

| | |
|---|---|
| Defeito introduzido por | #1402 (`50741f7c`, sub-fix H.3), 2026-08-27 |
| Corrigido por | #1503 (`1715651c`), 2026-08-31 |
| Revisão fria | retroativa, pós-merge — ver §7 |
| Veredito | **#1503 APROVADO com residuais.** Nenhum é regressão. |
| Revisão do próprio relatório | um SEV-1 no **scanner** (§5), achado por revisor independente e consertado aqui |
| Ref da medição histórica | `origin/main` @ `4befbe76` |
| Ref da atualização de integração | `origin/main` @ `fdd4b74176895ef6759e2a58f676b8fe59c64fa3` |

---

## Atualização de integração — 2026-09-02

O corpo histórico abaixo permanece preso à ref que mediu; não deve ser lido como
uma fotografia da `main` atual. Nesta integração, a varredura foi refeita contra
`fdd4b74176895ef6759e2a58f676b8fe59c64fa3`:

```
scanned blocks=124 cross-ref-safe=93 not-cross-ref-safe=31 VIOLATIONS=3
```

As três violações continuam sendo `workspace-lint`, `runner-fleet-health` e
`okf_nightly`, todas com `group: ${{ github.workflow }}` e cancelamento em voo.
Elas não são corrigidas por este relatório nem pelo scanner; pertencem ao item
de backlog B-172. A colisão por evento de grupos que já incluem `github.ref`
continua separada em B-150.

O scanner também foi endurecido antes de reutilizar essa medição. Uma expressão
de matriz e `github.sha` não provam separação por ref; o primeiro costuma repetir
o mesmo eixo em refs distintos e o segundo pode apontar para o mesmo commit. Os
dois agora são violações, não uma passagem implícita. O `--self-test` constrói
casos sintéticos para grupo literal, nome de workflow, matriz, SHA, comentário de
cauda, ref completo, run único, número de PR, comentário após `concurrency:`,
referência escrita só como literal e diretório vazio. Expressões só contam quando
referenciam o contexto de fato — `ci-github.ref` e `${{ 'github.ref' }}` não são
isolamento — e chaves duplicadas ou YAML ilegível fazem a varredura falhar, nunca
escolher silenciosamente a primeira. Assim o zero só é aceito quando o próprio
instrumento ainda consegue ficar vermelho.

---

## 0. Leia primeiro — a armadilha que quase enterrou este achado

A primeira resposta desta investigação **estava errada**, e o modo de errar é
repetível — por isso abre o documento.

A auditoria rodou contra a **árvore de trabalho local**, que estava em `4602be1d`,
**71 commits atrás** de `origin/main`. Nesse ref o `backlog-verify.yml` realmente
não tem bloco `concurrency:`, e a varredura realmente devolve zero violações. A
conclusão emitida foi *"premissa refutada"*. O scanner estava certo — ele até
tinha teeth test. O **ref de entrada** é que estava velho.

Contra `origin/main`, o mesmo scanner devolvia **14 violações**.

**Antes de concluir qualquer ausência neste repo:**

```bash
git fetch origin && git rev-list --count HEAD..origin/main
```

Se isso não imprimir `0`, não audite a sua árvore. Esta é a quarta roupa da mesma
armadilha nesta campanha — `head -8` truncando um controle validado, `gh`
devolvendo vazio com exit 0 no rate limit, laço quebrado imprimindo "nenhum
achado", e agora ref defasado. **Todas produzem ausência falsa com o instrumento
aparentemente são.**

---

## 1. O defeito

`.github/workflows/backlog-verify.yml`, linhas 15–17, antes do #1503:

```yaml
concurrency:
  group: "ci-backlog-verify"
  cancel-in-progress: true
```

A chave do grupo é **constante** — sem `github.ref`, sem `github.head_ref`, sem
`github.event.pull_request.number`. O GitHub escopa grupo de concorrência **por
repositório**, então chave constante torna o grupo **repo-global**: existe no
máximo um run de `backlog-verify` no repositório inteiro a qualquer instante, e
`cancel-in-progress: true` faz um run novo **cancelar o run em voo de todos os
outros PRs**.

É o oposto da intenção do ajuste. `cancel-in-progress` existe para superar
*revisões velhas do mesmo PR*. Aqui superava *o PR dos outros*.

### Por que passa de ruído

`cancelled` não é desfecho neutro. O merge gate trata como não-verde — e está
certo, um check que não terminou não decidiu nada — e o `gh pr checks` joga no
balde `fail`. Com vários PRs tocando `BACKLOG.md` ao mesmo tempo, o portão fica
**estruturalmente incapaz de ficar verde em qualquer um deles**, e re-run não é
remédio porque o run do próximo PR cancela o re-run.

O portão não fica vermelho: fica **AUSENTE**. E cancelado parece "sem falha" para
quem pergunta *"tem falha?"* em vez de contar o conjunto.

---

## 2. Evidência do diagnóstico

### 2.1 Procedência

Introduzido pelo **#1402**, commit `50741f7c`, sub-fix **H.3**:

> H.3 adicionado blocks 'concurrency:' a 19 workflows que faltavam.

A intenção era boa — aqueles 19 não tinham controle de concorrência nenhum. A
execução usou chave constante onde precisava de chave por ref.

### 2.2 Raio de alcance entre branches

O GitHub avalia um run de `pull_request` contra a **head do PR**, e um
`workflow_dispatch` contra o **ref selecionado** — não contra a `main`. Então os
branches importam por si:

```
branches varridos = 146
branches carregando group: "ci-backlog-verify" = 77
```

Isso importa para a verificação: um run só pode ser cancelado *por* um grupo se o
próprio ref daquele run declara o grupo. Todas as vítimas observadas declaravam.

### 2.3 O comportamento observado batia com o mecanismo

`gh run list --workflow=backlog-verify.yml`, janela 03:54–04:25 UTC de 2026-08-31.
Os cancelamentos coincidem com **outros** runs começando, em segundos:

| run cancelado | branch | cancelado às | run que começou | branch dele | Δ |
|---|---|---|---|---|---|
| `33355421818` | `wp-a-b080-scopes` | 03:56:56 | `33355450839` | `wp-a-b075-devenv` | 4s |
| `33355491121` | `wp-f-audit-gc` | 03:57:48 | `33355496034` | `wp-a-b080-scopes` | 2s |
| `33355897397` | `wp-a-b075-devenv` (dispatch) | 04:06:02 | `33355922227` | `b132-abbrev-cite-resolver` | 2s |
| `33356199294` | `wp-h-okf` | 04:14:02 | `33355496034` (tentativa 2) | `wp-a-b080-scopes` | 0s |
| `33356978570` | `wp-a-b080-scopes` | 04:24:42 | `33356982902` | `main` (push) | 1s |

Os pares entre branches distintos são a assinatura. A última linha é a mais clara:
um **push na `main`** cancelou o run de um **PR**. Só grupo repo-global faz isso.

Três confirmações no mesmo conjunto:

- Runs que acabaram passando só passaram na **tentativa 2 ou 3**, re-rodados às
  04:17–04:20, quando a tempestade acalmou.
- Três `workflow_dispatch` distintos no `claude/wp-a-b075-devenv` foram cancelados.
  Grupo por ref nunca faria isso — o grupo constante põe até dispatch manual para
  disputar com run de PR.
- As durações são curtas e **pós-admissão** (5s, 13s, 17s, 28s, 32s): os runs foram
  admitidos e então superados, não passaram fome na fila.

---

## 3. O conserto que entrou (#1503)

### 3.1 `backlog-verify.yml`

```yaml
concurrency:
  group: "ci-backlog-verify-${{ github.ref }}"
  cancel-in-progress: true
```

Push novo no **mesmo** PR ainda cancela o run velho daquele PR — o comportamento
para o qual o ajuste existe. Os outros PRs ficam intactos.

### 3.2 Os outros 13

Escopados com `${{ github.event.pull_request.number || github.ref }}`.

A leitura inicial de que *"são cron ou dispatch, a serialização é desejada"* estava
**errada**, e essa correção é o miolo do #1503: dois `workflow_dispatch` do **mesmo**
workflow em **branches diferentes** se cancelam. O grupo constante é repo-global
independentemente do gatilho — foi o que matou três dispatches em 2026-08-31.

### 3.3 `release-slsa3` — restauração de intenção, não escopo

O commit `50741f7c` declara que seis workflows de prod-build/signing/notarize
**omitem** `cancel-in-progress` porque *"cancelar deploy/sign em-flight é pior que
enfileirar o próximo"*. Cinco honravam. `release-slsa3` **não** — uma build de
proveniência SLSA L2 em voo podia ser cancelada por um segundo release.

O #1503 **removeu o flag** em vez de escopar o grupo. Correto: alinha com os cinco
irmãos e com a intenção declarada, em vez de trocar a política.

### 3.4 Os 26 que não foram tocados

Dos 40 grupos constantes originais: **21 com `cancel-in-progress: false`** e
**5 com o flag ausente** (`cf-deploy-prod`, `container-build-push-prod`,
`notarize-macos`, `sign-linux`, `sign-windows`). Constante **sem** cancelamento é o
idioma **correto** de singleton diário: **serializa em vez de matar**. Escopá-los
por ref deixaria duas execuções da mesma noturna se sobreporem.

O #1503 não tocou nenhum. Verificado em §7.

---

## 4. Reproduzindo a varredura

O scanner parseia blocos `concurrency:` **estruturalmente**, não por linha. Isso
importa: a receita ingênua (`group:` menos `${{`) somada a `grep -A1` erra, porque
`-A1` também emite a linha `cancel-in-progress` de blocos **interpolados** enquanto
`grep -v '${{'` apaga a linha `group:` que era dona dela. Produziu um "63" falso
durante esta investigação. **Conte blocos, não linhas.**

```bash
git fetch origin && git rev-list --count HEAD..origin/main && python3 scripts/concurrency_scope_scan.py
```

Saída em `origin/main` @ `4befbe76`:

```
scanned blocks=123 ref-scoped=92 not-ref-scoped=31 VIOLATIONS=3
```

As 3 violações são o SEV-1 da §5 — grupos `${{ github.workflow }}` que o #1503 nunca
tocou. Os 27 grupos que o #1503 deixou seguros continuam seguros:
`21 (cancel-in-progress: false) + 6 (flag ausente)`, sendo os 5 de release/signing mais o
`release-slsa3`. Os outros 4 do `not-ref-scoped=31` são as 3 violações mais o `stale.yml`,
que usa `stale-${{ github.workflow }}` com o flag **desligado** — idioma correto.

### 4.1 Calibração — não acredite no zero antes de ver o um

O resultado acima só vale porque o instrumento foi provado em oito eixos. Os cinco
primeiros existiam na versão anterior; os três últimos nasceram do SEV-1 da §5.

| # | entrada | esperado | obtido |
|---|---|---|---|
| 1 | `origin/main` | as 3 do SEV-1 | `123 · 92 · 31 · VIOLATIONS=3` |
| 2 | estado pré-fix (`1715651c^`) | **17** violações | `VIOLATIONS=17` |
| 3 | grupo constante literal | acusa | acusa |
| 4 | violação escondida por comentário de cauda | acusa | acusa |
| 5 | diretório sem workflow | **falha**, nunca "limpo" | `FATAL`, exit 1 |
| 6 | `group: ${{ github.workflow }}` sozinho | **acusa** | acusa |
| 7 | `group: ${{ github.workflow }}-${{ github.ref }}` | passa | passa |
| 8 | `cancel-in-progress:` como `True` / `"true"` | acusa as duas | acusa as duas |

O eixo 2 é o que mais mudou: a versão anterior media **14** ali. O número certo sempre foi
**17** — ela não enxergava as 3 da §5, nem antes nem depois do #1503.

O eixo 5 é a cláusula anti-vacuidade: uma lista vazia é a forma mais comum de uma varredura
mentir, então não obter arquivo nenhum é falha nomeada, não sucesso. Um literal que não seja
booleano nem expressão (`cancel-in-progress: maybe`) **levanta** em vez de ser adivinhado —
um scanner que adivinha é um scanner que reporta limpo sobre valor que não entendeu.

## 5. O SEV-1 do próprio scanner: ele falhava ABERTO sobre 3 violações vivas

Esta seção é o resultado da revisão fria **deste relatório**, não do #1503. O revisor
reproduziu as medições empíricas — as ids de run conferidas uma a uma, a aritmética
`40 = 21 + 14 + 5`, os eixos de calibração — e nada disso caiu. O que caiu foi o instrumento.

**O scanner perguntava apenas se o grupo continha `${{`.** Mas `${{ github.workflow }}` é o
**nome do workflow — uma constante.** Um grupo feito só dele é repo-global exatamente como
uma string literal, e passava. Três workflows vivos na `main` estão nessa forma, todos com
`cancel-in-progress: true`:

| workflow | grupo | gatilhos |
|---|---|---|
| `workspace-lint.yml` | `${{ github.workflow }}` | `pull_request`, `push`, `workflow_dispatch` |
| `runner-fleet-health.yml` | `${{ github.workflow }}` | `pull_request`, `schedule`, `workflow_dispatch` |
| `okf_nightly.yml` | `${{ github.workflow }}` | `schedule`, `workflow_dispatch` |

**Não é teórico.** `workspace-lint` acumula **7 cancelados / 2 falhas / 38 sucessos**, e
**6 dos 7 cancelados são `push` na `main`** — merges consecutivos matando o
`clippy --workspace -D warnings` um do outro. Dois seguidos em `2026-08-30T04:34:46Z` e
`04:35:32Z`. É literalmente o modo de falha da §1: o portão não fica vermelho, fica
**ausente**.

Então o `VIOLATIONS=0` que a versão anterior deste relatório exibia era **verde falso**, e o
número de capa da campanha nunca foi 14 — é **17**. As 3 nunca foram tocadas pelo #1503
porque ele também só procurou grupo literal.

**Interpolar não é variar.** A versão registrada nesta seção usava uma allowlist
(`REF_VARYING`) para exigir algo que parecesse diferir entre refs, em vez de deixar qualquer
expressão passar. Isso corrigiu o verde-falso do nome de workflow, mas a atualização de
integração acima identificou que `matrix.` e `github.sha` também não demonstram separação por
ref. O scanner atual aceita somente um discriminador cuja segurança pode ser justificada pelo
seu significado (ref completo, run único, ou número de PR em workflow exclusivamente de PR),
e recusa o resto até haver essa justificativa.

### 5.1 O SEV-2 do booleano

`cancel_value == 'true'` não reconhecia `True` nem `"true"`, ambas escritas legais para a
mesma coisa — e o Actions coage a segunda de qualquer jeito. Nenhuma existe no repo hoje, o
que a tornava **latente**: o docstring prometia fail-closed e não era. Agora o valor passa
por `yaml.safe_load`, e um literal que não seja booleano nem palavra truthy/falsy conhecida
**levanta** em vez de ser adivinhado.

### 5.2 Correção de atribuição — a explicação anterior estava no mecanismo errado

A versão anterior desta seção creditava o conserto do verde-falso ao filtro que descarta
linhas de comentário. **Esse filtro é inerte**, e o revisor está certo: o `re.match` já está
ancorado no início da linha, então `  # cancel-in-progress: false` nunca casa, com ou sem o
filtro. Medido:

```
linha-comentario inteira   substring(v1)=True   re.match=False
comentario citando o par   substring(v1)=True   re.match=False
comentario de CAUDA        substring(v1)=True   re.match=True
```

Quem conserta o caso de **linha inteira** é a **âncora do `re.match`**, que substituiu a
busca por substring da v1. Quem é load-bearing é o **`strip_comment`**, mas contra outra
coisa: comentário de **cauda** no valor (`cancel-in-progress: true  # antes era false`). Sem
ele o valor comparado seria `true  # antes era false`, que não casa com nenhuma escrita
conhecida e sairia **silenciosamente não-violante** — outro verde falso, por outro caminho.

O conserto era real; a explicação apontava para o mecanismo errado. Registro porque um
relatório que acerta o resultado e erra a causa ensina a coisa errada ao próximo leitor.

## 6. Limites declarados do scanner

Ditos aqui para que o exit code não seja lido como mais do que é.

- Só blocos `concurrency:` **de topo**. `concurrency:` a nível de job não é olhado.
- **O grupo pode carregar `github.ref` e ainda colidir.** `github.ref` é
  `refs/heads/<padrão>` para `push`, `schedule` **e** `workflow_dispatch` igualmente, então
  os três colapsam num grupo só. O scanner **passa** isso. Não é resíduo de canto:
  a população é medida pelo verify vivo de B-150. B-136 e B-137 fecharam o recorte original
  de seis workflows; não atribua a eles a população remanescente.
- Comentários: ver §5.2 para qual mecanismo cobre o quê.

A versão anterior desta seção dizia que o scanner "não verifica se um grupo interpolado de
fato *varia* por ref". A frase estava certa e **subestimava**: fazia um SEV-1 — grupo sem
componente algum de ref passando por conter qualquer expressão — parecer um SEV-3 sobre
granularidade. Um cabeçalho que descreve o próprio buraco como menor do que ele é fica mais
perigoso que buraco nenhum, porque compra confiança que não sustenta.

## 7. Revisão fria retroativa do #1503

Feita por sessão que diagnosticou o mesmo defeito de forma independente. O #1503 foi
escrito, gateado e mergeado pela mesma sessão, sem revisão adversarial — furo da
regra do owner, corrigido aqui a posteriori.

### Veredito: **APROVADO com residuais**

O PR é uma **melhora estrita**. Nenhum residual abaixo é regressão: cada um já
existia antes, em forma pior.

| Verificação | Resultado |
|---|---|
| Varredura pós-fix, dos 14 | os 14 saíram; nenhum voltou |
| Varredura pós-fix, do repo | `VIOLATIONS=3` — **fora** do escopo do #1503, ver §5 |
| Teeth test re-rodado | acusa **17** no estado pré-fix; oito eixos, anti-vacuidade ativa |
| Os 26 intocados | **conjunto idêntico** antes/depois, byte a byte |
| Diff toca só concorrência | **sim** — zero linhas adicionadas ou removidas fora de `concurrency:`/`group:`/`cancel-in-progress:`/comentário |
| Nenhum predicado de portão mexeu | confirmado: nada em `on:`, `runs-on:`, `steps:`, `if:` |
| Os 123 workflows parseiam | 0 falhas de YAML |
| Changelog + DCO | `changelog.d/1503-*.md` presente; `Signed-off-by:` presente |
| Efeito empírico | ver §7.4 |

### 7.1 R1 (SEV-3) — `${{ github.ref }}` colapsa três gatilhos na `main`

`backlog-verify` dispara em `pull_request`, `push`, `schedule` e `workflow_dispatch`.
O `github.ref` resolve assim:

| evento | `github.ref` | |
|---|---|---|
| `pull_request` | `refs/pull/<N>/merge` | único por PR ✓ |
| `push` (main) | `refs/heads/main` | ⎫ |
| `schedule` | `refs/heads/main` | ⎬ **mesmo grupo** |
| `workflow_dispatch` (main) | `refs/heads/main` | ⎭ |

Com `cancel-in-progress: true`, **um merge na `main` cancela o cron diário em voo**,
e vice-versa. O cron é a razão declarada de o `schedule` existir — o `CLAUDE.md`
registra que item de backlog fecha por trabalho em OUTRO repo, sem commit aqui. Se
ele é cancelado, a checagem diária de drift simplesmente não acontece, **em
silêncio**: exatamente a classe de falha que o #1503 conserta, em escala menor.

Não é regressão — antes disso, todos os gatilhos de todos os branches compartilhavam
um grupo só. Correção sugerida:

```yaml
group: "ci-backlog-verify-${{ github.event_name }}-${{ github.ref }}"
```

Push novo no mesmo PR continua cancelando o próprio run velho (mesmo `event_name`,
mesmo ref), e cron, push e dispatch param de se matar.

### 7.2 R2 (SEV-4) — mesma colisão em 5 noturnas, dentro dos 14

`byok_kill_switch_drill_weekly`, `byok_matrix_weekly`, `dr-drill-monthly`, `nightly` e
`perf-nightly` têm `schedule` + `workflow_dispatch`. Ambos resolvem para `refs/heads/main`,
então **um dispatch manual na `main` mata a noturna em voo**. Aposta menor que R1, mesmo
remédio (`github.event_name`).

Dos 14 escopados pelo #1503, **6 têm gatilho único** — `cas_foundation`, `coverage`,
`fabric-soak-proof`, `ffi-matrix-ci`, `fuzz-nightly`, `mutation-nightly` — e não colidem. Os
outros 2, `release-slsa3` e `sbom`, disparam em `release` + `workflow_dispatch`; `release`
resolve para `refs/tags/*`, que **não** colide com `refs/heads/main`, então também estão
fora. (A versão anterior dizia "os outros 8 têm gatilho único", o que era falso: são 6.)

**Duas ressalvas de escopo, para o número não ser lido como maior nem menor do que é.**

O `okf_nightly` **não** entra aqui, apesar de ser noturna com `schedule` + `dispatch`. O
grupo dele não colide por granularidade — ele **não varia por ref nenhuma**. É uma das 3
violações da §5, SEV-1, e listá-lo em R2 o rebaixaria a SEV-4.

E a classe é maior que os 14: **31 workflows na `main`** têm grupo com `github.ref`,
cancelamento ligado, e dois ou mais eventos que resolvem para `refs/heads/main`. R1 e R2
cobrem 6 deles porque foram os que o #1503 tocou. Os outros 25 já eram assim antes e
continuam — `rustfmt`, `docs-ci`, `e2e-prod`, `gitleaks`, `trivy`, entre outros. O item
[B-137] cobre os 5; a generalização repo-wide é escopo novo que este relatório **não**
resolve, só nomeia.

### 7.3 R3 (ferramental) — o scanner, que era o instrumento e virou o achado

Coberto na §5. **Subiu de SEV-2 para SEV-1** na revisão: o defeito não era só o verde falso
com comentário — era o scanner **passar sobre 3 violações vivas** por confundir interpolar
com variar. Consertado aqui, com três eixos de calibração novos (6, 7, 8 da §4.1).

Este é o achado que mais me custa registrar, e por isso vai explícito: **o instrumento que
media a campanha era ele próprio um caso da campanha.** Ele reportava `VIOLATIONS=0`
enquanto o `workspace-lint` perdia execuções de `push` na `main` pelo mesmo mecanismo que o
relatório descreve. Um instrumento calibrado em cinco eixos ainda mente nos eixos que
ninguém pensou em construir — e a defesa não é confiar mais no verde, é construir o eixo
que faltava assim que alguém o nomeia.

### 7.4 Divergência cosmética, sem defeito

`backlog-verify` usa `${{ github.ref }}`; os outros 13 usam
`${{ github.event.pull_request.number || github.ref }}`. **Funcionalmente equivalentes
aqui**: em `pull_request` o `github.ref` já é `refs/pull/<N>/merge`, único por PR; e
nos 13 restantes não existe gatilho `pull_request`, então a primeira metade da
expressão nunca resolve — é inerte. Nenhum furo. Fica o registro de que o único
arquivo onde `pull_request.number` teria função é justamente o que não a usa.

### 7.5 Q5 — `release-slsa3` sem o flag cria fila infinita?

**Não.** O GitHub mantém no máximo **um run rodando + um pendente** por grupo; um
terceiro run cancela o *pendente* (não o que está rodando). O teto é dois, não
cresce.

Consequência residual, herdada da semântica e idêntica à dos 5 irmãos: com três
releases em sequência rápida, o **do meio é descartado** enquanto pendente. É o
comportamento declarado como desejado no `50741f7c` — enfileirar em vez de matar em
voo — então é consistente, não defeito deste PR. Vale saber ao cortar releases em
rajada.

### 7.6 Efeito empírico pós-merge

**A tabela anterior estava contaminada e foi refeita.** Ela começava em `04:43:48Z`, mas o
#1503 mergeou em **`04:58:18Z`**: três das oito linhas eram **pré-merge**, quando a `main`
ainda tinha o grupo constante. Pior, a janela pegava a cauda da tempestade (03:56–04:33), de
modo que a quietação podia ser queda de tráfego, não o conserto. A conclusão continua a
mesma; **aquela evidência não a sustentava**.

Recortado em `created_at >= 2026-08-31T04:58:18Z`, N=13 concluídos:

```
05:01:01  success    push          main
05:01:56  success    pull_request  claude/backlog-wp-ci-findings
05:10:26  success    pull_request  claude/wp-a-b075-devenv
05:12:59  cancelled  pull_request  claude/wp-a-b080-scopes      <-- mesmo branch
05:13:12  success    pull_request  claude/wp-a-b080-scopes      <-- 13s depois
05:30:37  success    pull_request  claude/wp-a-b080-scopes
05:39:26  success    pull_request  claude/backlog-b136-b137-...
05:42:00  cancelled  pull_request  claude/backlog-owner-field-strict   <-- mesmo branch
05:42:29  success    pull_request  claude/backlog-owner-field-strict   <-- 29s depois
05:43:23  success    pull_request  claude/wp-a-b075-devenv
05:46:15  success    pull_request  claude/backlog-b138-runner-...
05:46:44  success    pull_request  claude/wp-c-b100
05:49:07  success    pull_request  claude/backlog-owner-field-strict
```

**Dos 13, exatamente 2 são cancelados, e ambos são supersessão do próprio branch** — push
novo matando o run velho do mesmo PR, 13s e 29s antes do sucesso que o substitui. É
precisamente para o que `cancel-in-progress` serve. **Zero cancelamentos entre branches
distintos**, com nove branches diferentes concorrendo e um `push` na `main` no meio.

Compare com a última linha da §2.3: `push main` às `04:24:41` matando `wp-a-b080-scopes` às
`04:24:42`.

A janela recortada é evidência **mais forte** que a original, não mais fraca: N maior,
tráfego maior, e nenhuma linha ambígua.

## 8. O que ainda está aberto

- [x] **R1** — B-136 acrescentou `github.event_name` ao `backlog-verify`; o verify do
      backlog confirma evento **e** ref no grupo atual.
- [x] **R2** — B-137 aplicou o mesmo reparo às cinco noturnas originalmente medidas.
- [ ] **As 3 violações vivas da §5** — `workspace-lint`, `runner-fleet-health`,
      `okf_nightly`. B-172 é o contrato que as rastreia; não confundir scanner que as
      revela com reparo dos workflows.
- [ ] **A população restante da colisão de ref** (§7.2), fora do recorte de B-136/B-137,
      continua em B-150. A contagem deve ser tomada do verify vivo do item, não desta
      medição histórica.
- [ ] **Gatear o scanner?** Decisão em aberto, e **não antes** de as 3 da §5 entrarem: hoje
      o portão nasceria vermelho, que é honesto mas não é mergeável. Um portão novo é custo
      novo, e o `runs-on:` teria de ser self-hosted, per mandato.
- [ ] **Branches vivos** — 77 de 146 carregavam o bloco velho. Herdam o conserto ao
      rebasear; branch de vida longa que não rebasear continua se cancelando.
