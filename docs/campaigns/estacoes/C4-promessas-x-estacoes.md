# C-4 — Promessa publicada × estação que a valida

> **Critério regente:** `GOAL-go-live-validation.md` §10 C-4 — *"Toda promessa
> publicada tem linha correspondente. Varra o material que o cliente lê — site,
> docs, README, material comercial — e cada capacidade prometida vira uma
> estação. Promessa sem estação é promessa não validada."*
> E C-6, o inverso: *"Feature que existe no binário e não aparece em lugar
> nenhum da documentação é achado — pode ser superfície não intencional."*

Data: 2026-08-31. Base: `origin/main` @ `0dd4a31b` (worktree limpo).
Método: **trabalho de disco** (grep/enumeração/DNS). Nada compilado, nada mergeado.

---

## 1. A POPULAÇÃO — enumerada por comando, não por memória (C-1)

### 1.1 Superfície publicada = **1148 arquivos**

Comando que produz o número (reexecutável, Q-1):

```bash
{ find apps/docs marketing legal openapi -type f \
    -not -path '*/node_modules/*' -not -path '*/.docusaurus/*' -not -path '*/build/*' \
    \( -name '*.md' -o -name '*.mdx' -o -name '*.html' -o -name '*.yaml' -o -name '*.yml' \
       -o -name '*.json' -o -name '*.txt' -o -name '*.mjml' -o -name '*.tsx' -o -name '*.ts' \)
  echo README.md
  find tools -type f \( -name '*.md' -o -name '*.rs' -o -name '*.py' -o -name '*.go' \) \
    -not -path '*/target/*' -not -path '*/node_modules/*'
} | sort -u | wc -l
# → 1148
```

Decomposição por bucket (do mesmo arquivo de lista):

| bucket | arquivos |
|---|---:|
| `apps/docs/**` | 922 |
| `tools/**` (cli, sdks, sbom-publish, openapi, dt-*) | 82 |
| `marketing/**` | 77 |
| `legal/**` | 66 |
| `openapi/**` | 2 |
| `README.md` | 1 |
| **TOTAL** | **1148** |

**Subpopulação EN (o que o cliente anglófono lê): 512 arquivos** —
`1148 − 636` de `apps/docs/i18n/{pt-BR,es-419,de}/**`. As traduções são espelho:
uma promessa falsa em EN é falsa 4×. Onde o número importa, dou **os dois**.

### 1.2 Extensões — o filtro cobre o que o cliente lê

⚠️ Esta campanha já perdeu uma varredura por confiar em `--include="*.md"`, cega
ao `.mdx`, **com controle positivo verde ao lado**. Controle prova ALCANCE, não
COBERTURA. Por isso a enumeração de extensões vem antes da conclusão:

| ext | arquivos | por que está dentro |
|---|---:|---|
| `.mdx` | 767 | **a maioria absoluta das páginas Docusaurus** — a extensão que a varredura anterior perdeu |
| `.md` | 185 | integrações, marketing, legal, README |
| `.rs` | 71 | exemplos de SDK/CLI em `tools/` que o cliente copia |
| `.ts` / `.tsx` | 54 | **páginas React de `src/pages/` — preços, trust, termos.** Não têm front-matter, nunca são `draft`, e são a superfície LIVE |
| `.json` | 17 | `openapi/corelink-v1.json`, fixtures de índice publicado |
| `.mjml` | 12 | e-mails transacionais em `legal/` |
| `.py` / `.go` | 20 | exemplos de SDK |
| `.yml` / `.yaml` | 12 | `openapi/corelink-v1.yaml` |
| `.html` | 6 | catálogo de features de marketing (497 KB) |
| `.txt` | 4 | `robots.txt`, `.well-known` |
| **soma** | **1148** | fecha com §1.1 |

### 1.3 Calibração do instrumento — controles, e um erro real que cometi

| controle | comando | esperado | obtido | veredito |
|---|---|---|---|---|
| positivo (alcance) | `grep -lFi corelink` na lista EN | ≫0 | **471 / 512** | instrumento alcança |
| negativo (falso-positivo) | `grep -rliE "zzz-impossible-token-xyzzy"` | 0 | **0** | sem ruído |
| **erro detectado** | `grep -rliE "pentest\|penetration test"` | ~45 | **0** | ❌ sob `-E`, `\|` é PIPE LITERAL. Zero por bug de sintaxe, não por ausência |

O terceiro é o registro honesto exigido por Q-2: **a primeira rodada devolveu
`0` para pentest, Object Lock, erasure, status page e audit-export
simultaneamente.** Cinco zeros num censo grande não é o mundo, é o instrumento
(*"variância zero em amostra grande é sintoma de instrumento"*). Reescrito com
alternação `-E` correta, os mesmos cinco viraram 45 / 47 / 79 / 43 / 52.
**Nenhum número abaixo vem da rodada quebrada.**

### 1.4 Onde a minha varredura NÃO chegou — dito antes de concluir (Q-7)

- O briefing definiu os exemplos de SDK como `tools/`. Existe um **`sdks/js/`
  na raiz**, fora de `tools/`, com o pacote `@corelink/client` que as 7 páginas
  de `how-to/sdk-js/` mandam instalar. Ele está **fora dos 1148**. Não invalida
  nada aqui — mas a população correta para uma auditoria de SDK é `tools/ ∪ sdks/`.
- `apps/admin-ui/**` (a UI que o cliente logado usa) está fora da população:
  não é "material que o cliente lê", é aplicação. Duas descobertas abaixo vêm
  dela mesmo assim, e estão marcadas como fora-de-população.

---

