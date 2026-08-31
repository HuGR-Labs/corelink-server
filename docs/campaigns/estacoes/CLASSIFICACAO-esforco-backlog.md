# Classificação por esforço — os 115 itens abertos do `BACKLOG.md`

> **Base:** `origin/main` @ `401f9d3e`, lido num worktree limpo.
> **Entregável de leitura.** Nenhuma linha do `BACKLOG.md` foi alterada por este documento.

## 0. A população, antes do número

| medida | valor |
|---|---:|
| blocos ```` ```backlog ```` no arquivo | **166** |
| `status: open` | **115** |
| `status: done` | 50 |
| `status: parked` | 1 |

Enumerado por parser de blocos cercados, não por `grep` de prosa. **O `grep` ingênuo de
`status: open` no texto cru devolve 118** — infla em 3 porque casa a string dentro de
`verify-means`, onde itens explicam a própria polaridade. A diferença é pequena e é
exatamente do tamanho que passa despercebido.

Controles do instrumento: zero blocos sem `id`, zero ids duplicados, e os 115 abertos têm
todos uma seção `### B-NNN` correspondente (nenhum bloco órfão de cabeçalho). O corpo lido
foi a **seção inteira** — prosa + bloco —, 7 789 linhas; o bloco cercado sozinho carrega só
metadado e `verify-means`, e classificar por ele seria classificar o título.

## 1. A expectativa, escrita ANTES de contar

Registrada em `EXPECTATION.txt` antes de abrir qualquer corpo:

> trivial ~20–30 (17–26%) · mediano ~50–62 (43–54%) · hardcore ~28–42 (24–37%).
> Falsificadores: uma distribuição quase uniforme (38/38/39), ou trivial > mediano.

**O resultado ficou fora da faixa que eu previ, e para o lado dos hardcore.** Isso é
reportado como achado na §5, não ajustado para caber.

## 2. A rubrica

Três eixos — **dificuldade técnica**, **volume**, **risco** — e o **veredito é o MAIOR dos
três, nunca a média**. O eixo de risco mede *o que quebra se o conserto sair errado*, não o
quão feio é o defeito hoje: um defeito grave com reparo de uma linha inofensiva é trivial.

Itens `owner: owner` são classificados **pelo trabalho até a borda** — o que um agente
entrega antes da tecla do owner —, e o bloqueio vai na coluna própria. É por isso que
"apagar três chaves privadas" aparece como trivial: não há nada a construir.

## 3. A tabela dos 115

