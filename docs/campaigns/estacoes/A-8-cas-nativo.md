# ESTAÇÃO A-8 — CAS/AC nativo (raw `curl` **e** o binário `corelink`)

**Página que promete:** `apps/docs/docs/integrations/raw-curl.md`
**Publicada em:** `https://humangr.com/corelink/docs/integrations/raw-curl` (200)
**Também exercitado:** `tutorial/01-installation`, `tutorial/02-first-pat` (a via
publicada de instalação e de obtenção do PAT).
**Executado em:** 2026-08-31, do Mac do owner (edge GRU). **CLI: `corelink 0.1.0`,
binário oficial `corelink-darwin-x86_64` da release `latest`, checksum verificado.**
**Contêiner medido por `GET` por `{id}`:** `corelink-prod-sam…` **version=143**, tag
**`ddd95560-r1`**.

---

## VEREDITO: **FALHOU**

Esta é a única estação em que `curl` é legítimo — e a regra dizia para rodar **também**
o CLI e comparar. Fiz isso, e é no CLI que está o estrago. **Quatro achados, dois deles
bloqueiam o cliente antes de qualquer byte trafegar**, e nenhum depende de ter PAT.

---

## ESCRITO ANTES DE MEDIR — a expectativa

1. O instalador publicado baixa o binário e **verifica o `.sha256` antes de dar `chmod +x`**.
   ✔ confirmado, e é boa engenharia.
2. `corelink login` com PAT do formato que a doc publica → aceita o formato e falha no
   servidor. **✘ REFUTADO — rejeita o formato.**
3. `corelink get <digest> -o <arquivo>` → erro limpo de auth. **✘ REFUTADO — PANIC.**
4. PUT sem auth com digest errado → **401 antes de 422** (I-3). ✔ confirmado.
5. A pipeline `tar | tee | curl` da página funciona. **✘ REFUTADO — monta a URL vazia.**

---

## FUNCIONA — a instalação sim; o uso não

### A via publicada de instalação (tutorial/01-installation)

```bash
curl -fsSL https://corelink-get.humangr.com | sh -s -- --token=<PAT>
```

O script existe (200, 3528 bytes, 100 linhas) e é sólido: normaliza `arm64→aarch64`,
recusa arquitetura desconhecida **por nome**, e **verifica o `.sha256` antes do `chmod +x`**
— um instalador `curl|sh` que se recusa a rodar sem checksum é acima da média do mercado.

Reproduzi o que ele faz, passo a passo, **sem o `sudo mv` para `/usr/local/bin`**
(declarado: não instalei no sistema; o binário exercitado é o mesmo byte-a-byte):

```bash
URL=https://github.com/HuGR-Labs/corelink-cli/releases/latest/download/corelink-darwin-x86_64
curl -fsSL "$URL" -o corelink            # 200, 4 576 460 bytes
curl -fsSL "$URL.sha256" -o corelink.sha256
```
```
publicado: 8c08cb37ced6b559eb920d955fa364565eb58c942f57ff1683dd1a1088837c37  corelink-darwin-x86_64
   real:   8c08cb37ced6b559eb920d955fa364565eb58c942f57ff1683dd1a1088837c37  corelink
CHECKSUM OK
```

Os 6 ativos que o instalador pode montar existem (**população: 6/6 → 200**):
`corelink-{darwin,linux}-{aarch64,x86_64}` + os dois `.sha256` que testei.

```
$ ./corelink version
corelink 0.1.0
  git rev:      unknown
  built:        epoch:1780091017
  target:       x86_64
  attestation:  https://corelink.humangr.com/attestations/cli/0.1.0/unknown/slsa3.json
```

**Artefato na mão: sim — um binário oficial, íntegro, que executa.** É o único artefato
que esta rodada conseguiu obter em cinco estações.

---

## 🔴 ACHADO 1 — o PAT que TODA a documentação publica é impossível de parsear

Este achado é **transversal às cinco estações** e é a razão de eu não conseguir avançar
mesmo com um PAT em mãos, se ele fosse copiado da doc.

```
$ ./corelink login --token "corelink_pat_AAAAAAAAAAAAAAAAAAAAAAAA"
error: PAT format invalid (expected `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`). Regenerate via admin UI.
```

