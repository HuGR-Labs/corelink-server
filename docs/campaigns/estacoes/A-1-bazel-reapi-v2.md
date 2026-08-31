# ESTAÇÃO A-1 — Bazel REAPI v2

**Página que promete:** `apps/docs/docs/integrations/bazel.md` (publicada)
**Executado em:** 2026-08-31 · **Cliente real:** `bazel 9.2.0` (não `curl`)
**Medido de:** Mac do fundador (`darwin-sandbox`), rede residencial → Cloudflare edge GRU

---

## VEREDITO: **FALHOU** (na doc) + **NÃO VALIDADO CONTRA PRODUÇÃO** (bloqueio de credencial)

Dois vereditos distintos, e a distinção importa:

- **O protocolo funciona.** O binário real do Bazel completa um ciclo cold→warm e
  reporta `1 remote cache hit`, com o artefato restaurado — provado contra um
  servidor de controle local.
- **A página publicada não funciona.** Copiar o `.bazelrc` da doc **quebra o build**
  e **imprime o PAT em texto claro** no console. Nenhum cliente chega ao cache
  seguindo a página como está escrita.
- **Produção não foi exercida** — sem PAT de cliente obtenível (ver PAREI EM).

---

## FUNCIONA

### O achado principal: o `.bazelrc` publicado não roda

A página (bazel.md:43-55) manda copiar:

```ini
build --remote_instance_name=${CORELINK_TENANT}
build --remote_header=Authorization=Bearer ${CORELINK_PAT}
```

e diz "Export both values before building". Executado **verbatim**:

```
comando : bazel --output_user_root=/tmp/a13probe/br build //:hello
          (com CORELINK_PAT e CORELINK_TENANT exportados, como a página manda)
saída   : ERROR: Skipping '${CORELINK_PAT}': no such target '//:${CORELINK_PAT}':
                 target '${CORELINK_PAT}' not declared in package ''
          ERROR: Build did NOT complete successfully
```

**Duas falhas independentes na mesma linha:**

1. **`.bazelrc` não expande variáveis de ambiente.** `${CORELINK_PAT}` chegou
   literal ao Bazel. A instrução "export both values before building" é falsa —
   exportar não tem efeito nenhum sobre um arquivo `.bazelrc`.
2. **O espaço em `Bearer <token>` parte o flag.** Bazel tokeniza `.bazelrc` por
   espaço, sem regras de shell. `--remote_header=Authorization=Bearer X` vira
   dois argumentos, e `X` é interpretado como **alvo de build**.

Com um PAT literal no lugar da variável, a segunda falha aparece sozinha e
**vaza o segredo**:

```
saída   : ERROR: Skipping 'corelink_pat_PROBEVALUE.x.y': no such target
                 '//:corelink_pat_PROBEVALUE.x.y'
```

→ O PAT é ecoado **verbatim** no stderr, isto é, no log de CI de qualquer cliente
que siga a página. Violação de **I-6 (segredo nunca sai)** causada pela própria
documentação.

### Controle — a mesma workspace funciona com a config corrigida

Sem controle ao lado, "o build quebrou" e "meu teste quebrou" teriam a mesma saída.
Única mudança: aspas em volta do header e token literal.

```ini
build --remote_header="Authorization=Bearer corelink_pat_PROBE.aa.bb"
```

```
saída : INFO: Build completed successfully, 2 total actions
```

→ A falha é **da config publicada**, não da minha workspace.

### Ciclo cold → warm, com o artefato na mão

```
comando : bazel clean && bazel shutdown && bazel build //:hello
saída   : INFO: 2 processes: 1 remote cache hit, 1 internal.
          INFO: Build completed successfully, 2 total actions
artefato: bazel-bin/hello.txt → "corelink-a1-probe"  (restaurado do cache)
```

`bazel shutdown` foi necessário: sem ele o servidor Bazel serve da memória e faz
**zero** requisições — a primeira tentativa de "warm" mediu nada. Registrado
porque um cold/warm sem shutdown produz um falso verde.

