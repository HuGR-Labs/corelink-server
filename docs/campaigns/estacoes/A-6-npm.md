# ESTAÇÃO A-6 — npm registry mirror

**Página que promete:** `apps/docs/docs/integrations/npm.md`
**Publicada em:** `https://humangr.com/corelink/docs/integrations/npm` (200)
**Executado em:** 2026-08-31, do Mac do owner (edge GRU), **npm 10.9.2**.
**Contêiner medido por `GET` por `{id}`:** `corelink-prod-sam…` **version=143**, tag
**`ddd95560-r1`** (ver A-4 para o comando exato).

---

## VEREDITO: **BLOQUEADO** (lentes 1-3) + **3 achados de documentação**, um deles é um
## comando da própria página que **não funciona para nenhum cliente do CoreLink**

---

## ESCRITO ANTES DE MEDIR — a expectativa

1. `.npmrc` conforme a página → `npm install` sai com **401** (PAT bogus). ✔ confirmado
2. `npm config get registry` → imprime a URL do CoreLink, provando que o `.npmrc` pegou.
   **✘ REFUTADO — ver abaixo.**
3. Sem auth, o mirror **não** busca no upstream (I-3: nada caro antes da credencial).
   ✔ confirmado (401 em 135 ms, sem latência de ida ao `registry.npmjs.org`).
4. O tenant é um UUID inventado (`11111111-…-555555555555`): esperava 401, **não** 404 e
   **não** um erro que distinguisse "tenant não existe" de "PAT inválido". ✔ confirmado —
   sem oráculo de existência de tenant.

---

## FUNCIONA — não, e o bloqueio é a credencial

### Configuração, verbatim da página (§Configure `.npmrc`)

```ini
registry=https://corelink-api.humangr.com/npm/11111111-2222-3333-4444-555555555555/
//corelink-api.humangr.com/npm/11111111-2222-3333-4444-555555555555/:_authToken=corelink_pat_AAAAAAAAAAAAAAAAAAAAAAAA
```
```json
{"name":"acme-probe","version":"1.0.0","dependencies":{"left-pad":"1.3.0"}}
```

### O comando da página (§Verify it worked)

```bash
npm install --loglevel http
```
```
npm http fetch GET 401 https://corelink-api.humangr.com/npm/***/left-pad 135ms (cache skip)
npm error code E401
npm error 401 Unauthorized - GET https://corelink-api.humangr.com/npm/***/left-pad - UNAUTHORIZED
```
```
$ npm install --loglevel http > /tmp/npm.out 2>&1; echo "REAL npm rc=$?"
REAL npm rc=1
$ ls node_modules
ls: node_modules: No such file or directory
```

**Artefato entregue ao usuário: nenhum.** Nenhum pacote instalado. Recusa correta.

> **Nota de método (Q-2 / "sem pipe que inverte"):** na primeira execução eu li
> `npm rc=0` porque o comando estava atrás de um `| tail -25` — o exit status era o do
> `tail`. Reexecutei **sem pipe** e o código real é **1**. Registro o próprio erro porque
> ele é exatamente a armadilha que o `§Verify it worked` da página irmã (`homebrew.md`)
> ainda contém.

---

## A LINHA MAIS CURTA QUE DECIDE — o comando de troubleshooting da página está morto

A página, na linha "Installs still hit `registry.npmjs.org`", manda:
*"Confirm the `.npmrc` scope (project vs. user) and re-run **`npm config get registry`**"*.

```bash
$ cd /tmp/clk-npm && npm config get registry
npm error The registry option is protected, and can not be retrieved in this way
rc=1
```

### Controles que isolam a causa

| # | `.npmrc` | `npm config get registry` | rc |
|---|---|---|---|
| **1** | `registry=` CoreLink **+** linha `_authToken` (a receita da página) | `npm error The registry option is protected…` | **1** |
| **2** (controle) | **só** `registry=` CoreLink, sem token | `npm error The registry option is protected…` | **1** |
| **3** (controle) | nenhum `.npmrc` (default npmjs) | `https://registry.npmjs.org/` | **0** |