Testei **toda** forma que a documentação publica, e as quatro foram rejeitadas:

| origem publicada | token | len | pontos | resultado |
|---|---|---|---|---|
| `oci/npm/pip/homebrew.md` | `corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXX` | 37 | 0 | **Malformed** |
| `raw-curl.md` | `corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX` | 53 | 0 | **Malformed** |
| `tutorial/02-first-pat` | `corelink_dev_t_xxx.xxx.xxx` | 26 | 2 | **Malformed** |
| idem, forma `prod` | `corelink_prod_t_abc.def.ghi` | 27 | 2 | **Malformed** |

### CONTROLE POSITIVO — o instrumento enxerga

Construí um token com a **forma canônica** (96 chars, `corelink_pat_` + 16 chars Crockford
b32 + `.` + 43 chars b64url + `.` + 22 chars b64url) e passei pelo mesmo comando:

```bash
WELL="corelink_pat_0123456789ABCDEF.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.AAAAAAAAAAAAAAAAAAAAAA"   # len=96
$ ./corelink login --token "$WELL"
error: login: token saved but whoami failed (whoami: HTTP 401 Unauthorized from /v1/users/me).
$ ls -l ~/.corelink/
-rw-------  1 gustavoschneiter  wheel  229 Aug 31 05:48 config.toml
```

**O portão de formato passou** (config gravado, modo `0600`) e a falha migrou para o
servidor. Isso prova que o rejeitador funciona e que a forma canônica é essa — não é o
comando que está quebrado, são os exemplos publicados.

### Qual das duas documentações está certa?

Nenhuma inteiramente, e a divergência é dupla:

- O **prefixo** correto é `corelink_pat_` — as páginas de integração acertam, e o
  `tutorial/02-first-pat` erra ao dizer que o segmento é o ambiente (`dev`/`staging`/`prod`).
  Os literais aceitos são `pat | ci | ro` (`crates/corelink-pat/src/format.rs`, `ENV_LITERALS`),
  e o servidor minta com `corelink_pat_` (`crates/corelink-container/src/customer_d1.rs:3287`).
- A **forma** correta é a de três segmentos com 2 pontos e 96 chars — o
  `tutorial/02-first-pat` acerta a forma e erra os valores; as **cinco páginas de
  integração erram a forma inteira** (0 pontos).

**Consequência para o cliente:** um PAT real, colado do admin UI, tem 96 chars e 2 pontos —
mas **nenhum exemplo publicado se parece com ele**. Quem truncar, quem copiar o
placeholder achando que é o formato, ou quem escrever um validador a partir da doc,
constrói algo que o produto recusa. E o servidor **não dá oráculo nenhum** (ver §Seguro):
todas as formas dão o mesmo 401 opaco, então o cliente não tem como descobrir sozinho.

---

## 🔴 ACHADO 2 — `corelink get <digest> -o <arquivo>` faz PANIC. 3 de 3.

O verbo central da página (baixar um blob) crasha com backtrace de Rust:

```
$ ./corelink get b178d2e0eb02754e1e78999c306c0450642a7b7edc2b99cfcaef68688b56bdf0 -o /tmp/a8.out

thread 'main' (13078034) panicked at clap_builder-4.6.0/src/parser/error.rs:32:9:
Mismatch between definition and access of `output`. Could not downcast to
TypeId(0x5eace4380be1ca23ccc66cd9a00f83b7), need to downcast to TypeId(0x3362b2f2b3e944096799fc5ef560a2a7)
```

**Reprodutível 3/3.** Isolei a causa por variação de uma coisa por vez:

| invocação | resultado |
|---|---|
| `corelink get <D>` (sem flag) | erro limpo (`tenant_id not set`) — **não** crasha |
| `corelink get <D> -o /tmp/x` | **PANIC** |
| `corelink get <D> --output /tmp/x` | **PANIC** |
| `corelink --output json get <D>` | **PANIC** |

