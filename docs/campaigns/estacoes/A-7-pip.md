# ESTAÇÃO A-7 — pip (PyPI) mirror

**Página que promete:** `apps/docs/docs/integrations/pip.md`
**Publicada em:** `https://humangr.com/corelink/docs/integrations/pip` (200)
**Executado em:** 2026-08-31, do Mac do owner (edge GRU), **pip 26.1.1 / Python 3.14**.
**Contêiner medido por `GET` por `{id}`:** `corelink-prod-sam…` **version=143**, tag
**`ddd95560-r1`** (ver A-4 para o comando).

---

## VEREDITO: **BLOQUEADO** (lentes 1-3) + **2 achados**, e **1 candidato a achado que eu
## refutei** (registrado, porque refutação que falhou calibra o resto)

---

## ESCRITO ANTES DE MEDIR — a expectativa

1. `pip install --index-url https://hugr:<PAT-bogus>@…/simple/ requests` → **401**, e pip
   **não** cai de volta no PyPI (porque `--index-url` substitui o índice). ✔ confirmado
2. pip **redige** o PAT no próprio log verboso. ✔ confirmado (0 ocorrências do literal)
3. O caminho de `pip.conf` que a página dá para macOS
   (`~/Library/Application Support/pip/pip.conf`) **não** seria lido — o `pip config debug`
   lista outros caminhos de usuário. **✘ MINHA HIPÓTESE FOI REFUTADA. A página está certa.**
4. Sem auth, o mirror não busca no upstream (I-3). ✔ confirmado

---

## FUNCIONA — não, e o bloqueio é a credencial

### O comando da página, verbatim (§Configure, forma inline)

```bash
pip3 install --dry-run --target /tmp/clk-pip/t -v \
  --index-url "https://hugr:corelink_pat_AAAAAAAAAAAAAAAAAAAAAAAA@corelink-api.humangr.com/pip/11111111-2222-3333-4444-555555555555/simple/" \
  requests
```

```
Using pip 26.1.1 from /usr/local/lib/python3.14/site-packages/pip (python 3.14)
Looking in indexes: https://hugr:****@corelink-api.humangr.com/pip/11111111-2222-3333-4444-555555555555/simple/
WARNING: 401 Error, Credentials not correct for https://corelink-api.humangr.com/pip/11111111-2222-3333-4444-555555555555/simple/requests/
ERROR: Could not find a version that satisfies the requirement requests (from versions: none)
ERROR: No matching distribution found for requests
pip rc=1
```

**Artefato entregue ao usuário: nenhum.** Nenhuma wheel, nenhuma sdist. Recusa correta,
e o pip **não** caiu de volta no `pypi.org` — comportamento correto de `--index-url`.

---

## A LINHA MAIS CURTA QUE DECIDE — o troubleshooting manda o cliente errado no lugar errado

O log real de um PAT ruim produz **duas** linhas de erro. A tabela da página trata cada
uma como sintoma de causa **diferente**:

| linha que o cliente vê | o que a página diz que é | o que era de fato |
|---|---|---|
| `401 Error, Credentials not correct` | *"Username is not `hugr`, or the password is not a `corelink_pat_...` PAT"* | o username **era** `hugr` e o PAT **era** `corelink_pat_...` — a causa era o PAT não existir |
| `Could not find a version that satisfies the requirement requests` | *"Project not yet cached and upstream unreachable → **Retry**"* | o mesmo 401 |

**A linha final — a que o pip imprime por último e que o usuário lê primeiro — leva o
cliente ao conselho "Retry", que nunca vai funcionar.** E a linha de 401 aponta duas
causas que não eram a causa. Um cliente com PAT expirado ou revogado é mandado para o
loop errado pela documentação.

---

## O CANDIDATO A ACHADO QUE EU REFUTEI (Q-6)

A página diz que no macOS o `pip.conf` do usuário fica em
`~/Library/Application Support/pip/pip.conf`. O próprio `pip config debug` **não lista
esse caminho** entre os de usuário:

```
$ pip3 config debug
user:
  /Users/gustavoschneiter/.pip/pip.conf, exists: False
  /Users/gustavoschneiter/.config/pip/pip.conf, exists: False
```

Isso me pareceu um achado. **Testei antes de afirmar, com HOME isolado:**

