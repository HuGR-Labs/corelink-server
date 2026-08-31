# RODADA 2 — as lentes que faltavam, medidas com DOIS tenants

**Data:** 2026-08-31 · **Sessão:** `estacoes-dois-tenants` · **Personas:** ACME + RIVAL
(provisionamento e ressalvas em `PROVISIONAMENTO-tenants-de-teste.md`)
**Medido de:** Mac do owner → edge Cloudflare **GRU** (rede residencial)
**Contêiner, por `GET` por `{id}` na Containers API — nunca pela LISTA, que é defasada:**

```
GET /accounts/$ACC/containers/applications/a033572c-0803-4866-b3a3-61f4812843b1
→ corelink-prod-corelinkserver-prod      version=178  image=…:ddd95560-r1  instances=20 (15 healthy, 5 active)
GET /accounts/$ACC/containers/applications/a0337243-13cb-46ef-adb1-781294294404
→ corelink-prod-sam-corelinkserver-prod-sam version=143 image=…:ddd95560-r1 instances=16 (15 healthy, 1 active)
```

**Mesma imagem (`ddd95560-r1`) da rodada 1** — os números das duas rodadas são
comparáveis; a diferença entre elas é a credencial, não o build.

---

## 0. A RESPOSTA À PERGUNTA QUE ESTAVA ABERTA

> A-1 mediu que o Bazel real emite `/bazel/v2/{ac,cas}/<sha256>` — **sem segmento de
> tenant**. Uma leitura do código dizia que o tenant vem de header injetado pelo
> Worker, fail-closed. **Prove com os dois PATs, não com leitura.**

**Provado. A hipótese estava certa, e o isolamento se sustenta.**

Predição escrita **antes** de medir: se o tenant vem do PAT, o RIVAL pedindo o
**mesmo hash** no **mesmo caminho sem tenant** recebe 404; se não vem, recebe os bytes.

```
D0  RIVAL GET /bazel/cache/cas/<sha>        → 404   (controle negativo: vazio p/ ambos)
D1  ACME  PUT /bazel/cache/cas/<sha>        → 204   (2528 ms)
D2  ACME  GET /bazel/cache/cas/<sha>        → 200, 29 bytes
    corpo: "ACME-BAZEL-SECRET-2026-08-31"          ← CONTROLE POSITIVO: a escrita pegou
D3  RIVAL GET /bazel/cache/cas/<sha>        → 404, "not found"
D4  sem auth                                → 401
```

**O controle positivo (D2) é o que dá valor ao D3.** Sem ele, um PUT que
silenciosamente não gravasse produziria exatamente o mesmo 404 e eu declararia
isolamento sem ter provado nada.

**I-1 sustentado no Bazel.** O tenant é resolvido a partir do PAT no edge; o caminho
tenantless não é uma superfície compartilhada.

### E o caminho que o Bazel real emite não é o que a página documenta

Enumerei a **população de rotas Bazel** montadas (`bazel_v2.rs:305-343`) — **6**:

```
/bazel/v2/{instance}/blobs/{hash}/{size}
/bazel/v2/{instance}/blobs/ac/{hash}/{size}
/bazel/v2/{instance}/uploads/{uuid}/blobs/{hash}/{size}
/bazel/v2/{instance}/findMissingBlobs
/bazel/cache/cas/{hash}          ← o que o binário real emite
/bazel/cache/ac/{hash}
```

**Zero rotas na forma `/bazel/v2/{ac,cas}/<hash>`.** Medido em produção:

| caminho | resposta | leitura |
|---|---|---|
| `/bazel/v2/cas/<sha>` (o que o Bazel emite com `--remote_cache=…/bazel/v2`) | **403 `tenant mismatch`** | o segmento após `v2` é lido como **tenant**; `"cas"` ≠ tenant do PAT |
| `/bazel/v2/<tenant>/cas/<sha>` | 404 | rota não existe nessa forma |
| `/bazel/v2/<tenant>/blobs/<sha>/<size>` | 404 (miss, montada) | forma REAPI, correta |
| `/bazel/cache/cas/<sha>` | **204 / 200** | ✅ a forma que funciona |