## 2. A DESCOBERTA ESTRUTURAL QUE MUDA COMO SE LÊ TUDO ABAIXO

**Nem toda página em `apps/docs/docs/` é publicada.** 72 dos 1148 arquivos
carregam `draft: true` no front-matter (18 deles em EN). Docusaurus **3.10.2**
exclui `draft` do build de produção. Isso corta em dois sentidos:

1. **Alivia:** a "tabela de preços com ✓ de Buck2 em todos os tiers"
   (`apps/docs/docs/pricing/index.mdx:45` e
   `apps/docs/docs/explanation/pricing/index.mdx`) está em página **draft**.
   O cliente não a vê. → §6, refutação parcial registrada.
2. **Agrava:** as páginas LIVE em React **linkam para páginas draft**. Um link
   para uma página excluída do build é **404**, e o `onBrokenLinks: "throw"`
   (`apps/docs/docusaurus.config.ts:148`) **não pega**, porque só valida links
   de Markdown/MDX e `<Link>` — não `<a href>` cru, que é exatamente o que
   `terms.tsx` / `trust.tsx` usam. → achado C4-16, e é a mecânica exata do
   *"três davam 404 no funil de compra, com build e type-check verdes"*.

A página de preços que o cliente realmente lê é
**`apps/docs/src/pages/pricing.tsx` (331 linhas, React, 6 tiers)** — não as duas
`.mdx` draft de 5 e 6 tiers. **Há três tabelas de preço publicadas em três
formas, com tiers e SLAs diferentes.** Todas as citações de preço abaixo são da
`.tsx`.

---

## 3. TABELA 1 — capacidades prometidas × estação (enumeração fechada: **44**)

Veredito: **✅ validada** (estação existente cobre e a promessa se sustenta no
disco) · **❌ falhou** (medida falsa) · **⚠️ não validada** (estação existe, ainda
sem dossiê — não é veredito de mérito) · **🆕 sem estação** (promessa que nenhuma
das 23 estações cobre — vira estação nova, C-7) · **N/A** com motivo.

