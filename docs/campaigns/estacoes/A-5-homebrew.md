# ESTAÇÃO A-5 — Homebrew bottle mirror

**Página que promete:** `apps/docs/docs/integrations/homebrew.md`
**Publicada em:** `https://humangr.com/corelink/docs/integrations/homebrew` (200)
**Executado em:** 2026-08-31, do Mac do owner (edge GRU), **Homebrew 6.0.14**.
**Contêiner medido por `GET` por `{id}`:** `corelink-prod-sam…` **version=143**,
image tag **`ddd95560-r1`** (idêntico ao de A-4; ver A-4 para o comando).

---

## VEREDITO: **FALHOU — com um achado de segurança que a própria página não sabia que tinha**

A página já se declara não-funcional (`:::caution Experimental — not functional with
current Homebrew`). Essa parte **eu confirmei, em versão mais nova que a da doc**. O que a
página **não** diz, e que eu medi, é que seguir a receita publicada ao pé da letra
**(a) quebra um `brew` que funcionava** e **(b) manda o PAT do CoreLink para o `ghcr.io`**.

---

## ESCRITO ANTES DE MEDIR — a expectativa

A doc afirma, verificado por ela em 2026-07-19 contra Homebrew 6.0.11: *"`brew` fetched
the bottle blob straight from `ghcr.io`, **0 requests to the mirror**"*.

**Minha expectativa antes de rodar:** reproduzir os 0 requests em 6.0.14, e que o `brew`
**continuasse funcionando** (baixando do ghcr.io como sempre), já que a doc descreve a
receita como *inerte*, não como destrutiva.

**A segunda metade da expectativa estava errada.** A receita não é inerte.

---

## FUNCIONA — não

### A receita da página, verbatim (§Configure)

```bash
export HOMEBREW_CACHE=/tmp/clk-brew-cache        # isolado
export HOMEBREW_ARTIFACT_DOMAIN="https://corelink-api.humangr.com/brew/11111111-2222-3333-4444-555555555555"
export HOMEBREW_DOCKER_REGISTRY_TOKEN="corelink_pat_AAAAAAAAAAAAAAAAAAAAAAAA"
brew fetch --verbose --formula jq
```

```
==> Fetching jq from homebrew/core
✘ Bottle Manifest jq (1.8.2)
Error: Failed to download resource "jq (1.8.2)"
Download failed: https://ghcr.io/v2/homebrew/core/jq/manifests/1.8.2-1
curl: (56) The requested URL returned error: 403
rc=1
```

### O comando de verificação da própria página (§Verify it worked)

```bash
grep -c 'corelink-api.humangr.com' /tmp/brew.out
→ 0
```

Hosts efetivamente tocados, contados:

```bash
grep -oE 'https://[a-zA-Z0-9./_-]+' /tmp/brew.out | sed -E 's#(https://[^/]+).*#\1#' | sort | uniq -c
→    1 https://ghcr.io
```

**População: 1 host distinto. CoreLink: 0 requisições.** A caution da página se confirma
em Homebrew 6.0.14 (a doc mediu 6.0.11). **Artefato entregue: nenhum — nem do CoreLink
nem do upstream.**

---

## A LINHA MAIS CURTA QUE DECIDE — três controles isolam a variável

Rodei o mesmo `brew fetch --verbose --formula jq`, sempre com cache **novo e vazio**,
variando **uma** variável de ambiente por vez:

| # | `HOMEBREW_ARTIFACT_DOMAIN` | `HOMEBREW_DOCKER_REGISTRY_TOKEN` | rc | desfecho |
|---|---|---|---|---|
| **A** (controle limpo) | ausente | ausente | **0** | `✔︎ Bottle jq (1.8.2)` — **baixa e verifica o checksum. Funciona.** |
| **B** | CoreLink | ausente | 1 | `ghcr.io … error: **401**` |
| **Doc** | CoreLink | `corelink_pat_AAAA…` | 1 | `ghcr.io … error: **403**` |
| **C** (refutação) | CoreLink | `zzz-not-a-pat` | 1 | `ghcr.io … error: **403**` |
| **D** (refutação) | **ausente** | `corelink_pat_AAAA…` | 1 | `ghcr.io … error: **403**` |

### O que os controles provam