→ **Um cliente Bazel apontado para `…/bazel/v2` recebe 403 em toda requisição**, e a
mensagem `tenant mismatch` manda o cliente investigar tenancy quando o problema é a
URL base. Corrobora o achado #3/#4 já registrado em A-1, agora **do lado do servidor**:
o `examples/bazel-starter/.bazelrc:22` (que aponta para `/bazel/cache`) está certo e a
**página publicada está errada**.

---

## 1. C-1/C-2/C-3 — POPULAÇÃO 8, TODO ELEMENTO COM VEREDITO, A SOMA FECHA

| # | superfície | Funciona (artefato) | Isolamento | veredito rodada 2 |
|---|---|---|---|---|
| A-1 | Bazel `/bazel/cache/*` | **SIM** — 29 B byte-idênticos | RIVAL 404 | **funciona** (doc segue falhando) |
| A-2 | Turborepo `/v8/artifacts/*` | **SIM** — 37 B byte-idênticos | RIVAL 404 (próprio e alheio `teamId`) | **funciona** |
| A-3 | sccache `/cargo/<t>/*` | **SIM** — 37 B byte-idênticos | RIVAL 404 | **funciona** |
| A-4 | OCI `/v2/*` | **SIM** — token emitido, `/v2/` 200 | token amarra o tenant do PAT | **funciona** (via troca de token) |
| A-5 | Homebrew `/brew/<t>/*` | **não exercido** — `PUT` 405 (espelho read-only); forma de caminho recusada 403 | — | **não aplicável a escrita** |
| A-6 | npm `/npm/<t>/*` | **SIM** — `left-pad-1.3.0.tgz`, **sha1 idêntico ao upstream** | MALICE 403 | **funciona** |
| A-7 | pip `/pip/<t>/simple/*` | **SIM** — índice do `six`, 14 807 B | MALICE 403 | **funciona** |
| A-8 | CAS/AC `/v1/cas/<t>/*` | **SIM** — 47 B byte-idênticos | RIVAL 404 · MALICE **403 `cross-tenant`** | **funciona** |

**7 exercidas + 1 não-aplicável-a-escrita = 8.** Fecha.

> ⚠️ **"funciona" aqui é a lente FUNCIONA do protocolo, não veredito de estação.**
> Os vereditos `FALHOU` da rodada 1 (documentação publicada que quebra o cliente)
> **continuam de pé** — eles não eram sobre credencial. Ver §5.

---

## 2. SEGURO — o ataque com o segundo PAT

Toda tentativa abaixo usou uma **credencial válida** do RIVAL contra dado do ACME.

| # | ataque | resultado | veredito |
|---|---|---|---|
| S-1 | RIVAL lê `/v1/cas/<ACME>/<digest>` | **403 `cross-tenant`** em 195 ms | recusado |
| S-2 | RIVAL lê `/bazel/cache/cas/<sha>` do ACME (caminho **sem** tenant) | **404** | recusado |
| S-3 | RIVAL lê `/v8/artifacts/<h>?teamId=<ACME>` | **404** | recusado |
| S-4 | RIVAL lê `/npm/<ACME>/left-pad/-/…tgz` | **403 `tenant mismatch`** | recusado |
| S-5 | RIVAL lê `/pip/<ACME>/simple/six/` | **403 `tenant mismatch`** | recusado |
| S-6 | RIVAL pede token OCI com `scope=repository:<ACME>/app:pull` | 200, mas o token traz **o tenant do RIVAL** | recusado |
| S-7 | Path traversal `/v1/cas/<RIVAL>/../<ACME>/<digest>` | **403** | recusado |
| S-8 | Tenant adivinhado `/v1/cas/00000000-…-000000000000/<digest>` | **403** | recusado |
| S-9 | PAT `read-only` escrevendo (`/v1/cas` e `/bazel/cache`) | **403 `insufficient scope`** (2/2) | recusado |
| S-10 | PAT forjado (96 chars, forma canônica, assinatura trocada) | **401** | recusado |
| S-11 | PAT revogado | **401** em ≤ 10 s | recusado |