| # | capacidade prometida | onde é prometida (`file:line`) | estação | veredito |
|---|---|---|---|---|
| C4-01 | Cache remoto Bazel (REAPI v2 por HTTP) | `apps/docs/docs/integrations/bazel.md:5` | A-1 | ⚠️ não validada |
| C4-02 | Cache remoto Turborepo (`TURBO_API`/`TURBO_TOKEN`) | `apps/docs/docs/integrations/turborepo.md:5` | A-2 | ⚠️ não validada |
| C4-03 | sccache/cargo via WebDAV | `apps/docs/docs/integrations/sccache-cargo.md:5` | A-3 | ⚠️ não validada |
| C4-04 | Registry OCI **completo, Distribution Spec v1.1** | `apps/docs/docs/integrations/oci-registry.md:5` | A-4 | ⚠️ não validada — "full spec v1.1" é claim forte; a estação tem de rodar o conformance, não `docker pull` |
| C4-05 | Espelho de bottles Homebrew | `apps/docs/docs/integrations/homebrew.md:5` | A-5 | ⚠️ não validada |
| C4-06 | Espelho npm (npm/pnpm/yarn/bun) | `apps/docs/docs/integrations/npm.md:5` | A-6 | ⚠️ não validada |
| C4-07 | Espelho pip/PyPI (pip/uv/poetry/pdm) | `apps/docs/docs/integrations/pip.md:5` | A-7 | ⚠️ não validada |
| C4-08 | CAS/AC nativo por curl | `apps/docs/docs/integrations/raw-curl.md:5` | A-8 | ⚠️ não validada |
| C4-09 | **Buck2 suportado** | `apps/docs/docs/pricing/index.mdx:45` (**draft**); 309 pos / 127 arq (1148), 170/70 (EN) | A-1 | ❌ **falhou** — o próprio código diz por quê: `worker/src/index.ts:916` *"Buck2 is NOT a client of this alias — it speaks REAPI over gRPC only"*, e CoreLink **não serve gRPC** (C4-10). `tutorial/04-buck2-quickstart.mdx` diz corretamente que não é suportado. Ver §6 pela refutação parcial |
| C4-10 | **Superfície gRPC REAPI v2** (CAS/AC/ByteStream/Capabilities) | `openapi/corelink-v1.yaml:7-9`; `apps/docs/docs/reference/reapi/_generated/{ByteStream,Capabilities,ContentAddressableStorage,Health}.mdx`; 242 pos / 80 arq (1148), 99/34 (EN) | A-1 | ❌ **falhou** — `crates/corelink-container/src/main.rs:12-17`: o servidor tonic na 50051 **foi REMOVIDO**; a porta serve HTTP/1.1. Existe uma seção inteira de referência gerada para uma API que não existe |
| C4-11 | Pants suportado | `apps/docs/docs/pricing/index.mdx:45` (**draft**); 31 arq EN | A-1 | ❌ **falhou** — zero implementação: `grep -rli pants crates/ worker/src tools/` → **0 arquivos** |
| C4-12 | **BYOK real em 4 KMS** (AWS/GCP/Azure/Vault) | `README.md:21` e `README.md:94-99`; `apps/docs/src/pages/pricing.tsx:275` (linha LIVE, Enterprise ✓); 1488 pos / 269 arq (1148), 960/148 (EN) | D-4 | ❌ **falhou** (já medido: ativação → **501**, único provider compilado é `InMemoryFake`). Número maior que os 1055/231 do briefing porque **a população é outra** — a minha inclui `marketing/`, `legal/`, `tools/` e i18n |
| C4-13 | SSO / SAML | `apps/docs/src/pages/pricing.tsx:283`; `apps/docs/src/lib/pricing.ts:246` | 🆕 **E-1** | ❌ **falhou** — `grep -rliE '\bsaml\b\|\bscim\b'` em `crates/ worker/src apps/*/src` acha **1 arquivo, e é `apps/admin-ui/src/lib/pricing.ts`** — a própria tabela de preço. Zero implementação |
| C4-14 | SCIM provisioning | `apps/docs/docs/pricing/index.mdx:51` (**draft**) — 1 pos EM TODA a superfície EN | 🆕 **E-1** | ❌ **falhou** — mesma evidência de C4-13 |
| C4-15 | **SLA 99,9% + créditos de 10%/25% aplicados AUTOMATICAMENTE na próxima fatura, para tier Pro** | `apps/docs/src/pages/legal/terms.tsx:327-344` (**Termos de Serviço LIVE — contratual**) | 🆕 **E-2** | ❌ **falhou, com contradição interna**: (a) `apps/docs/src/lib/pricing.ts:203` (bloco `pro` começa na 196) marca `slaCredits: false` para Pro e a página de preços LIVE mostra ✗ para Pro; (b) `grep -rliE 'service.credit\|sla.credit' crates/ worker/src apps/signup-worker/src` → **0 arquivos**: nenhum mecanismo de crédito existe. **Os Termos prometem um crédito contratual automático que o produto não sabe calcular nem aplicar** |
| C4-16 | Os links do rodapé legal/trust abrem | `terms.tsx` (`/explanation/sre/slo`, `/explanation/compliance/dpa`), `trust.tsx` (`/explanation/privacy/gdpr`, `/explanation/privacy/lgpd-full`) | B-1 | ❌ **falhou — 4 links 404 na superfície legal LIVE.** `/explanation/sre/slo`: o diretório `apps/docs/docs/explanation/sre/` **não existe** e nenhum arquivo declara esse slug. Os outros três **existem mas são `draft: true`** (excluídos do build) **e ainda por cima declaram outro slug**: `dpa.mdx:3` é `slug: "/compliance/dpa"`, não `/explanation/compliance/dpa`. Errados duas vezes. O portão não pega: §2 |
| C4-17 | DPA disponível | `apps/docs/src/pages/pricing.tsx:299` (✓ só Enterprise) vs `legal/dpa/**` (5 arq) + rota **obrigatória** `/v1/onboarding/dpa-accept` para todo mundo | 🆕 **E-3** | ⚠️ **não validada — promessa incoerente**: a tabela de preço vende DPA como feature Enterprise; o produto exige aceite de DPA de todo tenant. Uma das duas está errada |
| C4-18 | Exportação de audit log (Enterprise) | `apps/docs/src/pages/pricing.tsx:307`; `apps/docs/docs/how-to/export-audit-log.mdx` | D-1 | ⚠️ não validada — a rota existe (`/v1/audit/{tenant}/export`); falta dossiê |
| C4-19 | **URL publicada do export de auditoria** `GET /v1/audit/export?since=…&until=…` | `README.md:171` | D-1 | ❌ **falhou** — a rota servida é `/v1/audit/{tenant}/export` (`crates/corelink-container/src/routes/audit_export/state.rs:159`), e o próprio CLI usa a certa (`tools/cli/src/commands.rs:5`). O README publica um caminho que **não existe** |
| C4-20 | Verificação offline da cadeia (`corelink audit verify-ndjson`) | `README.md:178-186` | D-1 | ✅ **validada no disco** — subcomando existe: `tools/cli/src/commands/verify_ndjson.rs`, `verify_ndjson_http.rs`, registrado em `tools/cli/src/lib.rs:37-43`. **Refutação tentada e falhou** — ver §6 |
| C4-21 | Audit log encadeado BLAKE3 append-only, prova de inclusão | `README.md:100-106` | D-1 | ⚠️ não validada — mas ver a nota de SEV-0 do dreno em §7 |
| C4-22 | Imutabilidade do audit log / Object Lock / WORM | 169 pos / 77 arq (1148), 113/47 (EN) | D-1 | ❌ **falhou** (já medido: R2 devolve `NotImplemented`; B-046 platform-blocked) |
| C4-23 | Isolamento de tenant como invariante TLA+ | `README.md:87-92` | B-3 / I-1 | ⚠️ não validada |
| C4-24 | Integridade: todo GET é re-hasheado BLAKE3 no cliente; hash divergente recusa | `README.md:54-58`, `README.md:81-86` | B-2 / I-7 | ⚠️ não validada |
| C4-25 | Residência honesta em 4 regiões (`wnam`/`enam`/`weur`/`sam`) | `README.md:107-113` | D-2 | ⚠️ não validada |
| C4-26 | Region pinning (single/multi por tier) | `apps/docs/docs/pricing/index.mdx:48` (**draft**); 11 arq EN | D-2 | ⚠️ não validada — há `routes/residency.rs` e `/v1/customer/workspaces/{id}/pin`, mas **a promessa por tier só existe em página draft**; a página LIVE não vende region pinning |
| C4-27 | Instalação por `brew install HuGR-Labs/tap/corelink` | `README.md:34`, `README.md:124` | B-2 | ⚠️ não validada — e o mesmo README cita a org `HumanGuardrail` em `README.md:65`. **Duas orgs no mesmo arquivo**; só uma serve o tap |
| C4-28 | `corelink doctor` → 8/8 checks PASS | `README.md:40` | B-2 | ✅ **validada no disco** — `tools/cli/src/commands/doctor_cmd.rs:3` declara as 8 canônicas e `crate::doctor::run_checks` as executa. O "8/8 PASS" contra prod é da estação |
| C4-29 | `corelink put/get/ls/stat/bench/config/whoami/login/ac/cas/ci/import/export/tenant/audit/bazel-init` | `apps/docs/docs/reference/cli/*` (9 páginas) + `README.md:36-46` | B-2 | ✅ **validada no disco** — todos existem em `tools/cli/src/main.rs:64-230`. 9 páginas de referência para 18 subcomandos: **9 subcomandos sem página** (achado menor de doc, não de produto) |
| C4-30 | SDK Python (`pip install --extra-index-url …corelink-py`) | `apps/docs/docs/how-to/sdk-python/06-ci-integration.mdx:30` | 🆕 **E-4** | ⚠️ não validada — pacote existe (`tools/sdks/python`); a via de instalação publicada não |
| C4-31 | SDK Go (`GOPROXY=… go get go.corelink.humangr.com/corelink-go`) | `apps/docs/docs/how-to/sdk-go/01-authenticate.mdx:24` | 🆕 **E-4** | ⚠️ não validada — o host do module path é NXDOMAIN **por desenho** (a doc admite na linha 28 e fixa `GOPROXY`); é a única ocorrência do apex morto que **não** é achado |
| C4-32 | SDK JS (`@corelink/client`, `npm install https://humangr.com/corelink/docs/npm/corelink-client-0.1.0.tgz`) | `apps/docs/docs/how-to/sdk-js/01-authenticate.mdx:24` | 🆕 **E-4** | ⚠️ não validada — **7 páginas publicadas**; o pacote vive em `sdks/js/`, **fora da população que o briefing definiu** (§1.4) |
| C4-33 | **Status page** | `apps/docs/docs/trust/index.mdx:46` → `hugrl.betteruptime.com`; **e 29 posições EN** apontando `status.corelink.humangr.com` | 🆕 **E-5** | ❌ **falhou** — **duas URLs de status page publicadas, contraditórias**. DNS: `hugrl.betteruptime.com` RESOLVE; `status.corelink.humangr.com` **NXDOMAIN**. `apps/docs/tests/status-pill.test.ts:11` já registrava que aquele host "never resolved/handshaked" |
| C4-34 | **Hosts do produto** nos exemplos copiáveis | 19 hostnames distintos sob `*.corelink.humangr.com`, **262 posições / 123 arquivos** (1148); **158 posições excluindo o module path Go** | B-2 | ❌ **falhou** — **todo o apex pontilhado é NXDOMAIN**, confirmado por `socket.getaddrinfo`. Controle positivo na mesma execução: `corelink-api.humangr.com` e `corelink-docs.humangr.com` **RESOLVEM**. Inclui `docs./app./admin./cas./billing./attest./signup./pilot./releases./grafana./telemetry./bytestream./dt.` e os regionais `.weur.`/`.sam.` |
| C4-35 | Free forever, sem cartão | `apps/docs/src/pages/pricing.tsx:86-88`, `README.md:121` | B-1 | ⚠️ não validada |
| C4-36 | 6 tiers, preços concretos ($0/15/35/50/149/custom), anual = 2 meses grátis | `apps/docs/src/pages/pricing.tsx:109-134` + `src/lib/pricing.ts:133-252` | B-1 | ⚠️ **não validada — promessa incoerente**: três tabelas de preço publicadas (LIVE 6 tiers; `docs/pricing/index.mdx` 5 tiers com `$X`; `docs/explanation/pricing/index.mdx` 6 tiers com outros SLAs). As duas últimas são draft, mas a **incoerência de SLA vaza para os Termos** (C4-15) |
| C4-37 | Cota de storage **trava em 100%** — "no surprise bills" | `apps/docs/src/pages/pricing.tsx:180-184` | B-7 | ⚠️ não validada |
| C4-38 | Estourar a cota de storage devolve **`429 Quota Exceeded`** | `apps/docs/src/pages/pricing.tsx:200-209` (o código está na 205) | B-7 | ❌ **falhou (código diverge da doc)** — o cap de **storage** devolve **402 PAYMENT_REQUIRED**: `crates/corelink-container/src/routes/cas.rs:1737` e o teste `put_over_storage_cap_returns_402` (`cas.rs:2600-2612`). O 429 existe, mas é o medidor de **requisições** na borda (`worker/src/index.ts:4609-4622`). A página nomeia storage e cita o código do outro gate |
| C4-39 | Cota de requisições/mês por tier (declarada NÃO aplicada) | `apps/docs/src/pages/pricing.tsx:200-209` (o código está na 205) | B-7 | ✅ **validada no disco (é uma não-promessa honesta)** — a página LIVE **declara** que "cache-request-count enforcement is not live yet". Exemplo do padrão certo |
| C4-40 | SOC 2 Type I / ISO 27001 **IN-AUDIT, não certificado** | `apps/docs/src/pages/trust.tsx:222-228` | D-3 | ✅ **validada como enquadramento** — a página LIVE diz explicitamente *"We do not claim certifications we do not yet hold"*. **Refutação tentada e falhou**: §6 |
| C4-41 | **Relatório de pentest externo, anual, por Schellman / Bishop Fox, sob NDA** | `apps/docs/docs/trust/fedramp-info.mdx:63` (**LIVE, não-draft**) | 🆕 **E-6** | ❌ **falhou** — 202 pos / 73 arq (1148), 156/45 (EN); o rastreador de RFP tem **todos os fornecedores `NOT_CONTACTED`** (`marketing/launch/PENTEST-RFP-EMAIL-{BISHOP-FOX,NCC,TRAIL-OF-BITS}.md` nunca enviados). **Contradiz a própria casa**: `apps/docs/docs/trust/index.mdx:92` diz corretamente *"planned pre-GA; no report exists yet"*. Duas páginas LIVE de trust, respostas opostas |
| C4-42 | FedRAMP / HIPAA BAA / PCI DSS | `docs/trust/fedramp-info.mdx`, `docs/trust/pci-dss.mdx`, `docs/pricing/index.mdx:57` (draft); FedRAMP 268 pos/30 arq, HIPAA 89/49, PCI 15 arq EN | D-3 | ⚠️ não validada — postura, não certificação; a estação D-3 tem de ler cada página LIVE e separar "postura" de "temos" |
| C4-43 | SBOM por binário, acessível ao cliente | `docs/explanation/compliance/sbom-access.mdx` (**draft**), `tools/sbom-publish/**` (14 arq) | 🆕 **E-7** | ⚠️ não validada — e a página que explica o acesso é **draft**: a capacidade pode existir sem via publicada |
| C4-44 | Drill semanal do kill-switch | material de compliance | D-3 | ❌ **falhou** (já medido: o workflow roda e é simulação de shell que não invoca binário nenhum) |

