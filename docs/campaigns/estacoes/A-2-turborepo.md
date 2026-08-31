# ESTAÇÃO A-2 — Turborepo

**Página que promete:** `apps/docs/docs/integrations/turborepo.md` (publicada)
**Executado em:** 2026-08-31 · **Cliente real:** `turbo 2.10.12` via `npx` (não `curl`)
**Medido de:** Mac do fundador, loopback (controle) — produção não exercida

---

## VEREDITO: **NÃO VALIDADO CONTRA PRODUÇÃO** (bloqueio de credencial) · doc **quase** correta

Diferente de A-1: aqui a página **funciona como escrita**. O comando publicado roda,
o cliente real conecta, e o ciclo cold→warm entrega **hit remoto com o artefato
restaurado**. Duas imprecisões de doc, nenhuma fatal. Mas **nada disso foi contra
produção** — sem PAT, a lente "Funciona" não pode ser fechada.

---

## FUNCIONA

### O comando publicado roda como está escrito

A página (turborepo.md:31-41) manda exatamente isto, e foi o que executei:

```bash
export TURBO_API="https://corelink-api.humangr.com"   # apontei ao controle local
export TURBO_TOKEN="corelink_pat_..."
npx turbo run build --team=acme --token="$TURBO_TOKEN"
```

Nenhum conserto silencioso foi necessário — **contraste direto com A-1**, onde o
`.bazelrc` publicado não roda de jeito nenhum.

### Ciclo cold → warm, com o artefato na mão

```
### COLD ###
   • Remote caching enabled
web:build: cache miss, executing 8973e794d120319e
 Tasks:    1 successful, 1 total
Cached:    0 cached, 1 total
  Time:    556ms

### apaga o cache LOCAL (.turbo) — um hit só pode vir do remoto ###

### WARM ###
   • Remote caching enabled
web:build: cache hit, replaying logs 8973e794d120319e
 Tasks:    1 successful, 1 total
Cached:    1 cached, 1 total
  Time:    60ms  >>> FULL TURBO
```

```
artefato: packages/web/dist/out.txt → "a2probe-artifact"
```

**As quatro coisas que importam, juntas:** hit reportado pelo cliente, artefato
restaurado no disco, cache local apagado antes (então o hit é remoto), e **o tempo
caiu** — 556 ms → 60 ms. O GOAL §2 DIA-3 exige explicitamente "hits > 0 **e** tempo
menor"; aqui as duas condições valem (contra o controle local).

### O contrato de protocolo real (o que produção PRECISA servir)

```
GET /v8/artifacts/status?slug=acme            -> 200 {"status":"enabled"}   auth=Bearer
GET /v8/artifacts/<hash>?slug=acme            -> 404 (miss)                 auth=Bearer
PUT /v8/artifacts/<hash>?slug=acme            -> 201                        auth=Bearer
--- segunda rodada ---
GET /v8/artifacts/status?slug=acme            -> 200                        auth=Bearer
GET /v8/artifacts/<hash>?slug=acme            -> 200 (hit)                  auth=Bearer
```

**População: 5 requisições, 2 verbos (GET, PUT), 2 caminhos (`/status`, `/<hash>`).**

### Descoberta com consequência operacional: `/v8/artifacts/status` é obrigatório

Na primeira execução meu controle devolvia **404** para `/v8/artifacts/status`. O
cliente real reagiu assim:

```
 WARNING  • Remote caching unavailable (Could not connect to "http://127.0.0.1:8791")
```

…**e mesmo assim fez o PUT do artefato.** Dois fatos que valem registro:

1. Se produção não servir `GET /v8/artifacts/status` com `200` e corpo
   `{"status":"enabled"}`, **todo cliente vê "Remote caching unavailable"** — mesmo
   com as credenciais certas. A página não menciona esse endpoint em lugar nenhum.
2. A mensagem do Turbo (`Could not connect`) é **enganosa**: houve conexão, houve
   resposta HTTP, e o upload seguiu. Um cliente lendo esse aviso vai investigar rede
   e firewall, não o endpoint `/status`.

Com o `/status` devolvendo 200, o aviso vira `• Remote caching enabled` e o ciclo
fecha. Foi essa a única diferença entre as duas execuções.

---

## RÁPIDO

**Números de produção: não existem nesta estação.** Sem PAT, nenhuma requisição
autenticada foi feita a `corelink-api.humangr.com`. Os tempos abaixo são do
**controle local em loopback** e provam o mecanismo, **não** o alvo de 15-30 ms:

| estado | tempo total da task | origem |
|---|---|---|
| cold (miss + PUT) | 556 ms | mock local, loopback |
| warm (hit remoto) | **60 ms** (`FULL TURBO`) | mock local, loopback |

**`Server-Timing` decomposto: não coletado** — o header vem de produção, e produção
não foi exercida. Não preencho essa linha com dado de loopback, que mediria a minha
própria máquina e não o produto.

**Versão do contêiner por `GET` por `{id}` na Containers API: não coletada**, pela
mesma razão de A-1 — sem número de produção para parear, a versão só daria aparência
de medição.