1. **A receita publicada quebra o `brew`.** Controle A funciona (rc=0, bottle baixada e
   checksum verificado). Bastam as variáveis da página para rc=1. E a página manda
   `export`, isto é, isto vale para **toda** instalação subsequente daquela sessão — e, se
   o usuário puser no `~/.zshrc` como é o hábito, **para sempre**.

2. **🔴 O PAT do CoreLink é enviado para o `ghcr.io`.** A única diferença entre **B**
   (401) e **Doc** (403) é a presença de `HOMEBREW_DOCKER_REGISTRY_TOKEN`.
   **401 = nenhuma credencial apresentada. 403 = credencial apresentada e rejeitada.**
   O Homebrew converte essa variável em `Authorization: Bearer <valor>` na requisição de
   manifest — e essa requisição vai para `https://ghcr.io/v2/homebrew/core/jq/manifests/…`,
   **não** para o CoreLink.

3. **Refutação C:** trocar o valor por `zzz-not-a-pat` mantém o 403 → o que muda o
   desfecho é a **presença** do header, não o valor. Confirma o mecanismo.

4. **Refutação D:** remover `HOMEBREW_ARTIFACT_DOMAIN` e deixar só o token **mantém o
   403** → o vazamento **não depende** do artifact domain. Basta a linha
   `export HOMEBREW_DOCKER_REGISTRY_TOKEN=corelink_pat_...` que a página manda escrever.

**Tentei derrubar o achado e não consegui.** As três hipóteses alternativas que testei —
"é o artifact domain que quebra", "é o valor do PAT que o ghcr rejeita", "é cache sujo" —
foram todas refutadas pelos controles A/C/D e pelo cache novo em cada rodada.

---

## RÁPIDO

**Não aplicável, e o motivo é o achado:** não houve nenhuma requisição ao CoreLink para
cronometrar (0 de 0). O endpoint `/brew/<tenant>/` existe e recusa em

```
GET https://corelink-api.humangr.com/brew/11111111-2222-3333-4444-555555555555/
→ http=401  {"error":"UNAUTHORIZED","message":"authentication required","request_id":"f5b3a964-…"}
```

frio/quente indistinguíveis, ~55 ms do Mac ao edge GRU — mas isso mede a recusa de auth,
não a superfície. **Nenhum número contra o alvo de 15–30 ms é produzível nesta estação.**

---

## REGISTRADO

**Não verificável** — 0 requisições ao CoreLink significa 0 linhas de trilha a ler. Não há
o que conferir do lado do servidor.

**Vazou? SIM, mas não pelo nosso servidor — pela nossa documentação.** Procurei o PAT no
output do `brew`; o `brew` não o imprime. O vazamento não está no log, está no **destino**
da requisição: um terceiro (`ghcr.io` / GitHub) recebe a credencial de cache do cliente.
Isso é **I-6 violado por instrução publicada**.

---

## SEGURO

| ataque | resultado |
|---|---|
| `GET /brew/<tenant-adivinhado>/` sem auth | `401` — recusado |
| travessia a partir do prefixo `/brew/` | `401` — recusado |
| **o "ataque" que funcionou não fui eu: foi a própria página** | o cliente exfiltra o próprio PAT para o ghcr.io ao seguir a doc |

O endpoint do CoreLink se comportou corretamente em tudo que testei. **O defeito é da
página.**

---

## O QUE A PÁGINA PROMETIA E NÃO ENTREGOU

1. **🔴 ACHADO DE SEGURANÇA — a receita publicada exfiltra o PAT do cliente para o
   `ghcr.io`.** Severidade alta: o `HOMEBREW_DOCKER_REGISTRY_TOKEN` vira `Authorization:
   Bearer` numa requisição a um terceiro; a página manda exportá-lo; o hábito é pôr no
   shell profile; e o `ghcr.io` registra tentativas de auth. Um PAT de `cache:read+write`
   do CoreLink chega em log de terceiro. **A página não avisa.** A caution existente fala
   só de *não funcionar*, nunca de *vazar*.
   Mitigação imediata possível sem código: **remover a linha do
   `HOMEBREW_DOCKER_REGISTRY_TOKEN` da página**, já que ela comprovadamente não roteia
   nada para o CoreLink (0/0) e só serve para vazar.