**Resultado: 11/11 recusados. Zero vazamento. I-1 sustentado nas 8 superfícies.**

### O controle que impede um falso verde no S-9

Um 403 no `PUT` do PAT `read-only` seria indistinguível de um PAT quebrado. Os dois
controles ao lado:

```
PAT read-only do RIVAL — PUT  /v1/cas/<RIVAL>/<digest>   → 403 insufficient scope
PAT read-write do RIVAL — PUT mesmo caminho              → 201            ← o caminho funciona
PAT read-only do RIVAL — GET  mesmo caminho              → 200, 34 bytes  ← o PAT funciona
```

→ O 403 é **escopo**, comprovadamente. (A primeira leitura deu 404 porque nada havia
sido escrito ainda — **a ordem de medição fazia parte do resultado**, e refazer depois
da escrita é o que transformou o 404 ambíguo em 200 conclusivo.)

### Revogação — ≤ 10 s, com a população dita

```
pré-revogação                                   → 200
UPDATE pat SET revoked_at_ms=<t0> …             → changes=1
t+10 s                                          → 401   auth;dur=6;desc="kv"
```

**≤ 10 s, n=1.** A primeira amostra já estava 401; **não bisectei o intervalo (0, 10]**,
então o valor honesto é um **teto**, não uma medida. Vale notar que o `desc="kv"` mostra
que a revogação atravessou o cache KV de auth — era exatamente aí que ela poderia ficar
presa, e não ficou.

---

## 3. RÁPIDO — o número, e a correção de um achado anterior da campanha

### ⚠️ Correção: `Server-Timing` NÃO está ausente em 5 de 6 superfícies

A campanha registrou "`Server-Timing` ausente em 5 de 6 superfícies". **Medi o
contrário: presente em 6 de 6.** Duas medidas do mesmo objeto discordando é sinal
(Q-3), então testei a hipótese em vez de escolher a mais nova:

```
GET /v1/cas/<t>/<digest>  SEM auth        → 401 · Server-Timing: 0 ocorrências
GET /v1/cas/<t>/<digest>  PAT forjado     → 401 · Server-Timing: 0 ocorrências
GET /v1/cas/<t>/<digest>  PAT válido      → 200 · Server-Timing: 1 ocorrência
```

→ **O header só é emitido em requisição autenticada.** A rodada 1 só podia sondar
sem credencial, então mediu a **ausência do observador, não a ausência do
instrumento**. O instrumento sempre esteve lá.

**Consequência:** a lente RÁPIDO das 8 estações não é "sem instrumento" — é
mensurável, e está medida abaixo. E fica registrado que **um gate/probe não
autenticado não enxerga o que o cliente autenticado enxerga.**

### Os números (server-side `total;dur`, alvo **15-30 ms**)

`total;dur` é medido **pelo Worker**, então não carrega a minha rede residencial —
é o número comparável ao alvo. O `wall` do `curl` fica 50-150 ms acima e não é.

| superfície | estado | n | `total;dur` (ms) | mediana | vs alvo 30 ms |
|---|---|---|---|---|---|
| `/v1/cas` GET | quente | 6 | 426 · 455 · 472 · 513 · 545 · 579 | **~493** | **~16x fora** |
| `/bazel/cache/cas` GET | quente | 6 | 366 · 393 · 466 · 528 · 540 · 630 | **~497** | **~17x fora** |
| `/v8/artifacts` GET | quente | 1 | 339 | 339 | ~11x fora |
| `/cargo/<t>` GET | quente | 1 | 683 | 683 | ~23x fora |
| `/npm/<t>` tarball | quente | 3 | 596 · 665 · 1213 | 665 | **~22x fora** |
| `/pip/<t>/simple` | quente | 3 | 263 · 288 · 304 | 288 | ~10x fora |
| `/bazel/cache/cas` PUT | **frio** | 1 | 2463 | — | ~82x fora |
| `/v1/cas` PUT | **frio** | 1 | ~4051 (wall) | — | — |