**Causa:** colisão de nome entre a opção **global** `--output <FORMAT>` (`text`|`json`,
documentada como *"parsing-friendly for CI scripts"*) e a opção `-o/--output <FILE>` do
subcomando `get` (documentada em `corelink get --help` como *"Output file path"*). O clap
tenta fazer downcast do mesmo id `output` para dois tipos diferentes e aborta. Os TypeIds
aparecem **trocados** entre os dois sentidos, confirmando a colisão.

**Alcance do estrago:**
- a única forma documentada de gravar um blob em arquivo (`-o`) é inalcançável;
- **e o modo JSON, que a doc vende para scripts de CI, é inalcançável junto com `get`.**

Um cliente que siga a página bate num panic de Rust — não num erro, num crash — no
primeiro download.

---

## 🔴 ACHADO 3 — a receita "Upload a directory as an archive" monta a URL **sem digest**, e ainda imprime sucesso

A página publica:

```bash
tar -czf - ./dist/ \
  | tee >(b3sum | awk '{print $1}' > /tmp/digest.txt) \
  | curl -s -X PUT … "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$(cat /tmp/digest.txt)"
echo "Uploaded as $(cat /tmp/digest.txt)"
```

Troquei **só o `curl`** por um stand-in que imprime a URL que recebeu, e deixei o resto
idêntico:

```
### RUN 1 — primeira vez (digest.txt não existe)
curl recebeu a URL: https://corelink-api.humangr.com/v1/cas/acme-prod/
digest REAL deste tar: ba0872fb351ab7b0da2302e75f6dcde89234a8aef5944abdc61772d703a25020

### RUN 2 — com /tmp/digest.txt da RUN 1 já no disco
curl recebeu a URL: https://corelink-api.humangr.com/v1/cas/acme-prod/
digest REAL da RUN 2: 146181a839d257e331fabd7e4933afbe9f88031e1472c91a5493c96605d27122
```

Confirmado em **zsh 5.9** e em **bash** (mesma saída nos dois):

```
### bash
curl recebeu a URL: https://corelink-api.humangr.com/v1/cas/acme-prod/
Uploaded as ead2da761def4e0b4cbb2f6565589668b3e5cbfa8f52558952d3147e45c4ef6d
```

**Duas coisas erradas, e a segunda é pior:**

1. O `$(cat /tmp/digest.txt)` é expandido pelo shell **na montagem da pipeline**, no mesmo
   instante em que o `> /tmp/digest.txt` da process-substitution **trunca** o arquivo.
   O digest sempre chega **vazio**. A URL vira `/v1/cas/<tenant>/` — nunca um PUT válido.
2. A linha seguinte, `echo "Uploaded as $(cat /tmp/digest.txt)"`, roda **depois** e imprime
   um digest **correto**. O cliente lê *"Uploaded as ead2da76…"* e acredita que subiu.
   **Sucesso silencioso — a classe de defeito dominante desta casa, impressa na doc.**

E, de quebra: `tar -czf -` embute mtime no gzip, então o mesmo diretório dá digest
diferente a cada execução (medi 4 digests distintos para o mesmo conteúdo:
`43eca4a2…`, `b5202f4a…`, `049f1c19…`, `d89ab65e…`). Content-addressing um arquivo não
determinístico **nunca produz hit** — a receita, se consertada, ainda seria inútil como
cache.

---

## 🔴 ACHADO 4 — o `corelink doctor` acusa a rede do cliente por um host que não existe, e chama 401 de "cota excedida"

```
$ ./corelink doctor
CHECK              STATUS  LATENCY   ERROR_CODE                 NEXT_ACTION
network            FAIL     53ms    COR_NET_UNREACHABLE        Verify network connectivity, firewall rules, and DNS resolution for corelink.humangr.com
auth               FAIL     16ms    COR_AUTH_INVALID           Verify CORELINK_PAT env var …
storage_write      FAIL     16ms    COR_STORAGE_WRITE_DENIED   Verify tenant quota and plan limits
storage_read       FAIL      0ms    COR_STORAGE_READ_FAIL      Storage write failed; cannot verify read
byok               skip      n/a                               BYOK not configured
region             FAIL     16ms    COR_REGION_MISMATCH        Cannot determine tenant region. Verify tenant configuration in admin UI
quota              FAIL     15ms    COR_QUOTA_EXCEEDED         Cannot determine quota status. Verify plan + contact support
client_verify      ok       15ms
6 check(s) failed, 1 skipped.
```

