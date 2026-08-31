# ESTAÇÃO A-4 — OCI registry (Docker / Podman)

**Página que promete:** `apps/docs/docs/integrations/oci-registry.md`
**Publicada em:** `https://humangr.com/corelink/docs/integrations/oci-registry` (200)
**Executado em:** 2026-08-31, do Mac do owner (edge Cloudflare **GRU**).
**Contêiner medido por `GET` por `{id}`** (nunca pela LISTA):
`corelink-prod-sam-corelinkserver-prod-sam` → **version=143**, image tag **`ddd95560-r1`**;
`corelink-prod-corelinkserver-prod` → **version=178**, mesma tag `ddd95560-r1`.

```
GET https://api.cloudflare.com/client/v4/accounts/$ACC/containers/applications/a0337243-13cb-46ef-adb1-781294294404
→ version=143 image=...corelink-prod-sam-corelinkserver-prod:ddd95560-r1 instances=15
```

---

## VEREDITO: **BLOQUEADO** (lente 1-3) + **2 achados** na lente 4/documentação

Não é `validado` e não é `falhou`: o caminho feliz **não pôde ser executado** por falta
de credencial de cliente (ver `BLOQUEIO-credencial-de-cliente.md`). O que foi executável
sem credencial foi executado e está abaixo.

---

## ESCRITO ANTES DE MEDIR — a expectativa

1. `docker login` com PAT inválido → **401**, sem oráculo (mesma mensagem para PAT
   malformado e PAT desconhecido).
2. `GET /v2/` sem auth → **401** com `WWW-Authenticate: Bearer realm=...` apontando para
   um endpoint de token do próprio produto.
3. `/v2/_catalog` → recusado (a página promete "disabled by default").
4. Manifest sem auth → **401 antes de qualquer trabalho** (I-3).

Todas as quatro se confirmaram. Nenhuma surpresa de segurança.

---

## FUNCIONA

### `docker login` — o comando da página, verbatim

```bash
export DOCKER_CONFIG=/tmp/clk-docker-a4   # isolado, para não tocar o config do owner
echo "corelink_pat_AAAAAAAAAAAAAAAAAAAAAAAA" | docker login corelink-api.humangr.com \
  --username corelink --password-stdin
```

```
Get "https://corelink-api.humangr.com/v2/": unauthorized: authentication failed
  (ref: f3a96a96d7c14995bf625657ffa8eb81)
login rc=1
```

Docker 28.3.2. **Recusa correta.** Sem PAT válido não há como avançar.

### `docker pull` — NÃO EXECUTADO

```
docker pull corelink-api.humangr.com/my-app:latest
Cannot connect to the Docker daemon at unix:///var/run/docker.sock.
```

O daemon não está de pé neste Mac. **Não subi o Docker Desktop de propósito** — este Mac
é o runner do CI (invariante da casa) e, sem PAT válido, o `pull` terminaria no mesmo 401
que o `login` já entregou. **A camada não foi trazida; o artefato não está na mão.**

### A linha mais curta que decide (o fluxo de dois pernas que a página promete)

```bash
curl -s -D- -o /dev/null https://corelink-api.humangr.com/v2/ | grep -i www-authenticate
```
```
www-authenticate: Bearer realm="https://corelink-oci.humangr.com/token",service="corelink-oci",scope="repository:*:pull"
```

O desafio existe e é bem-formado. As duas pernas:

```bash
curl -u "corelink:corelink_pat_AAAA..." \
  "https://corelink-oci.humangr.com/token?service=corelink-oci&scope=repository:my-app:pull"
→ http=401 {"errors":[{"code":"UNAUTHORIZED","message":"authentication failed (ref: 72b57d…)"}]}

# sem credencial nenhuma
→ http=401 (idêntico)
```

**Artefato entregue ao usuário:** nenhum. Um 401 correto, e nada mais.

---

## RÁPIDO

| medido de | estado | operação | número |
|---|---|---|---|
| Mac → edge GRU | frio (1ª) | `GET /v2/` (401) | **193 ms** |
| Mac → edge GRU | quente (5 seguintes) | `GET /v2/` (401) | 203, 193, 192, 194, 201 ms |
| Mac → edge GRU | frio/quente | `GET /v1/users/me` (401) | 52 / 53–55 ms |

**Alvo do GOAL: 15–30 ms (50 ms cross-region).**
**Veredito: FORA — o 401 do `/v2/` custa ~195 ms, ~6,5× o teto de 30 ms e ~3,9× o teto
cross-region de 50 ms.** É o caminho **mais lento** das seis superfícies medidas, e é um
caminho que **não faz trabalho nenhum** (recusa de auth). O `/v1/users/me`, que recusa a
mesma coisa, custa 52 ms — ou seja, ~140 ms são específicos da rota OCI.

Frio e quente **não se distinguem** (193 vs 192–203 ms): não há aquecimento, o custo é
estrutural do caminho, não de cache.

### `Server-Timing` — ACHADO

`/v2/` é a **única** das seis superfícies que emite `Server-Timing`, e o que emite é:

```
$ curl -s -D- -o /dev/null https://corelink-api.humangr.com/v2/ | grep -i server-timing | od -c
0000000   s e r v e r - t i m i n g :   o o t h e r ; d u r = 0 \r \n
```

Byte-exato, 3 amostras idênticas: **`oother`** — nome de métrica com typo, e `dur=0` num
caminho que leva 195 ms. O header não decompõe nada. **O GOAL §3.2 exige colar o
`Server-Timing` decomposto; nesta superfície ele não existe de forma utilizável.**

---