**Nenhuma superfície chega perto do alvo. A mais rápida (pip, 263 ms) está ~9x fora.**

### Onde o tempo vai — o header decomposto

O PUT frio do Bazel, inteiro:

```
auth;dur=263;desc="d1"  wdb;dur=696  qtier;dur=266  qbatch;dur=123  qresid;dur=126
origin;dur=1504 ( ohop;dur=197  opat;dur=84  ostore;dur=822  oaudit;dur=213  oother;dur=188 )
total;dur=2463
```

O GET quente do mesmo caminho:

```
auth;dur=12;desc="kv"  wdb;dur=138  qtier;dur=11  qresid;dur=4
origin;dur=719 ( ohop;dur=167  ostore;dur=331  oaudit;dur=97  oother;dur=124 )
total;dur=869
```

Três fatos que os números dizem e a suíte não diria:

1. **`ohop` é um piso de ~150-183 ms, constante em TODAS as superfícies e em todos os
   estados** (frio, quente, hit, miss, 404). É o salto Worker→contêiner. Sozinho ele
   já é **5-6x o alvo inteiro** — nenhuma otimização de cache o alcança.
2. **`auth` tem dois regimes visíveis**, e o header nomeia qual: `desc="d1"` (263 ms,
   frio) → `desc="kv"` (6-14 ms) → `desc="l1"` (0 ms). O caminho de auth **já está
   resolvido**; não é ele o problema no estado quente.
3. **`oaudit` custa 92-213 ms em toda operação** — e §4 mostra que esse custo produz
   uma linha que **não chega à trilha visível ao cliente**.

**Frio vs quente, dito explicitamente (Q-4):** o PUT frio (2463 ms) e o GET quente
(~500 ms) **não são dois caminhos de código diferentes** — são miss e hit da mesma
função, mais o Argon2id/D1 do primeiro auth. Ler os dois como "PUT é 5x mais lento que
GET" seria repetir o erro que esta campanha já cometeu duas vezes.

**Variância existe** (366-630 ms em n=6 no mesmo caminho) — o instrumento está medindo
mundo, não a si mesmo.

---

## 4. REGISTRADO — fui ler o log DEPOIS, e ele conta duas histórias

Consultado em `corelink-config-prod` **após** todas as ações.

### O que está certo

**48 eventos** para os dois tenants, com o tenant correto em cada um:

| tenant | evento | n |
|---|---|---|
| ACME | `corelink.cas.read.attempted` | 16 |
| ACME | `corelink.cas.read.served` | 14 |
| ACME | `corelink.cas.write.attempted` | 4 |
| ACME | `corelink.cas.write.committed` | 3 |
| ACME | `corelink.cas.correctness.violation` | 1 |
| RIVAL | `read.attempted` / `read.served` / `write.attempted` / `write.committed` | 6 / 2 / 1 / 1 |

- **Par attempted→committed fecha:** ACME teve 4 tentativas de escrita e 3 commits. A
  quarta é o `PUT` com digest errado, e ela aparece nomeada como
  `corelink.cas.correctness.violation`. **A recusa 422 é observável na trilha** (I-10 ✅
  para esse caso).
- **I-4 (todo uso cobrável é contado) — SATISFEITO, e bate exatamente.** `usage_daily`:

  ```
  ACME  reads=16  writes=3  hits=14  misses=2
  RIVAL reads=6   writes=1  hits=2   misses=4
  ```

  Os mesmos 16/3 e 6/1 da trilha. E `writes` conta **committed**, não attempted — a
  escrita recusada por hash **não é cobrada**.