### O contrato de protocolo real (o que produção PRECISA servir)

Capturado de um servidor de controle local que registra cada requisição:

```
GET  /bazel/v2/ac/<sha256>    -> 404 (miss)      auth=Bearer
PUT  /bazel/v2/cas/<sha256>   -> 201             auth=Bearer   (x3)
PUT  /bazel/v2/ac/<sha256>    -> 201             auth=Bearer
--- após shutdown ---
GET  /bazel/v2/ac/<sha256>    -> 200 (hit)       auth=Bearer
GET  /bazel/v2/cas/<sha256>   -> 200             auth=Bearer
```

**População: 7 requisições, 2 verbos (GET, PUT), 2 famílias de caminho (`/ac/`, `/cas/`).**

### A promessa que a página NÃO entrega

A página abre (bazel.md:13-17) com:

```text
https://corelink-api.humangr.com/bazel/v2/<your-tenant-id>/blobs/<hash>/<size>
```
> "The `<instance>` path segment is your tenant UUID."

**O cliente real nunca emite esse caminho.** Bazel com `--remote_cache=https://…`
fala o protocolo **HTTP REST cache**, não ByteStream (ByteStream é gRPC). As 7
requisições capturadas mostram:

- **zero** caminhos `/blobs/<hash>/<size>`;
- **zero** ocorrências do tenant — `--remote_instance_name=acme-prod` **nunca
  chegou ao fio**. `instance_name` é um conceito gRPC/REAPI; com cache HTTP é
  ignorado silenciosamente.

Consequências diretas na página:
- A linha `build --remote_instance_name=${CORELINK_TENANT}` é inerte.
- A linha de troubleshooting *"`PERMISSION_DENIED`/403 → Set `--remote_instance_name`
  to your tenant UUID"* é **inacionável**: o flag não afeta requisição nenhuma.
- O tenant só pode vir do PAT (como a página do Turborepo corretamente descreve).

**Corroboração independente:** o exemplo que o repo de fato entrega,
`examples/bazel-starter/.bazelrc:22`, aponta para `/bazel/cache` — o alias
stock-HTTP, coerente com o que o binário emite. O exemplo está certo; **a página
publicada é que está errada.**

---

## RÁPIDO

**Não medido contra produção — e portanto não reportado.** Nenhum número de
latência de produção existe nesta estação porque nenhuma requisição autenticada
foi possível (ver PAREI EM). Os tempos abaixo são do **servidor de controle
local** e servem só para provar o mecanismo, **não** para comparar com o alvo de
15-30 ms:

| estado | tempo | origem |
|---|---|---|
| cold (popula) | `Elapsed 0.211s` | mock local, loopback |
| warm (`1 remote cache hit`) | `Elapsed 6.228s` | mock local, inclui start do servidor Bazel |

O 6.2 s do "warm" é dominado pelo restart do servidor Bazel que eu forcei, não
por rede — por isso **não** é uma medida de cache e não deve ser lida como uma.

**Versão do contêiner por `GET` por `{id}` na Containers API: não coletada.** Ela
só tem sentido pareada com números de produção; sem exercício de produção, anotar
uma versão daria falsa impressão de que houve medição. Declarado ausente em vez
de preenchido.

---

## REGISTRADO

**Não verificado.** A trilha de auditoria só registra ações autenticadas, e
nenhuma ação autenticada foi possível. Ler `wrangler tail` exigiria credencial de
operador — que o cliente não tem (§5). Lente **em aberto**, não aprovada.

---

## SEGURO

### Ataques executados (sem credencial — o que a MALICE consegue antes de ter conta)

| ataque | saída | veredito |
|---|---|---|
| `GET /v1/users/me` sem auth | `401 {"error":"UNAUTHORIZED"}` | recusado |
| `GET /bazel/v2/cas/<sha>` sem auth | `401` | recusado |
| PAT com forma válida mas forjado (`corelink_pat_deadbeef.deadbeef.deadbeef`) | `401 UNAUTHORIZED` | recusado |
| `GET /bazel/totally-fake/cas/<sha>` | `404` | prefixo validado |