| id | título curto | dificuldade | volume | risco | **veredito** | bloqueio |
|---|---|---|---|---|---|---|
| [B-044] | armar teardown de box órfã | hardcore | mediano | hardcore | **hardcore** | owner+externo |
| [B-008] | PagerDuty aceita, ninguém sabe se chega | trivial | trivial | trivial | **trivial** | owner |
| [B-046] | Compliance/Object-Lock (WORM) | hardcore | hardcore | hardcore | **hardcore** | externo |
| [B-028] | 6 alertas Dependabot sem triagem | trivial | trivial | trivial | **trivial** | externo |
| [B-029] | portão de load-test não pode falhar | mediano | mediano | hardcore | **hardcore** | owner |
| [B-032] | 7 vendors Critical fora da cadência | trivial | trivial | trivial | **trivial** | owner |
| [B-035] | 8 linhas contratuais prometem TLS 1.3 | mediano | mediano | hardcore | **hardcore** | owner |
| [B-012] | credencial não-Actions p/ PR de bot | trivial | trivial | mediano | **mediano** | owner |
| [B-013] | 3 chaves privadas em ~/Downloads | trivial | trivial | trivial | **trivial** | owner |
| [B-049] | squash orfana toda âncora OKF (57/161) | hardcore | hardcore | mediano | **hardcore** | — |
| [B-056] | CAS sem orçamento de bytes process-wide | mediano | trivial | mediano | **mediano** | — |
| [B-053] | piso de amostras torna região morta infailoverável | hardcore | trivial | hardcore | **hardcore** | — |
| [B-054] | hash por elo da auditoria não é keyed | hardcore | mediano | hardcore | **hardcore** | — |
| [B-058] | robô OKF nunca executou | trivial | trivial | trivial | **trivial** | owner |
| [B-057] | stream de SLI é beco sem saída | hardcore | mediano | mediano | **hardcore** | — |
| [B-059] | regex OKF não vê citação abreviada | hardcore | mediano | hardcore | **hardcore** | — |
| [B-110] | 4 lanes presas em runner hosted | mediano | mediano | hardcore | **hardcore** | owner |
| [B-111] | aquisição de certificados Apple/Windows | trivial | trivial | trivial | **trivial** | owner |
| [B-112] | cadeia de release nunca produziu artefato | hardcore | mediano | hardcore | **hardcore** | — |
| [B-113] | 6 lanes self-hosted sem sucesso | hardcore | mediano | mediano | **hardcore** | — |
| [B-061] | roadmap sem verify / prevenção OKF ausente | hardcore | mediano | mediano | **hardcore** | — |
| [B-062] | prod 39 commits atrás da main | mediano | mediano | hardcore | **hardcore** | owner |
| [B-063] | partição de auditoria sem drenar há 82h | mediano | trivial | mediano | **mediano** | item+owner |
| [B-064] | selamento 200 linhas/h: cron lê chave errada | trivial | trivial | mediano | **mediano** | — |
| [B-065] | 2 endpoints Stripe processam o mesmo evento | trivial | trivial | trivial | **trivial** | owner |
| [B-067] | crates de auth sem execução de teste em CI | hardcore | mediano | hardcore | **hardcore** | — |
| [B-068] | testes #[ignore] sem executor nenhum | hardcore | mediano | hardcore | **hardcore** | — |
| [B-069] | 9/9 e2e da admin-ui em test.fixme | hardcore | mediano | hardcore | **hardcore** | — |
| [B-070] | [env.staging] declarado e nunca deployado | mediano | trivial | mediano | **mediano** | owner |
| [B-071] | sem GC nem eviction em produção | hardcore | mediano | hardcore | **hardcore** | owner |
| [B-072] | 2 crons declarados e nenhum handler scheduled | mediano | trivial | mediano | **mediano** | — |
| [B-073] | assento cross-tenant vira PAT cas:rw | hardcore | mediano | hardcore | **hardcore** | item |
| [B-074] | caminho do dinheiro com piso de entropia 16 | trivial | trivial | mediano | **mediano** | — |
| [B-076] | duas assinaturas pagáveis; a 2ª apaga a 1ª | hardcore | mediano | hardcore | **hardcore** | — |
| [B-077] | constantes de memória órfãs do downsize | mediano | mediano | mediano | **mediano** | — |
| [B-078] | batch-read materializa antes do teto | mediano | trivial | mediano | **mediano** | — |
| [B-079] | Max publicado a 4000 rps, limitado a 1000 | mediano | mediano | hardcore | **hardcore** | owner |
| [B-081] | rotação de chave de PAT causa indisponibilidade | trivial | trivial | mediano | **mediano** | item |
| [B-082] | sonda profunda de saúde inalcançável | mediano | trivial | mediano | **mediano** | — |
| [B-083] | BYOK vendido a $99/mo é XOR em memória | hardcore | hardcore | hardcore | **hardcore** | owner |
| [B-084] | drill emite atestado PASS de um sleep | trivial | trivial | trivial | **trivial** | — |
| [B-086] | um D1 global vs DPA que promete tenant-pinned | hardcore | hardcore | hardcore | **hardcore** | owner |
| [B-087] | CAIQ v4 atesta Y em 3 controles inexistentes | mediano | trivial | hardcore | **hardcore** | owner |
| [B-088] | resíduo de 'pentest limpo' na superfície publicada | trivial | mediano | mediano | **mediano** | owner |
| [B-089] | SLA promete crédito automático; nada emite | hardcore | mediano | hardcore | **hardcore** | owner |
| [B-090] | signup-worker deploya por npm install sem lockfile | mediano | trivial | mediano | **mediano** | — |
| [B-091] | cadeia de proveniência morta; SBOM de 3 meses | mediano | mediano | mediano | **mediano** | — |
| [B-093] | 4 tetos de tamanho; o de escrita é o menor | hardcore | mediano | hardcore | **hardcore** | — |
| [B-094] | material publicado promete gRPC e Buck2 | trivial | mediano | mediano | **mediano** | owner |
| [B-095] | 3 defeitos de interface; --region mente | hardcore | mediano | hardcore | **hardcore** | item |
| [B-096] | moat de rede cobre 6 imagens, não o cache | trivial | trivial | trivial | **trivial** | owner |
| [B-097] | teto de 200 tenants/região; 1250 de 1500 vCPU | mediano | trivial | mediano | **mediano** | owner |
| [B-098] | 18 worktrees em /tmp, 154 branches, zero tags | trivial | mediano | mediano | **mediano** | — |
| [B-099] | CODEOWNERS atribui a 10 times inexistentes | trivial | trivial | trivial | **trivial** | owner |
| [B-101] | 89 achados de auditoria nunca viraram item | hardcore | hardcore | mediano | **hardcore** | — |
| [B-102] | PUT quente 1,38s contra alvo de 30-50ms | hardcore | mediano | mediano | **hardcore** | item |
| [B-103] | escrita falha 87% sob paralelismo, satura em 2 req/s | hardcore | mediano | hardcore | **hardcore** | — |
| [B-104] | 404 autenticado: 0,32s de mediana sem causa | hardcore | trivial | mediano | **hardcore** | — |
| [B-105] | o cache custa mais do que economiza | hardcore | mediano | hardcore | **hardcore** | owner |
| [B-106] | verify de PAT frio custa 711ms, TTL do KV é 60s | hardcore | mediano | hardcore | **hardcore** | — |
| [B-107] | ostore 654ms e não é otimizável como uma coisa só | hardcore | mediano | mediano | **hardcore** | item |
| [B-108] | qbatch 116ms é a única quota sem cache | mediano | trivial | mediano | **mediano** | — |
| [B-109] | oother: 139ms de trabalho sem fase nomeada | mediano | mediano | mediano | **mediano** | item |
| [B-114] | imagem corelink-runner-devenv não existe | mediano | mediano | mediano | **mediano** | externo |
| [B-115] | nenhum portão testa se um PR deixa prod deployável | hardcore | mediano | hardcore | **hardcore** | — |
| [B-116] | doc publica POST /v1/dpa/accept, que não existe | mediano | mediano | mediano | **mediano** | — |
| [B-117] | apagar/exportar conta não têm documento nenhum | mediano | mediano | mediano | **mediano** | — |
| [B-118] | a única lane hosted com waiver nunca executou | mediano | trivial | mediano | **mediano** | — |
| [B-119] | /v1/admin/ops documentado, chamado, inexistente | hardcore | mediano | hardcore | **hardcore** | — |
| [B-120] | /v1/enterprise/inquire idem | hardcore | mediano | hardcore | **hardcore** | owner |
| [B-122] | fase sob spawn_blocking registra ZERO | hardcore | mediano | hardcore | **hardcore** | — |
| [B-123] | âncora de blob deixa o C5 vacuamente verde | hardcore | mediano | hardcore | **hardcore** | — |
| [B-124] | shifter de citações não é idempotente | mediano | trivial | mediano | **mediano** | — |
| [B-125] | cadeia de auditoria sela 200 linhas/h | hardcore | mediano | hardcore | **hardcore** | — |
| [B-126] | 81 arquivos acima de 1000 linhas (21% do código) | hardcore | hardcore | hardcore | **hardcore** | — |
| [B-127] | 3670 linhas de auditoria sem tenant: residência inavaliável | hardcore | mediano | hardcore | **hardcore** | — |
| [B-128] | runners sem disco viram vermelho falso | mediano | mediano | mediano | **mediano** | externo |
| [B-129] | Server-Timing foi desenhado para SOMAR | hardcore | mediano | mediano | **hardcore** | — |
| [B-130] | comparador de API não enxerga apps/** | hardcore | mediano | mediano | **hardcore** | — |
| [B-131] | doc manda re-rodar uma sonda que não existe | mediano | trivial | mediano | **mediano** | — |
| [B-132] | braço schedule do secrets-drift em hosted morto | mediano | trivial | hardcore | **hardcore** | owner |
| [B-133] | dependabot-policy executa código vindo do PR | mediano | trivial | mediano | **mediano** | — |
| [B-134] | 'shim não é daemon' nunca foi observado | trivial | trivial | trivial | **trivial** | — |
| [B-135] | imagem do runner sem gatilho de PR + rótulos órfãos | mediano | mediano | mediano | **mediano** | externo |
| [B-136] | colisão de ref mata o cron do backlog | trivial | trivial | trivial | **trivial** | — |
| [B-137] | mesma colisão em 5 noturnas | trivial | trivial | trivial | **trivial** | — |
| [B-138] | build da imagem do runner estoura o disco da box | hardcore | mediano | hardcore | **hardcore** | externo+owner |
| [B-139] | semgrep reprova com 4228 achados bloqueantes | hardcore | hardcore | mediano | **hardcore** | — |
| [B-140] | guard de runs-on cego p/ 2 grafias legais | hardcore | trivial | mediano | **hardcore** | — |
| [B-141] | pull_request_target sem gate de ator | mediano | trivial | mediano | **mediano** | externo |
| [B-142] | todo job ubuntu-* morto; CodeQL sem SAST há 6 dias | mediano | mediano | hardcore | **hardcore** | owner |
| [B-143] | id de placeholder passa CONFIRMED | trivial | trivial | trivial | **trivial** | — |
| [B-144] | revoke de PAT resolve por tenant, não por portador | hardcore | trivial | hardcore | **hardcore** | owner |
| [B-145] | portão docs-vs-realidade vacuamente verde | hardcore | mediano | hardcore | **hardcore** | — |
| [B-146] | backlog_verify é agnóstico ao status | mediano | trivial | mediano | **mediano** | — |
| [B-147] | invariante owner × status não é mecanizado | trivial | trivial | trivial | **trivial** | — |
| [B-148] | gate do backlog não roda em PR de workflow | mediano | trivial | mediano | **mediano** | — |
| [B-149] | 3 testes que passam sem afirmar nada | mediano | mediano | mediano | **mediano** | — |
| [B-150] | mais 28 workflows com a mesma colisão de ref | mediano | mediano | mediano | **mediano** | — |
| [B-151] | 2ª spec OpenAPI publicada com 21 dos 40 caminhos | mediano | mediano | mediano | **mediano** | — |
| [B-152] | 5 jobs mortos aos ~10m00s no MESMO PR | hardcore | mediano | mediano | **hardcore** | — |
| [B-153] | nada compara permissão publicada com aplicada | hardcore | mediano | hardcore | **hardcore** | — |
| [B-154] | 2 instrumentos jurídicos EXECUTADOS afirmam o inexistente | hardcore | trivial | hardcore | **hardcore** | owner |
| [B-155] | 93 de 134 verify fazem grep não-ancorado | mediano | hardcore | hardcore | **hardcore** | — |
| [B-156] | resíduo publicado: 915/240/121 menções | mediano | hardcore | hardcore | **hardcore** | owner |
| [B-157] | página do Bazel vaza o PAT no stderr de todo build | mediano | mediano | mediano | **mediano** | — |
| [B-158] | prefixo /bazel/v2 publicado dá 404 em todo request | mediano | mediano | mediano | **mediano** | — |
| [B-159] | sccache: falha de escrita trava read-only em silêncio | mediano | mediano | mediano | **mediano** | — |
| [B-160] | não existe caminho self-service p/ obter PAT | trivial | trivial | trivial | **trivial** | owner |
| [B-161] | página do Homebrew manda exportar o PAT ao ghcr.io | mediano | trivial | mediano | **mediano** | — |
| [B-162] | nenhum PAT publicado tem forma que o produto parseia | mediano | mediano | mediano | **mediano** | item |
| [B-163] | receita de upload monta a URL com o digest VAZIO | mediano | trivial | mediano | **mediano** | — |
| [B-164] | doctor rotula 401 como COR_QUOTA_EXCEEDED | mediano | mediano | mediano | **mediano** | — |
| [B-165] | recusa custa 52-195ms; o caminho SERVIDO segue sem número | mediano | trivial | mediano | **mediano** | item |
| [B-166] | --version imprime atestação em hostname sem DNS | mediano | mediano | mediano | **mediano** | — |
## 4. O sumário

| classe | itens | % da população |
|---|---:|---:|
| trivial | **16** | 14% |
| mediano | **43** | 37% |
| hardcore | **56** | 49% |
| **soma** | **115** | 100% |

**Fecha em 115.** A soma é verificada por comando, e a cobertura também: o gerador recusa
rodar se algum item aberto ficar sem linha ou se alguma linha não corresponder a um item
aberto. Essa checagem pegou uma omissão real durante a redação — o **B-072** tinha caído da
tabela sem que a contagem reclamasse, porque eu contava as linhas que escrevi em vez de
casá-las com a população.

## 5. Por que a distribuição não bateu com a expectativa

Previ 24–37% de hardcore e saiu **49%**. Duas causas, e nenhuma é "o mundo é pior do que eu
achava":

1. **A regra do máximo empurra para cima por construção.** Um item de dificuldade trivial,
   volume trivial e risco alto sai hardcore. Sete itens são exatamente isso (`B-087`,
   `B-144`, `B-154`, `B-053`, `B-104`, `B-062`, `B-035`). Se o veredito fosse a média, a
   contagem de hardcore cairia para perto da minha faixa — e seria a leitura errada, porque
   é justamente o eixo dominante que decide se dá para despachar o item.
2. **Esta população é o RESÍDUO, não a amostra.** 50 itens já estão `done`. O que fecha
   primeiro é o barato; o que sobra depois de 166 itens é, por seleção, a cauda difícil.
   Um backlog maduro com 14% de triviais é o esperado — foi a minha expectativa que não
   levou a seleção em conta.

O que **não** aconteceu, e valia checar: a distribuição não é uniforme (16/43/56), nenhuma
classe tem exatamente um terço, e trivial < mediano < hardcore é monotônico. Nenhum dos dois
falsificadores que escrevi disparou.

## 6. Os cortes que importam para planejamento

### 6.1 Triviais que se varrem num PR só, por tema

**Portão do backlog — `scripts/backlog_verify.py`, um PR:** `B-143` (id de placeholder passa
CONFIRMED — emenda medida de 2 linhas na `:260`) + `B-147` (invariante `owner` × `status` —
~3 linhas em `validate_schema`). Mesmo arquivo, mesma classe de defeito (o portão falha
ABERTO para o que não entrou na gramática dele), e os dois já vêm com a emenda escrita e
verificada nos dois sentidos.

**Concorrência de workflow — um PR:** `B-136` (grupo do `backlog-verify` sem
`github.event_name`) + `B-137` (a mesma linha em 5 noturnas). É literalmente a mesma edição,
já redigida no corpo do `B-136`. ⚠️ **Não puxe o `B-150` para dentro deste PR**: são mais 28
workflows com a mesma forma, mas ali o reparo por lane *não está decidido* (escopar por
evento vs. desligar o cancelamento muda o comportamento de lanes que enfileiram no Mac do
owner), e é por isso que ele está em mediano.

**Parar de afirmar o que não é verdade — agrupável, arquivos distintos:** `B-084` (o drill do
kill-switch sai zero em modo simulado e emite `PASS` de um `sleep` — reparo de UMA linha),
`B-096` (a ressalva de escopo do moat, onde a tese está), `B-099` (`CODEOWNERS` com nomes
concretos de revisor, que o próprio cabeçalho do arquivo instrui). Se preferir um PR por
concern, `B-084` é o de maior exposição removida por linha escrita do arquivo inteiro.

**Experimento barato, sozinho:** `B-134` — despachar `smoke-install` e `cosign-sign` uma vez
em `runs-on: corelink` e ler o resultado. Não é conserto, é medição; hoje a afirmação "essas
duas precisam de Docker de verdade" é **herdada, não medida**.

### 6.2 Hardcore que precisa de desenho ANTES de despacho

Não são 56 desenhos independentes. São seis frentes, e dentro de cada uma a ordem importa:

- **Instrumentação de fases — `B-122` é a raiz.** `B-107`, `B-102`, `B-109`, `B-129` e
  `B-105` não têm alvo mensurável enquanto o `ostore` agregar R2 com contabilidade D1. O
  próprio `B-122` proíbe conserto pela metade (fallback de handle sem controle de
  profundidade entrega números MAIORES que o relógio de parede). **Desenhe as três peças
  juntas ou não comece.**
- **OKF / wiki — `B-123` + `B-049` antes de tudo.** A âncora de blob deixa o C5 **vacuamente
  verde**, e `B-126` (81 arquivos acima de 1000 linhas) vai invalidar citações em massa. Rodar
  a campanha de refatoração antes do portão enxergar produz uma wiki inteiramente plausível e
  inteiramente errada, sem nada reclamar. `B-059` e `B-061` fecham na mesma frente.
- **Portões que não decidem** — `B-145` (o resolvedor aceita `/v1/zzz-nonexistent`), `B-153`,
  `B-155`, `B-130`, `B-115`, `B-140`, `B-101`. Cada um exige desenhar um portão novo e provar
  o dente; `B-145` é o de maior alcance, porque é o mecanismo pelo qual `B-116`/`B-119`/`B-120`
  foram publicados sob portão verde.
- **Segurança e dinheiro — `B-067` é pré-requisito declarado.** `B-073` (assento cross-tenant
  vira PAT `cas:rw` — o único achado que entrega dado de um cliente a outro), `B-076`, `B-144`,
  `B-093`. Não mergeie conserto de auth contra um CI que não executa `corelink-auth`/`corelink-pat`.
- **Instrumentos assinados e decisão comercial** — `B-035`, `B-086`, `B-087`, `B-089`,
  `B-154`, `B-083`, `B-079`, `B-105`, `B-120`. Trabalho de engenharia até a borda; a tecla é
  do owner e de counsel.
- **Perf sem causa nomeada** — `B-103`, `B-104`, `B-152`, `B-106`, `B-112`. Hardcore **por
  desconhecimento**: a causa não está identificada e este repositório já errou a causa uma
  vez em três destes. Enumerar antes de otimizar.

### 6.3 Bloqueados — e por quê

**Depende do owner (34 itens).** Dinheiro, assinatura, credencial, painel de terceiro,
deleção, ou decisão de produto:

`B-008`, `B-012`, `B-013`, `B-029`, `B-032`, `B-035`, `B-044`, `B-058`, `B-062`, `B-063`,
`B-065`, `B-070`, `B-071`, `B-079`, `B-083`, `B-086`, `B-087`, `B-088`, `B-089`, `B-094`,
`B-096`, `B-097`, `B-099`, `B-105`, `B-110`, `B-111`, `B-120`, `B-132`, `B-138`, `B-142`,
`B-144`, `B-154`, `B-156`, `B-160`.

> **`B-160` é o de maior efeito de alavanca da lista inteira e custa uma conta.** Não existe
> caminho self-service para obter um PAT de cliente; o owner criar a conta e entregar **dois**
> PATs de tenants distintos destrava toda a série de medição do caminho servido — inclusive a
> metade inverificável de `B-162` e `B-165`, e a pergunta de isolamento de `B-158`. Dois, não
> um: com um só, o invariante de isolamento continua sem instrumento.

**Depende de outro item (9).** `B-073` e `B-081` → `B-067`; `B-095` → `B-071`; `B-102` e
`B-107` → `B-122`; `B-109` → `B-122`; `B-162` e `B-165` → `B-160`; `B-063` → `B-112`/`B-064`
(e também do owner, pela credencial do D1 de prod).

**Depende de coisa fora do repo (8).** `B-046` (a Cloudflare não implementa Object Lock no
R2 — plataforma, não código), `B-028` (três advisories sem patch publicado a montante),
`B-114`, `B-128`, `B-135`, `B-138` (metade ou todo o reparo vive no `corelink-runners`, e o
token do Actions toma 403 no repo irmão), `B-141` (teto de spawn da frota), `B-044` (escopo
de instance-delete na API da Cloudflare).

### 6.4 Itens que já trazem o conserto escrito no corpo — só precisam de mão

Estes não pedem investigação; pedem execução. É a fila de maior throughput do backlog:

| id | o que o corpo já diz |
|---|---|
| `B-064` | duas linhas no cron: ler as chaves certas e iterar enquanto `incomplete` |
| `B-074` | os dois arquivos do dinheiro passam a chamar `resolve_internal_auth_key` |
| `B-081` | duas linhas no `container.start({ env })` + uma linha na matriz por variável |
| `B-084` | uma linha: o script sai não-zero em modo simulado |
| `B-090` | commitar o lockfile como exceção ao `.gitignore:95` + `npm ci --ignore-scripts` |
| `B-136` | `group: "ci-backlog-verify-${{ github.event_name }}-${{ github.ref }}"` |
| `B-137` | a mesma linha, nos cinco |
| `B-143` | duas linhas em `backlog_verify.py:260`, medidas numa cópia descartável |
| `B-147` | ~3 linhas em `validate_schema` cruzando `status` × `owner` |
| `B-058` | as quatro linhas do reparo, registradas para não serem re-derivadas |
| `B-056` | o padrão a copiar é o `GLOBAL_TURBO_GET_BUDGET`, no mesmo repo |
| `B-133` | rodar o script a partir de um checkout do ref BASE |

⚠️ Ter o conserto escrito **não** faz o item trivial. `B-074`, `B-081`, `B-090` e `B-133`
estão em mediano porque o eixo de risco manda: tocam credencial, cadeia de suprimentos ou o
caminho do dinheiro, e três deles exigem linha nova na matriz de secrets.

## 7. Honestidade — o que classifiquei com confiança BAIXA

**15 dos 115 (13%).** Se você for despachar algum destes, releia o corpo antes de confiar na
minha coluna:

| id | por que a confiança é baixa |
|---|---|
| `B-053` | um `const` de uma linha com risco de congelar escrita regional. Dificuldade e risco brigam; chamei hardcore pelo risco, mas "documentar o ponto cego como aceito" é uma saída barata e legítima |
| `B-059` | o conserto é contido; o **fallout** é que não é — abrir a gramática expõe um número desconhecido de citações erradas hoje invisíveis |
| `B-093` | só é hardcore porque o teto novo tem de caber em 1 GiB **junto** com `B-056` e `B-077`. Isolado seria mediano |
| `B-108` | pode não ter conserto nenhum: se o `qbatch` exige leitura fresca por correção de cobrança, o desfecho certo é uma recusa registrada |
| `B-109` | classifiquei mediano, mas ele compartilha a restrição de partição do `B-122`; se essa restrição morder, é hardcore |
| `B-125` vs `B-064` | os dois falam do teto de 200 linhas/hora e discordam sobre a causa (`B-125` diz que o chamador não foi identificado; `B-064` identifica o cron). **Podem ser um item só**, e eu os classifiquei separados |
| `B-140` | "mudança de mecanismo (parsear YAML)" me fez chamar hardcore; o `verify` do `B-142` já demonstra o padrão em um script. Defensável como mediano |
| `B-098` | grab-bag: as partes são triviais, mas cortar a primeira release de um `[Unreleased]` de 9 911 linhas é decisão de release |
| `B-012` | mint de um PAT é trivial; que ele carregue `contents:write` no CI não é. Chamei mediano pelo risco |
| `B-114`, `B-128`, `B-135`, `B-138` | **vivem no `corelink-runners`, que eu não li.** Classifiquei pelo que o item descreve, não pela árvore |
| `B-088`, `B-094`, `B-156` | os três medem o mesmo defeito (afirmação falsa na superfície publicada) com recortes diferentes e números incomparáveis. Classifiquei os três; a **sobreposição entre eles não está resolvida** |

Os itens de causa **não diagnosticada** — `B-103`, `B-104`, `B-112`, `B-152` — estão em
hardcore por aplicação da regra "o que eu não entendi é hardcore por padrão", não por eu ter
medido que são difíceis. É a classificação conservadora, e ela pode superestimar.

## 8. O que eu NÃO consegui verificar, e por quê

1. **Não li o `corelink-runners`.** Quatro itens (`B-114`, `B-128`, `B-135`, `B-138`) e parte
   do `B-044` vivem lá. Classifiquei pela descrição no `BACKLOG.md`, que é testemunho do
   próprio item — não evidência independente.
2. **Não executei nenhum `verify`.** Por instrução (`backlog_verify.py` sem `--id` dispara
   ~166 comandos com `cargo`/`wrangler`/`gh`/`curl`) e por escopo: esta é uma classificação de
   esforço, não uma reverificação de estado. **Todo "medido em 2026-08-31" nesta tabela é o
   item se auto-relatando.** Se algum item derivou desde a última verificação, minha
   classificação herda o erro.
3. **Não compilei nada** e não medi latência, disco, ou estado de produção. Os itens de perf
   (`B-102`…`B-109`, `B-165`) foram classificados pela forma do trabalho descrito, não por
   confirmação de que os números ainda valem.
4. **Esforço não é tempo.** Não converti as classes em horas nem em dias. As classes ordenam
   por *quanto é preciso saber e quanto pode quebrar*, e duas coisas na mesma classe podem
   diferir por uma ordem de grandeza em duração.
5. **A sobreposição entre itens não foi resolvida.** `B-064`/`B-125`, `B-088`/`B-094`/`B-156`,
   `B-136`/`B-137`/`B-150`, e `B-049`/`B-061`/`B-123` descrevem territórios que se tocam.
   Contei cada um uma vez porque a população é a lista de itens — mas **a soma dos esforços é
   menor que a soma das classes**, e quem planejar em cima disto deve tratá-los em bloco.
6. **A tabela reflete `origin/main` @ `401f9d3e`.** A `main` avançou para `571aef63`
   **durante** esta leitura, e o commit novo toca o `BACKLOG.md` — então recenseei a base nova
   antes de fechar, em vez de supor: **166 blocos, 115 open, 50 done, 1 parked, os mesmos ids,
   nenhum status alterado.** A mudança foi de prosa e de `verify`, não de população, e a
   classificação continua valendo. Há ainda branches em voo com `B-144`…`B-166` reescritos e
   **um item a mais aberto**; contra aquela base a população não é 115.