- **I-6 (segredo nunca sai) — SATISFEITO.** Busca por `corelink_pat` / `Bearer` /
  `argon2` nos 48 payloads: **0 ocorrências**, com **controle positivo na mesma
  consulta** (48 ocorrências de `digest`). O zero é do mundo, não do instrumento.

### 🔴 O que está errado — três achados

**4.1 — A trilha visível ao cliente não recebeu NADA. (SEV-0 de dreno, confirmado vivo.)**

```
audit_outbox           = 82 622 linhas   (48 delas minhas, desta rodada)
customer_audit_events  =      71 linhas   (0 delas minhas)
```

A trilha selada e encadeada tem **0,086%** do outbox. **Nenhum** dos meus 48 eventos
chegou nela. O custo `oaudit` de 92-213 ms é pago em toda requisição para produzir
uma linha que o cliente **não pode ler**.

→ **I-5 (toda mutação é auditável, encadeada) FALHA na camada que importa.** O evento
existe numa fila interna; a trilha do cliente não sabe que aconteceu.

**4.2 — Nenhuma recusa de AUTORIZAÇÃO aparece em lugar nenhum da trilha.**

Enumerei a **população de `event_type` em toda a produção: 27 tipos.** Nenhum é de
negação de autorização. Os únicos parentes são
`corelink.cas.correctness.violation` (o 422) e `corelink.signup.pilot_token_rejected.v1`.

Disparei **11 ataques com credencial válida** (§2) — cross-tenant, traversal, escopo,
tenant adivinhado. **Zero linhas de auditoria.**

→ **I-10 (recusa é observável) VIOLADO para a classe que mais importa.** Um tenant
sondando dados de outro é recusado corretamente **e é invisível**. Não há como detectar
o ataque depois. O controle que prova que não é cegueira do instrumento: a trilha
**consegue** registrar recusa — ela registrou o 422 na mesma janela.

**4.3 — Atribuição de principal é inconsistente entre superfícies.**

```
H5TgaU                                     read.attempted 9 · read.served 7 · write.* 1+1
iq9gHE                                     read.attempted 2
anon@97f9827a-e185-4f23-99d4-5989dadeee73  read.attempted 7 · read.served 7 · write.* 3+2 · violation 1
anon@7080e06d-e948-4864-a7cb-19b30e7fab3f  read.attempted 4 · read.served 2 · write.* 1+1
```

As superfícies de adaptador (Bazel/npm/pip/cargo) gravam o **`token_prefix`** — dá para
dizer **qual credencial** agiu. A superfície nativa `/v1/cas` grava
**`anon@<tenant-id>`** — a credencial some, e toda ação de todo token do tenant vira o
mesmo principal.

→ Numa investigação ("qual token vazado escreveu isto?"), o plano nativo **não
responde**. Não é vazamento, é perda de atribuição.

**4.4 — 🟠 Retry do MESMO upload é cobrado de novo. (I-8)**

Eu tinha escrito na tabela de invariantes que I-8 estava bem, por analogia com o
conteúdo endereçado. **Fui medir e estava errado** — registro a correção porque um
item que muda de causa em silêncio é indistinguível de um que sempre esteve certo.

Três `PUT` **idênticos** (mesmos bytes, mesmo digest, mesmo tenant):

```
usage_daily.writes ANTES : 4
  PUT #1 → 201 (created)
  PUT #2 → 200 (já existe)
  PUT #3 → 200 (já existe)
usage_daily.writes DEPOIS: 7          ← +3
audit_outbox para este digest: write.attempted 3 · write.committed 3
```

- **O efeito não duplica** — 201 na primeira, 200 nas seguintes; o conteúdo endereçado
  faz o trabalho certo, e o status code até distingue os dois casos.
- **A cobrança duplica.** As duas escritas que **não armazenaram byte nenhum** foram
  contadas como escrita, e `writes` é dimensão cobrável.

O caso real não é um laço malicioso: é o **retry depois de um timeout de rede**. O
cliente que não recebeu a resposta reenvia — e paga duas vezes por um byte que já
estava lá. Com um PUT frio custando 2,5 s (§3), o timeout não é hipotético.