**O controle 3 prova que o instrumento enxerga** (o comando funciona e imprime uma URL).
O controle 2 prova que a causa **não** é o token — é o `registry=` apontar para fora do
default. npm 10.9.2 trata qualquer registry não-default como config **protegida** e se
recusa a imprimi-la.

**Consequência: o passo de diagnóstico que a página oferece é o único passo que um
cliente do CoreLink jamais conseguirá executar.** Quem seguir a página e tiver problema
recebe um erro do npm no lugar da resposta, e fica sem saber se o `.npmrc` pegou.

---

## RÁPIDO

| medido de | estado | operação | número |
|---|---|---|---|
| Mac → edge GRU | frio (1ª) | `GET /npm/<t>/left-pad` (401) | **60,2 ms** |
| Mac → edge GRU | quente (5 seguintes) | idem | 47,4 / 53,2 / 55,7 / 58,3 / 50,7 ms |
| npm (medido pelo próprio cliente) | — | `GET … /left-pad` | **135 ms** |

**Alvo: 15–30 ms (50 ms cross-region). Veredito: FORA — ~53 ms de mediana no `curl`, ~1,8×
o teto de 30 ms; e o próprio npm reporta 135 ms, ~4,5× o teto.** Frio e quente não se
distinguem (60 vs 47–58 ms): não há aquecimento no caminho de recusa.

**Este número mede a RECUSA, não a superfície.** O caminho servido (metadata + tarball do
CAS do tenant, frio vs. quente) **não foi medido** — exige PAT válido. Nenhum número da
promessa "repeat installs are faster" é produzível nesta rodada.

**`Server-Timing`:** ausente em `/npm/**` (verificado nas 6 superfícies; só `/v2/` emite
algo, e emite `oother;dur=0` — ver A-4). **§3.2 do GOAL exige o header decomposto; nesta
superfície ele não existe.**

---

## REGISTRADO

**Não verificável nesta rodada.** Nenhuma ação foi atribuída a um tenant (todas foram
recusadas antes da resolução do tenant), e ler a trilha de um tenant exige ser aquele
tenant. Regra anti-trapaça §5 proíbe a via de operador.

**Vazou?** Procurei nos corpos e headers das respostas por: nome de pacote de outro
tenant, id de tenant, caminho interno, versão de build. **Nada.** A resposta é
`{"error":"UNAUTHORIZED","message":"authentication required","request_id":"<uuid>"}` —
uniforme, com identificador de correlação.

Do lado do cliente, o npm **redige o tenant** no seu próprio log
(`/npm/***/left-pad`) — bom comportamento, ainda que acidental.

---

## SEGURO

| ataque | comando | resultado |
|---|---|---|
| tenant adivinhado | `GET /npm/11111111-…-555555555555/left-pad` | `401` — sem distinguir "tenant inexistente" de "PAT inválido" |
| enumerar a raiz do tenant | `GET /npm/<t>/` | `401` |
| travessia para rota de identidade | `GET /npm/../v1/users/me` | `401` |
| travessia profunda | `GET /npm/<t>/../../v1/users/me` | `401` |
| I-3: forçar busca upstream sem auth | `GET /npm/<t>/left-pad` sem token | `401` em 135 ms — **não** houve ida ao `registry.npmjs.org` |

**Resultado: todos recusados.** Nenhum oráculo de existência de tenant, nenhuma travessia.

**O que NÃO consegui atacar:** RIVAL lendo o cache da ACME com PAT válido de outro
tenant — **a metade decisiva de I-1, e ela está aberta.**

---

## O QUE A PÁGINA PROMETIA E NÃO ENTREGOU

1. **ACHADO — `npm config get registry` (§Troubleshooting) é impossível para clientes do
   CoreLink.** Provado com 3 controles acima. O passo de diagnóstico publicado erra 100%
   das vezes na configuração que a própria página manda criar.