**(a) O host `corelink.humangr.com` não existe.** Controle de DNS:

```
$ dig +short corelink.humangr.com            →  (vazio)
$ dig +short corelink-api.humangr.com        →  172.67.167.13 / 104.21.57.218   ← controle, resolve
```

O check `network` só pode falhar, para **todo** cliente, sempre. E o `NEXT_ACTION` manda o
cliente **auditar o próprio firewall e o próprio DNS** por um host que o produto nunca
publicou. O mesmo host morto aparece no link de atestação SLSA impresso pelo
`corelink version` (`https://corelink.humangr.com/attestations/…` → `http=000`,
falha de conexão, não 404) — e o `tutorial/01-installation` promete que
*"`corelink version` prints … SLSA attestation link"*. **Promessa publicada, link morto.**

**(b) Três dos seis FAIL têm código enganoso.** A causa real de `storage_write`, `region`
e `quota` é **uma só: 401**. O cliente é informado de que
`COR_QUOTA_EXCEEDED` — "verifique seu plano, contate o suporte" — quando na verdade ele
não está autenticado. O `doctor` é vendido como *"the one-shot diagnostic"*; nesta
configuração ele produz três diagnósticos falsos e um acusatório.

**(c) `git rev: unknown`** — o `corelink version` promete "build version, **git rev**, and
SLSA attestation link"; a release oficial `latest` não carrega o rev, e o link de
atestação é construído com `/unknown/` no caminho.

---

## FUNCIONA (raw `curl`) — o servidor se comporta

Os comandos da página, sem credencial válida:

| comando da página | resultado |
|---|---|
| `PUT /v1/cas/<t>/<digest-correto>` sem auth | `401` em 51,9 ms |
| `PUT /v1/cas/<t>/<digest-ERRADO>` sem auth | **`401`, não 422** — ordem correta (I-3) |
| `PUT` com PAT bogus + digest errado | `401` |
| `PUT` com digest não-hex (`zzzz`) | `401` |
| `HEAD /v1/cas/<t>/<digest>` | `401` |
| `GET /v1/users/me` | `401` |