→ **I-8 violado no eixo de cobrança**, sustentado no eixo de efeito.

---

## 5. O QUE ESTA RODADA **NÃO** MUDA

A credencial destravou as lentes. Ela **não** conserta o que a rodada 1 achou, e
nenhum veredito de estação vira "validado" por causa dela:

- A página do Bazel continua publicando um `.bazelrc` que **não roda** e que **imprime
  o PAT no stderr** — agora com corroboração do servidor (§0: `…/bazel/v2` dá 403).
- A página do Homebrew continua mandando o cliente **exfiltrar o PAT para o `ghcr.io`**.
- `corelink get -o` continua em **panic** 3/3; `corelink doctor` continua sondando um
  host sem DNS e rotulando 401 como três erros falsos.
- Os exemplos de PAT publicados (26/37/53 chars) continuam **não parseando** — e esta
  rodada **confirmou a forma canônica** (96 chars, 2 pontos) nos três tokens.
- **B-1 (funil de cadastro) segue NÃO VALIDADA**, por construção. Ver
  `PROVISIONAMENTO-tenants-de-teste.md` §0.

**Nenhum conserto de produto foi feito para uma estação passar.** Achado é entrega.

---

## 6. INVARIANTES — veredito contra esta rodada (10/10 nomeados)

| # | veredito | prova mais curta |
|---|---|---|
| **I-1** isolamento | **SUSTENTADO** | 11/11 ataques recusados, 8 superfícies, com controle positivo em cada uma |
| **I-2** fail-closed | **SUSTENTADO** | forjado/revogado/sem-auth → 401; escopo → 403; hash errado → 422; nenhum produziu acesso |
| **I-3** auth antes de caro | **SUSTENTADO** | `Server-Timing` ausente no 401 ⇒ o pipeline instrumentado nem começa sem credencial |
| **I-4** uso contado | **SUSTENTADO** | `usage_daily` bate exatamente com a trilha; escrita recusada não é cobrada |
| **I-5** mutação auditável | **FALHA** | 48 eventos no outbox, **0** na trilha do cliente (§4.1) |
| **I-6** segredo nunca sai | **SUSTENTADO** | 0 hits de `corelink_pat`/`Bearer`/`argon2` com controle positivo de 48 |
| **I-7** integridade | **SUSTENTADO** | 5 superfícies byte-idênticas; `left-pad` com **sha1 igual ao upstream**; hash errado → 422 |
| **I-8** idempotência | **VIOLADO na cobrança** | efeito não duplica (201→200→200), mas `usage_daily.writes` sobe **+3 em 3 PUTs idênticos** (§4.4) |
| **I-9** apagamento | **não exercido** | fora do escopo desta rodada; tenants deixados de pé para ela |
| **I-10** recusa observável | **VIOLADO** | 11 recusas de autorização, **0** linhas de auditoria (§4.2) |

---

## PAREI EM

**Nada me bloqueou nesta rodada.** As três coisas fora do escopo, ditas por nome:

1. **A-5 Homebrew não foi exercida como escrita** — `PUT` devolve 405 (é espelho
   read-only) e a forma de caminho que tentei foi recusada com 403 `forbidden repo
   path`. **A forma correta de caminho de bottle não foi determinada**; não afirmo
   nada sobre a superfície além disso.
2. **I-9 (apagamento)** — os dois tenants ficaram de pé exatamente para essa estação.
3. **A revogação é um TETO de 10 s, não uma medida** — n=1, sem bisseção.

**Cold review pendente:** quem executou esta rodada não a revisa (§6 do GOAL). Os três
alvos mais frágeis para tentar refutar: (a) o 404 do RIVAL no Bazel — atacar o controle
positivo; (b) a leitura de que `Server-Timing` depende de auth — testar uma superfície
que eu não testei; (c) a contagem de 0 recusas na trilha — procurar uma tabela de
auditoria que eu não enumerei.