2. **ACHADO — a receita quebra um `brew` funcional** e a página não diz. Controle A rc=0 →
   receita rc=1. Um cliente que siga a página perde a capacidade de instalar qualquer
   fórmula, e a caution o leva a crer que apenas "não acelera".

3. **ACHADO — o comando §Verify it worked é auto-enganoso.**
   `brew install --verbose jq 2>&1 | grep corelink-api.humangr.com | head` — o `| head`
   esconde o exit code (o pipeline sai com o status do `head`), e `grep` vazio é
   exatamente o que acontece hoje. A página diz *"Seeing requests … confirms"* mas não diz
   o que significa **não** ver — que é o caso real e universal.

4. **ACHADO — contradição de hostname com a página irmã.** `homebrew.md` chama a superfície
   OCI de `corelink-oci.humangr.com`; `oci-registry.md` diz que o host do registry é
   `corelink-api.humangr.com`. Ver A-4.

5. **ACHADO — o PAT publicado é impossível de parsear** (`corelink_pat_XXXXXXXXXXXX…`,
   37 chars, 0 pontos vs. os 96 chars e 2 pontos canônicos). Ver `A-8-cas-nativo.md`.

6. **A "supported path (roadmap)" é uma promessa sem estação.** A página oferece o caminho
   OCI com *custom tap* como saída. Não existe tap publicado. É roadmap declarado como tal
   — aceitável, mas registro aqui porque C-4 exige que promessa publicada tenha linha
   correspondente.

---

## OS 10 INVARIANTES CONTRA ESTA ESTAÇÃO

| # | veredito | base |
|---|---|---|
| I-1 isolamento | **NÃO VERIFICADO** | nada chegou ao servidor |
| I-2 fail-closed | **OK** | a receita falha fechada (rc=1, nada instalado) |
| I-3 auth antes de caro | **OK** | `/brew/<t>/` → 401 sem trabalho |
| I-4 uso contado | **N/A** | 0 uso |
| I-5 mutação auditável | **N/A** | 0 mutação |
| I-6 segredo nunca sai | **🔴 VIOLADO** | PAT do cliente enviado ao `ghcr.io` ao seguir a página |
| I-7 integridade | **N/A** | 0 bytes trafegados via CoreLink |
| I-8 idempotência | **N/A** | — |
| I-9 apagamento | **N/A** | — |
| I-10 recusa observável | **PARCIAL** | 401 com `request_id`; lado do log não lido |

---

## PAREI EM

**Não parei por bloqueio de credencial nesta estação** — a estação se resolveu sem PAT
válido, porque o cliente real nunca alcança o CoreLink. O que **não** consegui verificar:
se o endpoint `/brew/<tenant>/` de fato serve bottles quando autenticado (a página afirma
que "the mirror endpoint itself works"). Essa afirmação **permanece não verificada** —
exige PAT de tenant real.

---

## RODADA 2 (2026-08-31) — lentes fechadas com DOIS tenants

O bloqueio de credencial foi **levantado**: o owner autorizou explicitamente o uso de
`CORELINK_PAT_MINT_AUTH_KEY` para provisionar tenants de teste. Dois tenants distintos
— **ACME** e **RIVAL** — foram criados, e as lentes que estavam em branco foram medidas.

- Provisionamento, calibração do instrumento e a ressalva do que este caminho **não**
  prova (o funil de cadastro): `PROVISIONAMENTO-tenants-de-teste.md`
- Medições, os 11 ataques, os invariantes e os controles: `RODADA-2-lentes-com-dois-tenants.md`

**Resultado desta superfície:**

- **NÃO exercida como escrita.** `PUT` devolve **405** (é espelho read-only) e a forma de caminho que tentei foi recusada com **403 `forbidden repo path`**. **A forma correta de caminho de bottle não foi determinada** — não afirmo nada sobre esta superfície além disto.
- **O veredito `FALHOU` da rodada 1 segue de pé, inteiro** — a página manda o cliente exfiltrar o PAT para o `ghcr.io` e quebra um `brew` que funcionava. Nada disso dependia de credencial, e nada disso foi consertado aqui.

**O veredito da rodada 1 desta estação não muda por causa disto.** Ele era sobre a
documentação publicada e o cliente real, não sobre credencial. **Nenhum conserto de
produto foi feito para esta estação passar** — achado é entrega.

