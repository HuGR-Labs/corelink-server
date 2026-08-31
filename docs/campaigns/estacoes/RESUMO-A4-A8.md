# RESUMO — estações A-4 … A-8 (as cinco superfícies desta sessão)

**Sessão:** `audit-report-analysis-4e1455` · **Data:** 2026-08-31 · **Origem das medidas:**
Mac do owner → edge Cloudflare **GRU**.
**Contêiner medido por `GET` por `{id}`** (nunca pela LISTA, que é defasada):

```
GET /accounts/$ACC/containers/applications/a0337243-13cb-46ef-adb1-781294294404
→ corelink-prod-sam-corelinkserver-prod-sam  version=143  image=…:ddd95560-r1  instances=15
GET /accounts/$ACC/containers/applications/a033572c-0803-4866-b3a3-61f4812843b1
→ corelink-prod-corelinkserver-prod          version=178  image=…:ddd95560-r1  instances=18
```

---

## C-2 / C-3 — POPULAÇÃO 5, TODO ELEMENTO COM VEREDITO, A SOMA FECHA

| # | estação | veredito | por quê, em uma linha |
|---|---|---|---|
| A-4 | OCI registry | **BLOQUEADO** | `docker login` recusa corretamente; sem PAT não há `pull`, nem camada, nem trilha |
| A-5 | Homebrew | **FALHOU** | a receita publicada não roteia nada (0/0), **quebra um `brew` funcional**, e **manda o PAT para o `ghcr.io`** |
| A-6 | npm | **BLOQUEADO** | mirror recusa certo; o comando §Troubleshooting da página (`npm config get registry`) é impossível para clientes do CoreLink |
| A-7 | pip | **BLOQUEADO** | mirror recusa certo; a tabela §Troubleshooting manda o cliente para o conselho errado |
| A-8 | CAS/AC nativo (curl + CLI) | **FALHOU** | `corelink get -o` faz **panic** 3/3; a receita de upload de diretório monta **URL sem digest** e imprime sucesso |

**2 falharam + 3 bloqueados + 0 não-aplicáveis = 5.** Fecha.

**Nenhuma recebeu `validado`, e nenhuma poderia:** as lentes *Funciona*, *Rápido*
(autenticado) e *Registrado* dependem de um PAT de tenant criado pelo funil real de
signup, que esta sessão não pode executar. Ver `BLOQUEIO-credencial-de-cliente.md`.

---

## OS ACHADOS, POR SEVERIDADE

### 🔴 Crítico — segurança

| # | achado | estação | prova mais curta |
|---|---|---|---|
| 1 | **A página do Homebrew manda o cliente exfiltrar o próprio PAT para o `ghcr.io`.** `HOMEBREW_DOCKER_REGISTRY_TOKEN` vira `Authorization: Bearer` numa requisição a **terceiro**. | A-5 | token ausente → ghcr `401`; token presente (qualquer valor) → ghcr **`403`**. Sem CoreLink env → **rc=0, funciona**. |

**I-6 violado por instrução publicada.** Mitigação imediata e sem código: remover a linha
do `HOMEBREW_DOCKER_REGISTRY_TOKEN` da página — ela comprovadamente não roteia nada para o
CoreLink (0 de 0 requisições) e só serve para vazar.

### 🔴 Crítico — o cliente não consegue usar o produto

| # | achado | estação | prova mais curta |
|---|---|---|---|
| 2 | **Nenhum PAT publicado em nenhuma das 5 páginas nem no `tutorial/02-first-pat` tem a forma que o produto parseia.** Canônico: 96 chars, 2 pontos. Publicado: 26/37/53 chars. | todas | 4 formas publicadas → `PAT format invalid`; **controle positivo**: forma canônica de 96 chars **passa** o portão e falha só no servidor (401), gravando `config.toml` `0600` |
| 3 | **`corelink get <digest> -o <file>` faz PANIC de Rust. 3/3.** Colisão do `--output` global (`text\|json`) com o `-o/--output <FILE>` do `get`. **Leva junto o modo JSON para CI.** | A-8 | `corelink get <D>` sem flag: erro limpo. Com `-o`, com `--output`, ou com `--output json` global: **panic** |
| 4 | **A receita "Upload a directory" monta a URL com digest VAZIO e imprime "Uploaded as <digest-correto>".** Sucesso silencioso publicado. | A-8 | curl stand-in recebe `…/v1/cas/acme-prod/` em zsh **e** em bash, nas duas execuções |
| 5 | **A receita do Homebrew quebra um `brew` que funcionava** (rc 0 → 1), e a página manda `export` — vale para a sessão inteira, e para sempre se for ao `~/.zshrc`. | A-5 | controle limpo rc=**0** (`✔︎ Bottle jq`) vs. receita rc=**1** |