### 3.1 A soma fecha (C-3)

| veredito | contagem | quais |
|---|---:|---|
| ✅ validada (no disco) | **5** | C4-20, 28, 29, 39, 40 |
| ❌ falhou | **15** | C4-09, 10, 11, 12, 13, 14, 15, 16, 19, 22, 33, 34, 38, 41, 44 |
| ⚠️ não validada (estação existe, dossiê ausente) | **24** | C4-01..08, 17, 18, 21, 23, 24, 25, 26, 27, 30, 31, 32, 35, 36, 37, 42, 43 |
| N/A | **0** | — |
| **soma** | **44** | **= população** ✔ |

C4-17 e C4-36 são "promessa incoerente" — ficam em ⚠️, não em ❌: a incoerência
diz que **uma** das duas versões publicadas está errada, e o disco não decide
qual. Quem decide é a estação.

**Nenhum elemento sem veredito.** Zero buracos (C-2).

⚠️ **Como ler o 5.** Cinco ✅ num total de 44 **não** quer dizer "5 promessas
funcionam". Quer dizer: cinco puderam ser **decididas por disco** e se
sustentaram. As 24 ⚠️ não são "provavelmente ok" — são **não medidas**. O único
número duro deste documento é **15 promessas publicadas medidas como falsas**.