**A ordem 401→422 é a certa** e é uma promessa da própria página (*"422 … BLAKE3 digest in
URL does not match"*): o servidor **não** gasta trabalho de validação de digest antes de
autenticar. **I-3 confirmado.** O 422 em si **não foi observado** — exige PAT.

`b3sum` está disponível (1.8.5) e a instrução da página (`brew install b3sum`) está
correta: a fórmula existe.

---

## RÁPIDO

| medido de | estado | operação | número |
|---|---|---|---|
| Mac → edge GRU | frio (1ª) | `GET /v1/cas/<t>/<d>` (401) | **53,7 ms** |
| Mac → edge GRU | quente (5) | idem | 54,3 / 61,2 / 56,5 / 54,5 / 55,1 ms |
| Mac → edge GRU | frio/quente | `GET /v1/users/me` (401) | 52,1 / 53–55 ms |
| Mac → edge GRU | — | `PUT /v1/cas/…` (401) | 51,9 ms |

**Alvo: 15–30 ms (50 ms cross-region). Veredito: FORA — ~55 ms de mediana, ~1,8× o teto de
30 ms, e acima do teto cross-region de 50 ms.** Frio e quente indistinguíveis.

**Isto mede a RECUSA de auth, não o CAS.** O `put`/`get` servido, o `Server-Timing`
decomposto que o GOAL §3.2 exige, e a comparação frio-vs-quente do blob **não foram
medidos** — exigem PAT. **Nenhuma resposta de `/v1/**` emite `Server-Timing`** (verificado
em 6 superfícies; só `/v2/` emite, e emite `oother;dur=0` — ver A-4). Ou seja: **hoje o
cliente do CAS nativo não tem como decompor a latência que a página lhe promete.**

---

## REGISTRADO

**Não verificável nesta rodada.** Nenhuma ação resolveu um tenant. Ler a trilha exige ser
o tenant (`corelink audit export --tenant …`, que existe no CLI e pede PAT) — a via de
operador é proibida por §5.

**Vazou?** Procurei, em todas as respostas e em todo o output do CLI: PAT em claro, hash
de PAT, id de tenant alheio, caminho de arquivo do servidor, versão de build, stack do
servidor. **Nada.** Os corpos são uniformes com `request_id`/`ref` correlacionáveis.

Do lado do cliente: `corelink login` grava `~/.corelink/config.toml` em **`0600`** — certo.
**Ressalva:** o instalador publicado faz `cat > "$HOME/.corelink/config.toml"` e só
**depois** `chmod 600`. Entre as duas operações o arquivo com o PAT existe com a umask do
usuário (tipicamente `0644`). Janela curta, mas é um segredo em disco legível por um
instante — vale um `umask 077` antes do `cat`.

---

## SEGURO

| ataque | resultado |
|---|---|
| `PUT`/`GET`/`HEAD` no CAS sem auth | `401` — todos |
| digest errado sem auth (tentar 422 antes de 401) | `401` — **a ordem certa** |
| digest malformado (`zzzz`) | `401` — sem oráculo de validação |
| `GET /v1/cas/acme-prod/` (listar tenant) | `401` |
| `GET /_internal/pat/mint` de fora | **`403` do Cloudflare Access** — a rota interna não é alcançável da internet |
| `GET /_health/container` | `200 {"status":"ok"}` — público, e **não vaza nada além disso** |
| **oráculo de formato de PAT** — 5 formas distintas em `/v1/users/me` | **todas 401 byte-idênticas** (só muda o `request_id`) |

**Resultado: todos recusados.** Nenhum vazamento no que consegui atacar.

O **não-oráculo de PAT** é uma escolha correta de segurança — e é exatamente ela que torna
o **Achado 1** doloroso: o servidor não pode ajudar o cliente a descobrir que o formato da
doc está errado.

**O que NÃO consegui atacar:** MALICE da RIVAL lendo blob da ACME, PAT revogado, PAT
expirado, escalada de escopo (`cache:read` escrevendo), replay. **Toda a metade
`credencial-válida-mas-errada` de I-1 está aberta.**

---

## OS 10 INVARIANTES CONTRA ESTA ESTAÇÃO

| # | veredito | base |
|---|---|---|
| I-1 isolamento | **NÃO VERIFICADO** | exige 2 PATs válidos |
| I-2 fail-closed | **OK (parcial)** | 401 em todos os caminhos de erro; `corelink login` recusa formato e não grava |
| I-3 auth antes de caro | **OK** | 401 precede 422 e 404 em todas as variantes |
| I-4 uso contado | **N/A** | 0 uso cobrável |
| I-5 mutação auditável | **NÃO VERIFICADO** | 0 mutação bem-sucedida |
| I-6 segredo nunca sai | **OK medido / ressalva** | nada vazou; janela de umask no instalador |
| I-7 integridade | **NÃO VERIFICADO** | nenhum round-trip de bytes; 422 não observado |
| I-8 idempotência | **NÃO VERIFICADO** | — |
| I-9 apagamento | **N/A** | — |
| I-10 recusa observável | **PARCIAL** | chega ao cliente com `request_id`; log não lido. **E o `doctor` rotula a recusa errado** (401 → `COR_QUOTA_EXCEEDED`) |

---

## PAREI EM

**Parei em dois lugares distintos, e vale distinguir:**

1. **Por bloqueio de credencial:** todo o caminho servido (put/get real, 422 HashMismatch,
   frio-vs-quente, trilha, cross-tenant). Ver `BLOQUEIO-credencial-de-cliente.md`.
2. **Por defeito do produto, não por bloqueio:** o Achado 2 (`corelink get -o` faz panic)
   e o Achado 3 (pipeline que monta URL vazia) me impediriam de completar a estação
   **mesmo com PAT válido em mãos**. Esses dois **não** são "não verificado" — são
   **falha**, e por eles esta estação é `FALHOU` e não `bloqueado`.

**Não instalei o binário em `/usr/local/bin` via `sudo`** — declarado, e sem efeito sobre
os achados, já que o binário exercitado é byte-idêntico ao publicado (checksum colado).