**I-3 (autenticado antes de caro): indício POSITIVO.** `/v1/definitely-not-a-route-xyz123`
responde **401, não 404** — a autenticação dispara antes do roteamento, então uma
requisição anônima não consome trabalho de rota. Nenhum corpo de erro vazou
tenant, caminho interno ou id.

### Achado de segurança (severidade média, na documentação interna)

`docs/integrations/bazel-vs-buck2.md:68` publica:

```ini
build --credential_helper=%workspace%/.bazel/corelink-credential-helper.sh
```

**sem prefixo de host.** Um `--credential_helper` sem escopo é invocado para
**todo** host que o Bazel contata — `bcr.bazel.build`, mirrors de `http_archive`,
qualquer registry — entregando o PAT do cliente a terceiros.

**Tentativa de refutação (Q-6), e ela derrubou parte do achado:**
- `apps/docs/docs/integrations/` contém **8 páginas**, e `bazel-vs-buck2.md`
  **não** é uma delas → o arquivo **não é servido no site**. Não é superfície de
  cliente.
- O exemplo que de fato é entregue, `examples/bazel-starter/.bazelrc:29`, está
  **corretamente escopado**: `--credential_helper=corelink-api.humangr.com=%workspace%/...`,
  com o comentário `The <host>= prefix is not optional`.

→ Rebaixado de crítico para **médio**: é uma armadilha de copy-paste num
documento interno que **contradiz** o exemplo correto que o produto entrega.
Não houve vazamento de PAT para terceiro nesta estação.

---

## O QUE A PÁGINA PROMETIA E NÃO ENTREGOU

| # | promessa (bazel.md) | realidade medida |
|---|---|---|
| 1 | `.bazelrc` copiável com `${CORELINK_PAT}` | `.bazelrc` não expande env vars; build falha |
| 2 | `--remote_header=Authorization=Bearer …` | espaço parte o flag; PAT vira alvo de build e **é impresso no stderr** |
| 3 | URL `/bazel/v2/<tenant>/blobs/<hash>/<size>` | cliente real emite `/bazel/v2/{ac,cas}/<sha256>`; zero `/blobs/` |
| 4 | "The `<instance>` path segment is your tenant UUID" | `--remote_instance_name` nunca chega ao fio |
| 5 | troubleshooting: 403 → ajuste `--remote_instance_name` | inacionável; o flag não afeta requisição alguma |

---

## O QUE NÃO CONSEGUI VERIFICAR, E POR QUÊ

1. **Qualquer coisa contra produção** — sem PAT de cliente (ver PAREI EM).
2. **Qual das duas formas de caminho produção realmente serve.** O probe anônimo
   é cego aqui, e eu **testei o controle antes de concluir**:
   - `/bazel/v2/cas/<sha>` → 401 · `/bazel/v2/<tenant>/blobs/<sha>/13` → 401
   - **mas** `/bazel/v2/zzz/<sha>` → 401 também, e `/bazel/v2/zzz/zzz/zzz` → 401.
   - Só `/bazel/v2` (profundidade 2) → 404 e `/bazel/totally-fake/cas/<sha>` → 404.

   → O 401 cobre **tudo** com profundidade ≥3 sob um prefixo válido. Portanto os
   401 provam apenas que os prefixos `/bazel/v2/` e `/bazel/cache/` estão
   montados — **não** provam que qualquer rota específica existe. Registro isso
   explicitamente porque o mesmo 401 poderia ser lido como confirmação, e não é.
3. **Isolamento de tenant (I-1)** — exige dois PATs de tenants distintos. Não
   testável. **Pergunta em aberto e material**, dado o achado #3/#4: se o caminho
   não carrega tenant, todo o isolamento do Bazel depende exclusivamente da
   resolução via PAT no servidor. Não verificado nem refutado.
4. **Lente REGISTRADO** — inteira.

---

## INVARIANTES — veredito contra esta estação (10/10 com veredito nomeado)