---

## 4. TABELA 2 — o contrato de API publicado (subenumeração fechada: **48 caminhos**)

Um caminho num contrato OpenAPI publicado **é** uma promessa. Duas populações,
porque **são dois contratos publicados diferentes**:

- `openapi/corelink-v1.yaml` — **40 caminhos**
  (`grep -cE "^  /" openapi/corelink-v1.yaml` → 40), com **45 páginas de
  referência geradas** em `apps/docs/docs/reference/api/endpoints/`.
- `/openapi.json` — o que o Worker **realmente serve** ao cliente
  (`worker/src/index.ts:977` e `2055-2059`) — **8 caminhos**, todos DevEnv.

### 4.0 O achado que essa comparação revela

**O `/openapi.json` público NÃO é o `openapi/corelink-v1.yaml`.** O Worker
importa `./lib/openapi_devenv.js` e serve **só a spec de DevEnv**. Ou seja:

- as 40 promessas do contrato "oficial" **não aparecem** no contrato que o
  cliente baixa;
- as 8 promessas que o cliente baixa **não aparecem em nenhuma das 1148 páginas**
  (§5).

### 4.1 Fantasmas — publicado no contrato, com página de referência, **não servido**

Verificado contra o inventário de rotas servidas (107 literais distintos de
`.route(...)` em `crates/**`, extraídos com resolução de constantes) **e** contra
a tabela de roteamento da borda (`worker/src/index.ts:863-1178`, que **encaminha
`pathSuffix` sem reescrever**).

| caminho publicado | página de referência publicada | servido? |
|---|---|---|
| `POST /v1/dpa/accept` | `post-v1-dpa-accept.mdx` | ❌ o servido é `/v1/onboarding/dpa-accept` |
| `POST /v1/dpa/re-accept` | `post-v1-dpa-re-accept.mdx` | ❌ só existe como constante em `corelink-privacy/src/dpa/versioning/middleware.rs:64` |
| `GET/POST /v1/pats`, `DELETE /v1/pats/{pat_id}` | 3 páginas | ❌ o servido é `/v1/customer/keys`. **E a admin-ui chama o fantasma**: `apps/admin-ui/src/app/[locale]/onboarding/actions.ts:168` |
| `GET/POST /v1/admin/ops`, `/{op_id}`, `/approve`, `/reject` | 4 páginas | ❌ existe **só como mock de e2e** (`apps/admin-ui/src/lib/e2e-mock-fixtures.ts:440,472`) |
| `GET /v1/admin/audit/events` | `get-v1-admin-audit-events.mdx` | ❌ só mock + `tests/e2e-browser/specs/06-admin-console.spec.ts:42` |
| `GET /v1/admin/tenants` (coleção) | `get-v1-admin-tenants.mdx` | ❌ só o nível-item `/v1/admin/tenants/{id}/…` é servido |
| `POST /v1/enterprise/inquire` | `post-v1-enterprise-inquire.mdx` | ❌ nenhuma referência fora de `tools/openapi/src/lib.rs:145` |
| `GET /v1/data-categories` | `get-v1-data-categories.mdx` | ❌ idem, `tools/openapi/src/lib.rs:149` |
| `GET /v1/audit/export` | `get-v1-audit-export.mdx` | ❌ o servido é `/v1/audit/{tenant}/export` (= C4-19) |