---

## REGISTRADO

**Não verificado.** Nenhuma ação autenticada ocorreu, logo não há linha de trilha
para procurar. Lente **em aberto**, não aprovada.

---

## SEGURO

| ataque (sem credencial) | saída | veredito |
|---|---|---|
| `GET /v8/artifacts/status` sem auth | `401` | recusado |
| `GET /v8/artifacts/<hash>` sem auth | `401` | recusado |
| `GET /v8/zzz-not-real` | `404` | prefixo validado |
| PAT forjado com forma válida | `401 UNAUTHORIZED` | recusado |

**I-3: indício positivo** — refusa antes de rotear. Nenhum corpo de erro trouxe
tenant, caminho interno ou segredo.

### A pergunta de segurança que fica em aberto — e é a mais séria desta estação

A página (turborepo.md:22-26) afirma:

> "O `teamId` que o Turborepo envia é tratado como sub-namespace lógico *dentro* do
> seu tenant autenticado… **não é uma fronteira de segurança**."

Mas o cliente real **não envia `teamId`**. Envia `?slug=acme` — em todas as 5
requisições capturadas, `teamId` não aparece uma única vez. A doc descreve um
parâmetro que o Turborepo 2.10.12 não emite com `--team=`.

Isso importa porque o particionamento por time depende de qual parâmetro o servidor
lê. Se o servidor lê `teamId` e o cliente manda `slug`, dois times sob o mesmo tenant
**colidem no mesmo namespace** — dois builds diferentes com a mesma hash de task se
sobrescrevem. A própria doc diz que isso não é fronteira de segurança, então não é
vazamento entre tenants; é **corrupção de cache entre times**.

**Não confirmado nem refutado** — decidir exigiria um PAT (para observar o
comportamento do servidor) ou ler o código do servidor, e §5 proíbe validar por
leitura de código. Fica como pergunta aberta explícita, não como achado.

---

## O QUE A PÁGINA PROMETIA E NÃO ENTREGOU

| # | promessa (turborepo.md) | realidade medida | gravidade |
|---|---|---|---|
| 1 | descreve o parâmetro como `teamId` | cliente real envia `?slug=` (0 ocorrências de `teamId` em 5 requisições) | média — ver acima |
| 2 | não menciona `/v8/artifacts/status` | endpoint **obrigatório**; sem 200 o cliente diz "Remote caching unavailable" | média |
| 3 | troubleshooting cobre 401, cache miss, `400` | não cobre o modo de falha real (`/status` ausente), que se apresenta como erro de **conexão** | baixa |

O restante da página — `TURBO_API` como origem nua, PAT como `TURBO_TOKEN`, tenant
resolvido do PAT, exemplo de GitHub Actions — está **coerente com o cliente real**.

---

## O QUE NÃO CONSEGUI VERIFICAR, E POR QUÊ

1. **Tudo contra produção** — sem PAT (ver PAREI EM).
2. **Se produção serve `/v8/artifacts/status` com `{"status":"enabled"}`.** É a
   dependência que decide se a superfície funciona para o cliente, e o probe anônimo
   não alcança: `401` cobre tudo sob `/v8/` com profundidade ≥2. Controle rodado:
   `/v8/zzz-not-real` → 404, `/v8` → 404. Logo o 401 prova o **prefixo** montado,
   não a rota.
3. **`slug` vs `teamId` no servidor** — ver acima.
4. **Isolamento entre tenants (I-1)** — exige dois PATs.
5. **Lente REGISTRADO** — inteira.

---

## INVARIANTES — veredito contra esta estação (10/10)

| # | veredito |
|---|---|
| I-1 isolamento | **não testável** sem 2 PATs |
| I-2 fail-closed | **indício positivo** — anônimo e forjado → 401 |
| I-3 auth antes de caro | **indício positivo** — 401 precede roteamento |
| I-4 uso contado | **não testável** sem produção |
| I-5 mutação auditável | **não testável** sem produção |
| I-6 segredo nunca sai | **indício positivo** — token no header, nunca em argv/erro (contraste com A-1) |
| I-7 integridade | **indício positivo (mock)** — `out.txt` byte-idêntico após hit remoto |
| I-8 idempotência | **não testável** sem produção |
| I-9 apagamento | **não aplicável** a esta estação |
| I-10 recusa observável | **parcial** — 401 chega ao cliente; log não lido. Ver ressalva: a recusa do `/status` chega ao cliente com **motivo errado** ("Could not connect") |

---

## PAREI EM

**O mesmo bloqueio de A-1 e do `BLOQUEIO-credencial-de-cliente.md`:** não há PAT de
cliente obtenível. O signup publicado exige criar conta com senha, o que está fora do
que esta sessão pode fazer; e `CORELINK_PAT_MINT_AUTH_KEY` (presente em `.env.local`)
é credencial de operador, proibida por §5 — **não a usei**.

Para fechar A-2 preciso de **um** PAT (lentes Funciona/Rápido/Registrado) e de **dois**,
de tenants distintos, para I-1.