| # | veredito |
|---|---|
| I-1 isolamento | **não testável** sem 2 PATs — em aberto, e material (ver acima) |
| I-2 fail-closed | **indício positivo** — anônimo e PAT forjado → 401 |
| I-3 auth antes de caro | **indício positivo** — 401 precede o roteamento |
| I-4 uso contado | **não testável** sem produção |
| I-5 mutação auditável | **não testável** sem produção |
| I-6 segredo nunca sai | **VIOLADO pela documentação** — o `.bazelrc` publicado faz o Bazel imprimir o PAT no stderr |
| I-7 integridade | **indício positivo (mock)** — `hello.txt` byte-idêntico após hit |
| I-8 idempotência | **não testável** sem produção |
| I-9 apagamento | **não aplicável** a esta estação |
| I-10 recusa observável | **parcial** — 401 chega ao cliente; o lado do log não foi lido |

---

## PAREI EM

**Obter um PAT de cliente.** Caminho publicado (`quickstart.md:18`,
`tutorial/02-first-pat.mdx:14`): assinar em `humangr.com/corelink/sign-up`, um
wizard Clerk que exige **criar conta com senha**. `concepts/tenancy.md:38`
confirma que o PAT inicial do signup é **a única** via self-service —
`POST /v1/pats` não está montada.

Dois bloqueios somados:
1. **Criar conta e digitar senha está fora do que me é permitido fazer.** Não é
   escolha de escopo; é limite meu. O owner precisa executar o signup e me
   entregar o PAT, ou autorizar outra via.
2. `CORELINK_PAT_MINT_AUTH_KEY` existe em `.env.local`, **e não a usei**: §5
   proíbe credencial de admin e endpoint interno. Validar por essa porta
   certificaria a porta dos fundos, não o produto.

**Consequência honesta: a estação A-1 não pode receber "validado" hoje.** O que
está provado é o protocolo e o defeito da doc; produção segue não exercida.

---

## RODADA 2 (2026-08-31) — lentes fechadas com DOIS tenants

O bloqueio de credencial foi **levantado**: o owner autorizou explicitamente o uso de
`CORELINK_PAT_MINT_AUTH_KEY` para provisionar tenants de teste. Dois tenants distintos
— **ACME** e **RIVAL** — foram criados, e as lentes que estavam em branco foram medidas.

- Provisionamento, calibração do instrumento e a ressalva do que este caminho **não**
  prova (o funil de cadastro): `PROVISIONAMENTO-tenants-de-teste.md`
- Medições, os 11 ataques, os invariantes e os controles: `RODADA-2-lentes-com-dois-tenants.md`

**Resultado desta superfície:**

- **Funciona:** SIM — 29 B byte-idênticos em `/bazel/cache/cas/<sha256>`.
- **Isolamento:** SUSTENTADO — RIVAL recebe 404 no **mesmo caminho sem tenant**, com controle positivo do ACME (200 + bytes) provando que a escrita havia pegado. **A pergunta que estava aberta está respondida: o tenant vem do PAT.**
- **Rápido:** `total;dur` ~497 ms quente (mediana, n=6, faixa 366-630) — **~17x fora** do alvo de 30 ms. PUT frio 2463 ms.
- **Registrado:** 48 eventos no outbox dos dois tenants, **0** na trilha visível ao cliente.
- **ACHADO NOVO (corrobora #3/#4 pelo lado do servidor):** `/bazel/v2/cas/<sha>` — exatamente o que o binário real emite quando apontado para `.../bazel/v2` — responde **403 `tenant mismatch`**, porque o segmento após `v2` é lido como tenant. A população de rotas Bazel montadas é **6**, e **nenhuma** tem a forma `/bazel/v2/{ac,cas}/<hash>`.

**O veredito da rodada 1 desta estação não muda por causa disto.** Ele era sobre a
documentação publicada e o cliente real, não sobre credencial. **Nenhum conserto de
produto foi feito para esta estação passar** — achado é entrega.