**13 caminhos fantasma / 40 (32,5%), com 15 das 45 páginas de referência
publicadas** (13 caminhos ≠ 15 páginas: `/v1/pats` e `/v1/admin/ops` têm mais de
uma operação, e cada operação virou página).
Mesmíssima classe dos 18 endpoints de admin da doc RBAC já corrigidos —
**a correção não propagou para o contrato OpenAPI nem para as páginas geradas**.

⚠️ **Isto NÃO foi verificado com `scripts/validate_docs_reality.py`, de propósito.**
Aquele portão é **vacuamente verde para endpoint**: `/v1/zzz-nonexistent` resolve
`True`, porque `collect_routes()` recolhe `/{*path}`, `/{pkg}` e um `/v1` nu, e
`_route_to_regex:470` anexa `(?:/.*)?$`; além disso `endpoint.flagship_files` é
`[]`, então todo achado seria só aviso. É exatamente o portão sob o qual os 18
fantasmas anteriores foram publicados. **A evidência acima é inventário de rota,
não portão.**

### 4.2 Os 8 caminhos que o cliente realmente baixa em `/openapi.json`

`/v1/customer/devenv`, `/devenv/status`, `/devenv/stop`, `/devenv/snapshot`,
`/devenv/resize`, `/devenv/vnc`, `/devenv/tty`, `/devenv/code`
(`worker/src/lib/openapi_devenv.ts:160-268`).

**Todos os 8 devolvem 503 hoje.** O handler
(`worker/src/index.ts:2996-2999`) faz:

```
if (!env.RUNNER_DEVENV_DO) {
  return … reapiError("SERVICE_UNAVAILABLE", "RUNNER_DEVENV_DO binding not configured", 503, …)
}
```

e `RUNNER_DEVENV_DO` **não existe em nenhum `wrangler*.toml` do repo**
(`grep -rn RUNNER_DEVENV_DO --include='*.toml'` → **0 linhas**). Controle
positivo na mesma varredura: os bindings reais aparecem
(`CORELINK_SERVER`, `ROLLOUT_DO`, `EVENT_LOG_DO`, `REPLICATION_COORDINATOR_DO`,
`REQUEST_METER_COORDINATOR_DO`, `REQUEST_METER_SHARD_DO`) em `env.prod`,
`env.staging`, `env.prod-sam`, `env.prod-lhr`.

> **O único contrato de API que o CoreLink publica em produção descreve
> exclusivamente 8 endpoints que estão estruturalmente mortos em todos os
> ambientes.**

### 4.3 Soma da Tabela 2

| classe | caminhos |
|---|---:|
| publicado + servido (`corelink-v1.yaml`) | 27 † |
| publicado + **fantasma** (`corelink-v1.yaml`) | 13 |
| publicado em `/openapi.json` + servido-porém-503 | 8 |
| **soma** | **48 = população** ✔ |

† Dos 27, **três não são servidos pelo contêiner e sim pela borda ou pela
admin-ui**: `/api/health` e `/v1/signup` pelo Worker
(`worker/src/index.ts:878,972`) e `/api/csp-report` pela admin-ui
(`apps/admin-ui/src/lib/csp.ts:74`). Contei-os como servidos; **não emiti
requisição para confirmar** (§8.1).

---

## 5. TABELA 3 — o inverso (C-6): **servido e não publicado em lugar nenhum**

Medido por `grep -rlF "<caminho>"` sobre os **1148** arquivos (não só EN).
Contagem **0** significa zero ocorrências em toda a superfície publicada,
traduções incluídas.

| # | capacidade servida | onde é servida | ocorrências nos 1148 | veredito |
|---|---|---|---|---|
| I-1 | **DevEnv** — provisionar / status / stop / snapshot / resize / VNC / TTY / code (8 rotas), com gate de cota e auth PAT-ou-Clerk | `worker/src/index.ts:983` e `2954-3020` | **0** | 🆕 achado — publicado só na spec-máquina `/openapi.json`, **nenhuma página humana**, **nenhuma estação**. E morto (§4.2) |
| I-2 | **Workspaces** — listar / detalhar / **pin de região** | `crates/corelink-container/src/routes/workspaces.rs:111,115,119` | **0** | 🆕 achado — a página de preços LIVE **vende** "workspaces" como cota por tier (`pricing.tsx:172-176`) mas **nenhuma doc diz o que é nem como usar**. Promessa de cota sem superfície documentada |
| I-3 | **Runners** — entitlement / allowlist / runs | `crates/corelink-container/src/routes/customer_runners.rs:113-115` | **0** | 🆕 achado — "runner" aparece em `marketing/expansion/ci-build-acceleration.md` (brief pós-lançamento) e no catálogo HTML, mas **nunca a superfície de API**. Estação C-1 é do repo irmão; **estas 3 rotas são deste repo** |
| I-4 | **Atestado público de apagamento** `/v1/public/attestation/{request_id}` | `crates/corelink-container/src/routes/public_attestation.rs:105` | **0** | 🆕 achado — endpoint **público, sem auth**, que é justamente a prova que o GDPR Art. 17 exige o cliente poder buscar. Zero documentação de como buscá-lo. (O irmão `/v1/public/keys/erasure/{region_pub}` tem **1** ocorrência) |
| I-5 | **Alias de cache do Bazel de fábrica** `/bazel/cache/{cas,ac}/{hash}` | `crates/corelink-container/src/routes/bazel_v2.rs:338,342`; `worker/src/index.ts:922` | **0** | 🆕 achado — é a forma que `bazel --remote_cache=https://host/bazel/cache` de fato fala, **e a página `integrations/bazel.md` não a menciona**. Auth confirmada (o `_anonymous` da borda significa "defere ao PAT", `worker/src/index.ts:917-921`) — não é buraco de segurança, é superfície não documentada |
| I-6 | Enumeração de adapters: `/v2/_catalog` (OCI), `/-/v1/search` (npm) | `corelink-adapter-host/src/{oci,npm}/server.rs:42,70` | **0** | 🆕 achado menor — endpoints de **listagem** num produto multi-tenant, sem doc e sem estação que teste o que eles listam entre tenants |