### 🟠 Alto — diagnóstico que mente para o cliente

| # | achado | estação |
|---|---|---|
| 6 | **`corelink doctor` acusa a rede do cliente por um host que não existe.** O check `network` sonda `corelink.humangr.com` — **sem registro DNS** (controle: `corelink-api.humangr.com` resolve) — e o `NEXT_ACTION` manda auditar o firewall e o DNS **do cliente**. Falha garantida para 100% dos clientes. | A-8 |
| 7 | **`corelink doctor` rotula 401 como `COR_QUOTA_EXCEEDED`, `COR_STORAGE_WRITE_DENIED` e `COR_REGION_MISMATCH`.** Três diagnósticos falsos de uma única causa: não autenticado. "Contate o suporte" para quem só tem o PAT errado. | A-8 |
| 8 | **O link de atestação SLSA impresso pelo `corelink version` está morto** (`https://corelink.humangr.com/…` → `http=000`, falha de conexão). O `tutorial/01-installation` promete esse link. E `git rev: unknown` na release oficial. | A-8 |
| 9 | **`npm config get registry` — o passo de diagnóstico da página — erra para todo cliente do CoreLink.** 3 controles: com token → erro; **só** com `registry=` CoreLink → erro; sem `.npmrc` → funciona e imprime. A causa é o registry não-default, não o token. | A-6 |
| 10 | **A tabela §Troubleshooting do pip mapeia o sintoma de PAT inválido para a causa errada** e termina no conselho "Retry", que nunca funciona. A linha que o cliente lê por último (`Could not find a version…`) está catalogada como "upstream unreachable". | A-7 |

### 🟡 Médio — documentação e observabilidade

| # | achado | estação |
|---|---|---|
| 11 | **`corelink-docs.humangr.com/docs/<path>` devolve 404** — o redirect duplica o mount (`→ humangr.com/corelink/docs/**docs/**/…`). A URL boa é `corelink-docs.humangr.com/<path>` ou `humangr.com/corelink/docs/<path>`. | todas |
| 12 | **`Server-Timing` não existe em 5 das 6 superfícies medidas**, e a única que emite manda `server-timing: oother;dur=0` — nome com typo, `dur=0` num caminho de 195 ms. Verificado byte-a-byte, 3 amostras. **O GOAL §3.2 exige o header decomposto: hoje ele não é obtível.** | A-4 + todas |
| 13 | **Contradição de hostname entre páginas publicadas:** `oci-registry.md` diz que o registry é `corelink-api.humangr.com`; `homebrew.md` diz `corelink-oci.humangr.com`. Ambos resolvem para os mesmos IPs e ambos 401 em `/v2/`, mas nenhuma página menciona a outra. | A-4/A-5 |
| 14 | **`/v2/_catalog` devolve HTTP `401` com código OCI `DENIED`** (que é 403 no spec) e revela estado de configuração (`"catalog endpoint disabled"`) a chamador **sem credencial**. Nenhuma resposta traz `Docker-Distribution-API-Version`. | A-4 |
| 15 | **Os §"Verify it worked" de `homebrew.md` e `npm.md` terminam em `\| head`**, que mascara o exit code, e só dizem o que significa **ver** as requisições — nunca o que significa **não** ver, que é o caso real hoje. | A-5/A-6 |
| 16 | **O instalador grava o PAT com `cat >` e só depois `chmod 600`** — janela curta com a umask do usuário. `umask 077` antes resolve. | A-8 |
| 17 | **A receita de upload usa `tar -czf -`, que não é determinístico** (mtime no gzip): 4 digests distintos medidos para o mesmo conteúdo. Content-addressing disso nunca dá hit. | A-8 |

---

## O QUE FOI REFUTADO (Q-6 — refutação que falhou também é registro)