## REGISTRADO

**Não verificável nesta rodada.** Ler a trilha de auditoria de um tenant exige ser aquele
tenant (`corelink audit export`) ou credencial de operador — a segunda é proibida pela
regra anti-trapaça §5. As ações que executei foram todas recusas de auth sem tenant
resolvido, então nem haveria tenant a que atribuí-las.

O que **é** observável: cada 401 traz um identificador de correlação
(`ref: f3a96a96d7c14995bf625657ffa8eb81` no OCI, `request_id` nas rotas `/v1`), o que
torna a recusa rastreável do lado do servidor — condição necessária para I-10, mas **não
verificada**, porque não pude ler o outro lado.

**Vazou?** Procurei nos corpos e headers de todas as respostas 401 por: id de tenant,
nome de repositório de outro tenant, caminho interno, versão de build, stack. **Nada.**
As mensagens são uniformes e opacas.

---

## SEGURO

| ataque | comando | resultado |
|---|---|---|
| enumerar repositórios | `GET /v2/_catalog` | `401 {"code":"DENIED","message":"catalog endpoint disabled"}` — recusado |
| listar tags de repo adivinhado | `GET /v2/my-app/tags/list` | `401 UNAUTHORIZED` |
| ler manifest sem auth (I-3) | `GET /v2/my-app/manifests/latest` | `401 UNAUTHORIZED` — **antes** de qualquer trabalho |
| token endpoint sem credencial | `GET /token?service=…&scope=…` | `401` |
| oráculo de formato de PAT | 5 formas distintas em `/v1/users/me` | **todas 401 idênticas** — sem oráculo |

**Resultado: todos recusados.** Nenhum vazamento encontrado no que consegui atacar.

**O que NÃO consegui atacar:** o cross-tenant real (MALICE da RIVAL lendo `my-app` da
ACME) — exige dois PATs válidos. **Esta é a metade que importa da lente 4, e ela está
aberta.**

### Achado menor de conformidade OCI

`/v2/_catalog` devolve **HTTP 401** com código OCI **`DENIED`**. `DENIED` é semanticamente
403 no OCI Distribution Spec; e a mensagem `"catalog endpoint disabled"` revela estado de
configuração a um chamador **sem nenhuma credencial**. Não é vazamento de dado de cliente,
mas é divergência de spec numa página que promete "full OCI Distribution Spec v1.1".

### Header ausente

Nenhuma resposta traz `Docker-Distribution-API-Version: registry/2.0`. Não é obrigatório
na v1.1, mas vários clientes o usam para detectar registries — vale registrar contra a
promessa "any standard OCI client".

---

## O QUE A PÁGINA PROMETIA E NÃO ENTREGOU

1. **ACHADO — o PAT que a página manda usar não é parseável pelo produto.**
   A página instrui `corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX` (37 chars, 0 pontos). O
   parser canônico (`crates/corelink-pat/src/format.rs::parse_plaintext`) só aceita
   **96 chars com 2 pontos** para env `pat`. Detalhe completo, com controle positivo, em
   `A-8-cas-nativo.md` §"O PAT publicado é impossível". Vale para as 5 páginas.

2. **ACHADO — contradição de hostname entre duas páginas publicadas.**
   `oci-registry.md`: *"The registry host is `corelink-api.humangr.com`"*.
   `homebrew.md`: *"the CoreLink OCI registry (`corelink-oci.humangr.com`)"*.
   Ambos resolvem (`104.21.57.218` / `172.67.167.13`, os mesmos IPs) e ambos devolvem 401
   em `/v2/`. O `WWW-Authenticate` de `corelink-api` aponta o realm para `corelink-oci`.
   Funciona, mas **duas páginas dão nomes diferentes para o mesmo produto** e nenhuma
   menciona a outra — o cliente não sabe qual é o canônico.

3. **Não é achado, é registro:** a promessa "no tenant segment in the URL — your tenant
   is derived from the token" está **coerente** com o `scope="repository:*:pull"`
   observado. Não pude confirmá-la sem dois tenants.

---

## OS 10 INVARIANTES CONTRA ESTA ESTAÇÃO

| # | veredito | base |
|---|---|---|
| I-1 isolamento | **NÃO VERIFICADO** | exige 2 PATs válidos |
| I-2 fail-closed | **OK (parcial)** | PAT inválido → 401; nenhum caminho de erro deu acesso |
| I-3 auth antes de caro | **OK** | manifest/blob/tags/catalog todos 401 sem trabalho |
| I-4 uso contado | **NÃO VERIFICADO** | nenhum uso cobrável ocorreu |
| I-5 mutação auditável | **NÃO VERIFICADO** | nenhuma mutação ocorreu |
| I-6 segredo nunca sai | **OK (parcial)** | nenhum segredo em corpo/header de 6 respostas |
| I-7 integridade | **NÃO VERIFICADO** | nenhum blob subiu/desceu |
| I-8 idempotência | **NÃO VERIFICADO** | — |
| I-9 apagamento | **N/A** nesta estação | — |
| I-10 recusa observável | **PARCIAL** | recusa chega ao cliente com `ref:` correlacionável; o lado do log não foi lido |

---

## PAREI EM

**Parei no `docker login`.** Sem PAT de tenant criado pelo funil real de signup, as lentes
*Funciona*, *Rápido* (autenticado) e *Registrado* são inalcançáveis, e a metade que
importa da lente *Seguro* (cross-tenant com credencial válida) também. Adicionalmente
**não subi o daemon do Docker** — decisão consciente, o Mac é o runner do CI e o `pull`
terminaria no mesmo 401.