**Total C-6: 6 capacidades servidas com zero cobertura publicada, somando 18 rotas.**

Contraprova de que o instrumento vê (mesma execução, mesma lista de 1148):
`/v1/customer/keys` → 20 arquivos, `/v1/privacy/dsr` → 41, `/v8/artifacts` → 28,
`/v1/audit/analytics` → 29. **Os zeros acima são zeros do mundo, não do grep.**

---

## 6. REFUTAÇÕES — o que tentei derrubar e não consegui (Q-6)

Vale tanto quanto a confirmação.

1. **"Buck2 tem ✓ na tabela de preços em todos os tiers" — refutado em parte.**
   A tabela com o ✓ é `apps/docs/docs/pricing/index.mdx:45`, e a página é
   `draft: true`; Docusaurus 3.10.2 a exclui do build de produção. **O cliente
   não vê esse ✓.** O que sobra, e sustenta o veredito ❌, são as 170 posições em
   70 arquivos EN fora da tabela draft. **Corrige-se a formulação do achado, não
   o achado.**
2. **"O `corelink audit verify-ndjson` do README não existe" — refutei minha
   própria suspeita.** Existe: `tools/cli/src/commands/verify_ndjson.rs` +
   `verify_ndjson_http.rs`, registrados em `lib.rs:37-43`. A promessa **se
   sustenta**. O que falha na mesma seção é só a URL (C4-19).
3. **"As páginas de trust exageram a conformidade" — refutado nas páginas
   principais.** `trust.tsx:222-228` diz textualmente que não reivindica
   certificação que não tem, e `trust/index.mdx:92` diz que o pentest não existe
   ainda. **O enquadramento LIVE é honesto.** O achado C4-41 sobrevive porque
   **uma terceira página LIVE** (`trust/fedramp-info.mdx:63`) contradiz as duas —
   o defeito é **incoerência entre páginas**, não overclaim generalizado.
4. **"Todo `corelink.humangr.com` publicado é achado" — refutado para 104 das
   262 posições.** `go.corelink.humangr.com/corelink-go` é **module path Go**,
   que não precisa resolver quando o `GOPROXY` está fixado, e a própria doc
   explica isso (`sdk-go/01-authenticate.mdx:28`). O achado C4-34 vale para as
   **158 restantes**.
5. **"Os adapters npm/pip/brew/cargo/OCI foram construídos mas estão
   inalcançáveis" — refutado.** `crates/corelink-container/Cargo.toml:190`
   depende de `corelink-adapter-host` e `routes.rs:117-217` monta cada adapter
   sob `/brew|/npm|/pip|/cargo`, com a borda roteando os cinco
   (`worker/src/index.ts:934-965`). **Estão montados.** A-4..A-7 seguem ⚠️ por
   falta de dossiê, não por inalcançabilidade.
6. **Refutação que NÃO consegui fazer: o `/openapi.json` servir a spec certa.**
   Procurei um segundo handler, um fallback, um build-step que substituísse
   `openapi_devenv.js` por `corelink-v1.json`. `grep -n openapi worker/src/index.ts`
   devolve **um único** ponto de serviço, linha 2059, e ele importa a spec de
   DevEnv. Não achei como salvar essa.

---

## 7. C-7 — ESTAÇÕES NOVAS

A lista era **fechada em 23**. Sete promessas publicadas não são cobertas por
nenhuma delas. Entram na lista **e** precisam de item de `BACKLOG.md` (regra
C-7: *"lista que cresce em silêncio não mede completude"*).

| id | estação nova | por que nenhuma das 23 cobre | origem |
|---|---|---|---|
| **E-1** | SSO / SAML / SCIM (Enterprise) | B-4 é convite + PAT + revogação; federação é outra superfície, com outro IdP | C4-13, C4-14 |
| **E-2** | SLA, medição de uptime e **crédito de serviço automático** | B-8 mede fatura/downgrade; crédito por SLA é obrigação contratual dos Termos, com cálculo próprio | C4-15 |
| **E-3** | Ciclo de vida do DPA (aceite obrigatório, re-aceite por versão, gating por tier) | D-3 é "hash aceito = bytes vistos"; não cobre o gate de tier nem o re-aceite | C4-17 |
| **E-4** | Instalação dos 3 SDKs (Python/Go/JS) pela via publicada | B-2 valida a instalação do **CLI**. São 21 páginas publicadas de SDK sem estação | C4-30/31/32 |
| **E-5** | Status page e comunicação de incidente ao cliente | A jornada INCIDENTE olha log e fail-closed; não olha o que o cliente vê **de fora** | C4-33 |
| **E-6** | Evidência de pentest / pacote de segurança sob NDA | D-3 é DPA/consentimento. Aqui o artefato é o relatório, e ele não existe | C4-41 |
| **E-7** | Acesso do cliente ao SBOM por binário | Nenhuma das 23 toca supply-chain do lado do cliente | C4-43 |