2. **ACHADO — o PAT publicado (`corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX`, 37 chars, 0
   pontos) não é parseável pelo produto** (canônico: 96 chars, 2 pontos). Ver
   `A-8-cas-nativo.md`, com controle positivo. Vale para as 5 páginas.

3. **ACHADO — `§Verify it worked` termina em `| head`**, cujo exit status mascara o do
   `npm`. Mesmo defeito de `homebrew.md`. E a página só diz o que significa **ver** as
   requisições, nunca o que significa **não** ver.

4. **Promessas não verificadas** (nem confirmadas nem refutadas — exigem PAT):
   - *"Tarballs are … integrity-checked against the publisher's `dist.shasum` before
     caching"* — nenhuma evidência acessível ao cliente.
   - *"repeat installs — especially in CI — are faster"* — nenhum número.
   - *"resilient to upstream outages"* — não exercitado.
   - *"does not accept `npm publish`"* — negativa não executada (exigiria PAT).

---

## OS 10 INVARIANTES CONTRA ESTA ESTAÇÃO

| # | veredito | base |
|---|---|---|
| I-1 isolamento | **NÃO VERIFICADO** | exige 2 PATs; sem oráculo de tenant no que testei |
| I-2 fail-closed | **OK (parcial)** | 401 → nada instalado, rc=1; nenhum erro virou acesso |
| I-3 auth antes de caro | **OK** | 401 em 135 ms, sem ida ao upstream |
| I-4 uso contado | **N/A** | 0 uso cobrável |
| I-5 mutação auditável | **N/A** | mirror é read-only; 0 mutação |
| I-6 segredo nunca sai | **OK (parcial)** | nada em 6 respostas; npm redige o tenant no próprio log |
| I-7 integridade | **NÃO VERIFICADO** | `dist.shasum` não exercitado |
| I-8 idempotência | **NÃO VERIFICADO** | — |
| I-9 apagamento | **N/A** | — |
| I-10 recusa observável | **PARCIAL** | recusa chega ao cliente com `request_id`; o log não foi lido |

---

## PAREI EM

**Parei no primeiro `npm install`.** Sem PAT de tenant criado pelo funil real, não há
população de cache, não há segundo install, não há hit, não há número contra o alvo, e não
há trilha a ler. Os três achados acima **não** dependem disso e ficam de pé.

---

## RODADA 2 (2026-08-31) — lentes fechadas com DOIS tenants

O bloqueio de credencial foi **levantado**: o owner autorizou explicitamente o uso de
`CORELINK_PAT_MINT_AUTH_KEY` para provisionar tenants de teste. Dois tenants distintos
— **ACME** e **RIVAL** — foram criados, e as lentes que estavam em branco foram medidas.

- Provisionamento, calibração do instrumento e a ressalva do que este caminho **não**
  prova (o funil de cadastro): `PROVISIONAMENTO-tenants-de-teste.md`
- Medições, os 11 ataques, os invariantes e os controles: `RODADA-2-lentes-com-dois-tenants.md`

**Resultado desta superfície:**

- **Funciona:** SIM, **com artefato real na mão** — `/npm/<tenant>/left-pad/-/left-pad-1.3.0.tgz` devolve 3 619 B, `sha1 = 5b8a3a7765dfe001261dde915589e782f8c94d1e`, **idêntico ao `registry.npmjs.org`** (baixado e comparado com `cmp`). `tar tzf` lista `package/package.json`. **I-7 sustentado contra o upstream, não contra mim mesmo.**
- **Isolamento:** SUSTENTADO — RIVAL no caminho do ACME recebe **403 `tenant mismatch`**.
- **Rápido:** `total;dur` 596 / 665 / 1213 ms (n=3) — **~22x fora** do alvo.
- **Correção do veredito de bloqueio:** o `BLOQUEADO` era por falta de credencial; o espelho serve pacote real com PAT.

**O veredito da rodada 1 desta estação não muda por causa disto.** Ele era sobre a
documentação publicada e o cliente real, não sobre credencial. **Nenhum conserto de
produto foi feito para esta estação passar** — achado é entrega.