```bash
H=/tmp/clk-piphome
# A) o arquivo SÓ no caminho que a página manda
printf '[global]\nindex-url = %s\n' "$U" > "$H/Library/Application Support/pip/pip.conf"
env HOME=$H pip3 config get global.index-url
→ https://hugr:corelink_pat_AAAA@corelink-api.humangr.com/pip/TENANT-DOC-PATH/simple/

# B) CONTROLE — o MESMO arquivo movido para ~/.config/pip/pip.conf
env HOME=$H pip3 config get global.index-url
→ ERROR: No such key - global.index-url
```

**Refutado: a página está certa e o `pip config debug` é que engana.** O caminho
documentado é o que o pip 26.1.1 realmente lê no macOS; o caminho que o `config debug`
lista como "user" é o que ele **não** lê. Registro para que ninguém "conserte" a página
com base na saída do `config debug`.

---

## RÁPIDO

| medido de | estado | operação | número |
|---|---|---|---|
| Mac → edge GRU | frio (1ª) | `GET /pip/<t>/simple/requests/` (401) | **55,5 ms** |
| Mac → edge GRU | quente (5 seguintes) | idem | 60,3 / 57,1 / 56,3 / 61,0 / 52,3 ms |

**Alvo: 15–30 ms (50 ms cross-region). Veredito: FORA — mediana ~57 ms, ~1,9× o teto de
30 ms e acima até do teto cross-region de 50 ms.** Frio e quente indistinguíveis: sem
aquecimento no caminho de recusa.

**Isto mede a RECUSA.** O caminho servido — índice Simple, wheel do CAS do tenant, frio
vs. quente — **não foi medido** (exige PAT). A promessa da página de mirror mais rápido
não tem número nesta rodada.

**`Server-Timing`:** ausente em `/pip/**`. Das 6 superfícies verificadas, só `/v2/`
emite algo, e emite `oother;dur=0` (ver A-4).

---

## REGISTRADO

**Não verificável nesta rodada** — nenhuma ação chegou a resolver um tenant; ler a trilha
de um tenant exige ser aquele tenant (§5 proíbe a via de operador).

**Vazou?** Duas superfícies verificadas:

- **Servidor:** procurei nas respostas por nome de pacote de outro tenant, id de tenant,
  caminho interno, versão de build. **Nada.** Corpo uniforme
  `{"error":"UNAUTHORIZED","message":"authentication required","request_id":"<uuid>"}`.
- **Cliente:** contei ocorrências literais do PAT no log verboso do pip:
  ```bash
  grep -c "corelink_pat_AAAA" /tmp/pip.out  →  0
  grep -n "hugr:" /tmp/pip.out              →  Looking in indexes: https://hugr:****@…
  ```
  **pip redige o PAT.** Bom — mas é o pip que redige, **não** uma escolha do CoreLink.

### Ressalva sobre I-6 que a página cria e não nomeia

A receita canônica da página é **credencial embutida na URL**
(`https://hugr:<PAT>@…`). O GOAL §7 I-6 diz explicitamente *"segredo nunca sai … nem em
URL"*. O pip 26.1.1 protege o cliente por conta própria; **nada na página garante isso**,
e a mesma URL vai para `pip.conf` em disco, para o histórico do shell, e para qualquer
ferramenta que registre a linha de comando. **Não é achado confirmado** (não observei
vazamento efetivo), é um risco criado pela receita publicada — registro para o lead.

---

## SEGURO

| ataque | comando | resultado |
|---|---|---|
| tenant adivinhado | `GET /pip/11111111-…-555555555555/simple/requests/` | `401` |
| enumerar o índice do tenant | `GET /pip/<t>/simple/` | `401` — sem lista de pacotes |
| travessia | `GET /npm/<t>/../../v1/users/me` (mesmo prefixo de roteamento) | `401` |
| I-3: forçar busca no PyPI sem auth | `GET /pip/<t>/simple/requests/` | `401` em ~55 ms, **sem** ida ao `pypi.org` |
| oráculo de tenant | tenant inexistente vs. PAT ruim | **mesma resposta** — sem oráculo |

**Resultado: todos recusados.**

**O que NÃO consegui atacar:** RIVAL lendo o cache da ACME — **a metade decisiva de I-1,
aberta.**

---

## O QUE A PÁGINA PROMETIA E NÃO ENTREGOU