**Mais as 6 do inverso (§5)** — DevEnv, Workspaces, Runners-deste-repo,
Atestado público, alias `/bazel/cache/`, enumeração de adapters — que **não são
promessas** e por isso não viram estação de C-4, mas **viram item de backlog**:
ou ganham documento + estação, ou são removidas como superfície não intencional.

### 7.1 Itens de backlog necessários (ids a alocar **no push**, não agora)

1. `/openapi.json` serve só a spec DevEnv — o contrato v1 nunca chega ao cliente.
   `verify`: baixar `/openapi.json` e assertar que contém `/v1/cas/{tenant}/{hash}`.
2. `RUNNER_DEVENV_DO` sem binding — 8 endpoints publicados em 503 permanente.
   `verify`: `grep -c RUNNER_DEVENV_DO wrangler.toml` ≥ 1.
3. 13 caminhos fantasma no `corelink-v1.yaml` + 13 páginas de referência geradas.
   `verify`: diff entre os caminhos do YAML e o inventário de `.route(...)`.
4. `apps/admin-ui` chama `/v1/pats` (fantasma) em `onboarding/actions.ts:168`.
   `verify`: nenhum `fetch` da admin-ui para caminho fora do inventário de rotas.
5. Termos prometem crédito de SLA automático que não existe em código, e
   contradizem a tabela de preços. **Escalar: é decisão de owner (contratual).**
6. 4 links 404 na superfície legal/trust LIVE + `onBrokenLinks` cego a `<a href>`.
   `verify`: extrair todo `href="/…"` de `src/pages/**/*.tsx` e casar com slug
   publicado não-draft.
7. 158 posições / apex `*.corelink.humangr.com` NXDOMAIN na doc publicada.
   `verify`: resolver cada hostname distinto extraído dos 1148.
8. `status.corelink.humangr.com` vs `hugrl.betteruptime.com` — duas status pages.
9. gRPC: `openapi/corelink-v1.yaml:7-9` + 4 páginas `_generated/` descrevem uma
   API removida em `main.rs:12-17`.
10. `pricing.tsx:200-209` publica 429 para o cap de storage; o código devolve 402.
11. Pants: prometido, zero implementação.
12. 6 capacidades servidas sem nenhuma linha publicada (§5).
13. `validate_docs_reality.py` é vacuamente verde para endpoint — **o portão que
    deixou passar tudo isto**. Enquanto ele existir verde, qualquer conserto
    acima volta a apodrecer em silêncio.

**População da lista de estações passa de 23 para 30.**
`8 (A) + 9 (B) + 2 (C) + 4 (D) + 7 (E) = 30`.

---

## 8. O QUE EU **NÃO** CONSEGUI VERIFICAR, E POR QUÊ

Exigido pelo GOAL §4 ("PAREI EM") e §5 (parar com honestidade vale mais).

1. **Se as 13 rotas fantasma devolvem 404 em produção.** Provei que nenhum
   `.route(...)` do workspace as declara e que a borda encaminha `pathSuffix`
   sem reescrever. **Não emiti requisição HTTP** — este trabalho é de disco e o
   orçamento de API era explícito. A prova final é da estação, com `curl`.
2. **Se o `/openapi.json` em produção realmente é o de DevEnv.** Provei no
   código do Worker (um único handler, linha 2059). **Não baixei o artefato
   servido.** Um deploy defasado poderia divergir do `main` — improvável, mas
   não medido.
3. **Se as páginas `draft: true` estão mesmo ausentes do build de produção.**
   Baseei-me no comportamento documentado do Docusaurus 3.10.2 e no
   `package.json:34`. **Não rodei `docusaurus build`** (Node/pnpm + o Mac que é
   o runner de CI). Se algum override de config republicar drafts, o §2 muda de
   sinal: o ✓ de Buck2 e de SCIM voltam a ser visíveis ao cliente, e os 4 links
   de C4-16 deixam de ser 404 — **piorando C4-09/14 e melhorando C4-16**.
   Nenhum dos dois cenários salva as duas coisas ao mesmo tempo.
4. **Se o pacote `@corelink/client` em `sdks/js/` publica de fato o `.tgz` que a
   doc manda instalar.** `sdks/js/` ficou fora da população que o briefing
   definiu (§1.4); não estiquei o escopo sem dizer.
5. **Se a admin-ui quebra de fato ao chamar `/v1/pats`.** Achei a chamada
   (`onboarding/actions.ts:168`) e a ausência da rota. **Não executei o fluxo de
   onboarding** — e a suíte e2e não o pegaria, porque
   `e2e-mock-fixtures.ts` serve `/v1/pats` de mentira. Isso, por si só, já é um
   achado sobre a suíte.
6. **Estado real dos fornecedores de pentest.** Reusei o fato já medido nesta
   campanha (todos `NOT_CONTACTED`); **não reconferi as caixas de e-mail.**
7. **Mérito das 24 promessas ⚠️.** Elas têm estação e **não têm dossiê**.
   "⚠️ não validada" é ausência de medição, **não** veredito de que funcionam.
   Ler ⚠️ como "provavelmente ok" é exatamente o erro que C-4 existe para impedir.
8. **A trilha de auditoria de qualquer coisa acima.** C-4 é varredura de
   superfície publicada; a terceira lente (§3.3 do GOAL, "Registrado") não se
   aplica a um documento e **não foi exercida**. Não confunda este documento com
   uma estação: **ele não valida nenhuma promessa — ele enumera quais estão
   sem validação.**