| candidato a achado | por que caiu |
|---|---|
| *"o caminho de `pip.conf` no macOS que a página dá está errado"* — `pip config debug` não o lista entre os de usuário | **Refutado com controle de HOME isolado:** o arquivo **só** no caminho da página é lido (`pip config get global.index-url` devolve o valor); movido para `~/.config/pip/pip.conf` **não** é lido. **A página está certa; o `pip config debug` é que engana.** |
| *"o 403 do ghcr.io é o PAT sendo rejeitado por valor"* | **Refutado:** token com valor arbitrário (`zzz-not-a-pat`) também dá 403. É a **presença** do header que muda o desfecho. |
| *"o 403 do ghcr.io depende do `HOMEBREW_ARTIFACT_DOMAIN`"* | **Refutado:** sem artifact domain, só com o token, ainda 403. O vazamento depende **só** da linha do token. |
| *"o PUT sem auth com digest errado devolve 422 antes de 401"* | **Refutado:** 401 em todas as variantes (digest correto, errado, não-hex). **I-3 confirmado.** |
| *"o servidor dá oráculo do formato do PAT"* | **Refutado:** 5 formas distintas → 401 byte-idêntico (só muda o `request_id`). |

---

## O QUE NÃO CONSEGUI VERIFICAR, E POR QUÊ

**Tudo o que exige um PAT de tenant criado pelo funil real de signup:**

- O artefato de cada superfície (camada OCI, `node_modules`, wheel, blob do CAS).
- Frio vs. quente **do caminho servido** — os números colados neste dossiê medem a
  **recusa de auth**, e estão todos **fora** do alvo de 15–30 ms (52–195 ms).
- **A trilha de auditoria** — nenhuma ação resolveu um tenant, e ler a trilha exige ser o
  tenant (a via de operador é proibida por §5).
- **A metade decisiva de I-1:** MALICE da RIVAL com credencial **válida** lendo dado da
  ACME; PAT revogado; PAT expirado; escalada de escopo; replay.
- `422 HashMismatch` observado de fato (só provei que **não** vem antes do 401).
- As promessas de integridade: `dist.shasum` (npm), `#sha256=` (pip), verificação
  client-side BLAKE3 do `corelink get`.
- A completude do mirror de pip — premissa que sustenta a escolha (correta) de
  `--index-url` em vez de `--extra-index-url`. Buraco de cobertura vira falha total.

**Também não feito, por decisão declarada:** não subi o daemon do Docker (este Mac é o
runner do CI, e sem PAT o `pull` terminaria no mesmo 401 do `login`); não instalei o
binário em `/usr/local/bin` via `sudo` (o binário exercitado é byte-idêntico ao publicado,
checksum colado em A-8).

---

## ITENS DE BACKLOG RECOMENDADOS — **não escritos**, aguardando o lead

O GOAL §6 manda cada falha virar item de `BACKLOG.md` com `verify`. **Não aloquei ids nem
editei o `BACKLOG.md`**, por três razões: o lead pediu explicitamente para não abrir PR de
conserto sem perguntar; ids se alocam no push e há sessões paralelas no mesmo branch; e o
`verify` de vários destes achados precisa de um PAT que ainda não existe.

Os 17 achados acima estão redigidos para virar item direto. Os `verify` que **já decidem a
própria alegação hoje, sem PAT**:

| achado | `verify` proposto |
|---|---|
| 2 (formato do PAT) | grep dos placeholders publicados vs. `PAT_*_LEN` de `crates/corelink-pat/src/format.rs` — falha se algum exemplo publicado não tiver 96 chars/2 pontos |
| 3 (panic do `get -o`) | `corelink get <digest> -o /tmp/x` — falha se o exit for por sinal/panic |
| 4 (URL sem digest) | reexecutar a pipeline da doc com stand-in de `curl`; falha se a URL terminar em `/` |
| 6 e 8 (host morto) | `dig +short corelink.humangr.com` — falha se vazio |
| 9 (`npm config get registry`) | os 3 controles da estação A-6 |
| 11 (redirect da doc) | `curl -sL -o /dev/null -w '%{http_code}' corelink-docs.humangr.com/docs/integrations/npm` — falha se 404 |
| 12 (`Server-Timing`) | `curl -sD- /v2/ \| grep -c 'oother'` — falha se ≠ 0 |
| 1 e 5 (Homebrew) | os 3 controles da estação A-5 (rc do controle limpo vs. rc da receita; 401 vs 403) |