1. **ACHADO — a tabela §Troubleshooting mapeia o sintoma real de PAT inválido para a causa
   errada e para o conselho "Retry"**, que não pode funcionar. Provado com o log real
   acima (username correto, PAT correto em forma, erro atribuído a username/formato).

2. **ACHADO — o PAT publicado é impossível de parsear.**
   `corelink_pat_XXXXXXXXXXXX` (25 chars, 0 pontos) na `pip.conf`; canônico são **96 chars
   com 2 pontos**. Ver `A-8-cas-nativo.md` com controle positivo. Vale para as 5 páginas.

3. **Promessas não verificadas** (exigem PAT — nem confirmadas nem refutadas):
   - *"implements the Simple Index API (PEP 691 JSON, with PEP 503 HTML fallback)"* — não
     exercitado; um `Accept:` de PEP 691 recebe 401 como qualquer outro.
   - *"integrity-checked against the upstream `#sha256=` fragment before caching"* —
     nenhuma evidência acessível.
   - *"does not accept `twine upload`"* — negativa não executada.
   - **`--index-url` (substituição total do PyPI) só é seguro se o mirror for completo.**
     A página usa `--index-url`, não `--extra-index-url`, o que é a escolha **certa para um
     mirror** — mas significa que **qualquer** buraco de cobertura vira falha total de
     instalação, sem fallback. A completude do mirror **não foi verificada** e é a
     premissa que sustenta essa escolha. Item para o lead.

4. **Refutado, não é achado:** o caminho de `pip.conf` no macOS. Ver seção de refutação.

---

## OS 10 INVARIANTES CONTRA ESTA ESTAÇÃO

| # | veredito | base |
|---|---|---|
| I-1 isolamento | **NÃO VERIFICADO** | exige 2 PATs |
| I-2 fail-closed | **OK (parcial)** | 401 → rc=1, nada instalado, sem fallback silencioso ao PyPI |
| I-3 auth antes de caro | **OK** | 401 em ~55 ms, sem ida ao upstream |
| I-4 uso contado | **N/A** | 0 uso cobrável |
| I-5 mutação auditável | **N/A** | mirror read-only |
| I-6 segredo nunca sai | **OK medido / RISCO na receita** | pip redige; mas a página embute o PAT em URL, contra a letra de I-6 |
| I-7 integridade | **NÃO VERIFICADO** | `#sha256=` não exercitado |
| I-8 idempotência | **NÃO VERIFICADO** | — |
| I-9 apagamento | **N/A** | — |
| I-10 recusa observável | **PARCIAL** | recusa chega ao cliente com `request_id`; log não lido |

---

## PAREI EM

**Parei no primeiro `pip install`.** Sem PAT de tenant criado pelo funil real não há
índice resolvido, não há wheel, não há segundo install, não há número contra o alvo e não
há trilha a ler. Os dois achados acima não dependem disso.

---

## RODADA 2 (2026-08-31) — lentes fechadas com DOIS tenants

O bloqueio de credencial foi **levantado**: o owner autorizou explicitamente o uso de
`CORELINK_PAT_MINT_AUTH_KEY` para provisionar tenants de teste. Dois tenants distintos
— **ACME** e **RIVAL** — foram criados, e as lentes que estavam em branco foram medidas.

- Provisionamento, calibração do instrumento e a ressalva do que este caminho **não**
  prova (o funil de cadastro): `PROVISIONAMENTO-tenants-de-teste.md`
- Medições, os 11 ataques, os invariantes e os controles: `RODADA-2-lentes-com-dois-tenants.md`

**Resultado desta superfície:**

- **Funciona:** SIM — `/pip/<tenant>/simple/six/` devolve o índice, 14 807 B. (`/pip/<t>/six/` e `/pip/<t>/simple/` dão 404 — o prefixo `simple/` é obrigatório.)
- **Isolamento:** SUSTENTADO — RIVAL no caminho do ACME recebe **403 `tenant mismatch`**.
- **Rápido:** `total;dur` 263 / 288 / 304 ms (n=3) — **a mais rápida das oito superfícies, e ainda ~10x fora** do alvo.
- **Correção do veredito de bloqueio:** era por falta de credencial.

**O veredito da rodada 1 desta estação não muda por causa disto.** Ele era sobre a
documentação publicada e o cliente real, não sobre credencial. **Nenhum conserto de
produto foi feito para esta estação passar** — achado é entrega.

