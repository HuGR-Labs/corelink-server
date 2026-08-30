# BRIEFING DE EXECUÇÃO — ADENDO v2 (waves 2–3 de performance e disponibilidade)

**Destinatário:** agente executor (`ox alpha` / OpenRouter) + sub-agentes paralelos
**Relação com o documento anterior:** este é um **ADENDO** a
`reports/audits/2026-08-25-fix-briefing-ox-alpha.md` (15 WPs, A–M). Ele não o
substitui. Leia a §0 antes de qualquer coisa — três destes itens **colidem** com WPs
daquele documento e o procedimento de resolução é obrigatório.
**Base de verificação:** `origin/main` @ `8cd0f920` + API de produção Cloudflare, 2026-08-25.

---

## 0. LEIA PRIMEIRO — colisão com o briefing anterior

O briefing anterior distribui 15 work-packages (A, B, C, D1, D2, E, F1, F2, G, H, I,
J, K, L, M) com um mapa de conflitos onde **cada arquivo pertence a exatamente um
WP**. Três itens deste adendo caem dentro de arquivos já reservados lá:

| Item deste adendo | Arquivo | Dono no briefing anterior |
|---|---|---|
| P-S2 config do AWS SDK | `crates/corelink-container/src/storage/r2_s3.rs` | **WP-C** |
| M-X3 graceful shutdown | `crates/corelink-container/src/main.rs` | **WP-D1** |
| P-A6 cache de CryptoKey, P-A8 quad paralelo | `worker/src/index.ts` | **WP-F2** |

Além disso, **H9** e **H10** adicionam migrations, e o WP-D1 já adiciona uma — três
números de migration em voo ao mesmo tempo.

### Regra de resolução — aplique literalmente, sem julgamento

Para cada um dos três, verifique se o WP dono já foi despachado:

```bash
gh pr list --state all --search "fix-C OR fix-D1 OR fix-F2" --limit 20
git ls-remote --heads origin 'claude/fix-*'
```

- **Se o branch do WP dono NÃO existe ainda** (não despachado): o item deste adendo
  vira parte daquele WP. **Mesmo agente, mesmo branch, mesmo PR.** Estão marcados
  abaixo como `WP-C+`, `WP-D1+`, `WP-F2+`.
- **Se o branch JÁ existe** (despachado ou em voo): o item vira um WP separado que só
  começa **DEPOIS que o PR dono mergear**. Não abra um branch paralelo no mesmo
  arquivo — isso quebra o mapa de conflitos e gera conflito de merge garantido.

Os outros oito WPs deste adendo (`N1`–`N8`) **não colidem com nada** — nem com o
briefing anterior, nem entre si — e podem ser despachados imediatamente em paralelo.

### As regras do briefing anterior continuam valendo integralmente

Não vou repeti-las. Vá lê-las lá. Em resumo do que mata a tarefa: o diretório
principal do repo está numa branch em quarentena (`feat/remediation-gated-features` @
`0a5e3349`) — trabalhe **sempre** em worktree novo a partir de `origin/main`;
`git add -A` é proibido; **máximo 3 PRs em voo** porque o runner de CI é o Mac do
dono; merge só por `bash scripts/pre-merge-gate-check.sh --merge <PR>`, nunca com
pipe; todo `feat:`/`fix:` exige entrada única no CHANGELOG e trailer `Signed-off-by:`
contíguo ao `Co-Authored-By:`; e código citado pelo wiki OKF exige reconcile em commit
separado **depois** do commit de código.

---

## 1. ⛔ ITENS DO RELATÓRIO v2 QUE SÃO FALSOS — não conserte

O relatório v2 (`2026-08-25-definitive-master-report.md`) repete integralmente o
núcleo C1–C6 do relatório v1, que foi medido numa branch em quarentena 316 commits
atrás de `main`. Além daquelas refutações, as waves 2–3 trouxeram estas:

| Alegação do v2 | Verificação em `main` |
|---|---|
| **P-S3 (Tier S, ranking #3)** — npm/pip/cargo pagam Argon2id 64MiB por request; "reusar o padrão do NativePatGate" | **Já implementado, e melhor.** `adapter_pat.rs:806` `SecretMatchMemo` (TTL 300s, teto 32768) + `:1362` `verify_flights: FlightGroup<VerifyFlight>` (coalescing do burst frio) + `SingleFlightPatLookup` para a leitura D1. O doc-comment em `:1346` diz literalmente *"the adapter plane never got one. This memo closes that gap"*. Entrou pelo PR #1031; zero ocorrências na branch em quarentena, 17 em `main`. O desenho é superior ao proposto: memoiza só a comparação imutável, mantendo a leitura da linha D1 por request — revogação continua **imediata** no plano de adapters |
| **P-A5** — o comentário do worker "jura que skipa Argon2id, mente" | O comentário em `index.ts:1353-1362` é honesto e preciso. Descreve o **Worker**, explica o porquê, chama de *"the decided posture, not a TODO"*, e **já sinaliza** que o backstop container-side é TODO rastreado. O `cas.rs:800` é outro plano. Não há mentira |
| **P-A3** — `runQuotaBatch` nunca conectado, "fix de 1 linha" | Conectado em `worker/src/index.ts:3532` e `:4054` |
| **`d1_http.rs` reqwest sem timeout nenhum** | `crates/corelink-container/src/storage/d1_http.rs:107-110` já tem `connect_timeout(5s)` + `timeout(20s)` + `pool_idle_timeout(30s)` |
| **`corelink-ops` arrasta `rusqlite` bundled "pro grafo do SERVER"** | O container **não** depende de `corelink-ops` (grep vazio em `crates/corelink-container/Cargo.toml`). O custo existe, mas só em build de `--workspace`, não no binário shipado |
| **M-X4** — PagerDuty aguardado no caminho de cold-start | Zero `pagerduty` em `crates/corelink-container/src/main.rs`. Vive em `corelink-ops` e `corelink-dt-webhook`, fora do grafo do container |
| **M-X2** — `lastActivityMs` gravado "só no START", pre-destroy lê valor stale | É gravado **a cada request proxiado** (`worker/src/durable_object.ts:606`). A janela real é só a persistência (in-memory até o alarme de health), muito menor que o alegado. Baixo valor — fora deste plano |
| **Ranking #11 — "CI flips para `ubuntu-latest`, ~800 Mac-min/semana"** | A string `zero-hosted` está em **33 arquivos de workflow**, um deles citando `Per the zero-hosted-CI mandate`. É mandato do dono. E a premissa está errada: `cas-canary.yml` roda em `runs-on: corelink` (fabric do próprio produto), não no Mac. **Não toque nos runners.** |
| **P-A2** findMissingBlobs "totalmente serial" | O seam `exists_batch` já existe (`crates/corelink-bazel-bridge/src/find_missing.rs:179`). Resta só concorrência de probes — é o WP-N8, escopo bem menor que o alegado |

---

## 2. RECLASSIFICAÇÃO — dois destes NÃO são performance

O relatório v2 empacota tudo como otimização. Dois itens, se entrarem na fila de
perf, vão para o fim dela — e são os que derrubam request de cliente.

| Item | Rótulo honesto | Prioridade |
|---|---|---|
| **M-X3** graceful shutdown | **Disponibilidade.** Sem handler de `SIGTERM`, todo deploy mata conexões em voo — rajada de 502. Zero relação com latência | **P1** |
| **P-S2** metade timeout | **Disponibilidade.** Conexão R2 travada estaciona a task para sempre, e o `block_in_place` que envolve essas chamadas já tem comentário no repo dizendo *"hangs forever (observed: 60s curl timeout in prod)"*. A outra metade (checksums CRC32) é CPU/custo | **P1** |
| **H9** índice do `audit_outbox` | **Perf com prazo.** Hoje é um `SCAN` por hora numa tabela que nunca encolhe. Cresce até o drain estourar o tempo — e aí o selamento da cadeia de auditoria **para**. Desfecho é compliance | **P1** |
| P-A11, P-A6, P-A8, P-A9, P-A10, P-A1, P-A4, H10, build diet | Performance de verdade | P2 |

---

## 3. AS 11 WORK-PACKAGES DE CORREÇÃO

*(mais 5 WPs arquiteturais na §6-BIS — total 16)*

---

### WP-C+ — `P1` Config do AWS SDK: sem timeout, sem controle de checksum, sem retry

**Arquivo:** `crates/corelink-container/src/storage/r2_s3.rs`
**⚠️ Este arquivo é do WP-C do briefing anterior. Aplique a regra da §0.**

**O defeito.** A config inteira do cliente S3 é esta, e não há mais nada:

```rust
// crates/corelink-container/src/storage/r2_s3.rs:125-131
let s3_config = aws_sdk_s3::Config::builder()
    .behavior_version(BehaviorVersion::latest())
    .region(region)
    .endpoint_url(&env.r2_endpoint)
    .credentials_provider(credentials)
    .force_path_style(false)
    .build();
```

Um grep por `TimeoutConfig`, `timeout_config`, `when_required`, `WhenRequired`,
`request_checksum` e `response_checksum` no arquivo inteiro retorna **zero**. Três
consequências:

1. **CPU por byte, nos dois sentidos.** `aws-sdk-s3` está em `1.134.0` (confira em
   `Cargo.lock`), versão em que `BehaviorVersion::latest()` implica cálculo de
   checksum `when_supported` — **CRC32 sobre todo upload e validação de CRC32 sobre
   todo download**, em cima do BLAKE3 que o CAS já paga em write e read. Numa caixa
   de meia vCPU, o recurso mais escasso é queimado duas vezes por blob.
2. **Sem timeout.** Uma conexão R2 travada estaciona a task para sempre. Isso é o
   lado grave — o repo já registra o sintoma em comentário: *"hangs forever
   (observed: 60s curl timeout in prod)"*.
3. **Retry default ×3** com backoff exponencial: a cauda de latência é multiplicada
   por 3 sem que ninguém tenha escolhido isso.

**A correção — cinco linhas.**

```rust
.request_checksum_calculation(RequestChecksumCalculation::WhenRequired)
.response_checksum_validation(ResponseChecksumValidation::WhenRequired)
.retry_config(RetryConfig::standard().with_max_attempts(2))
.timeout_config(
    TimeoutConfig::builder()
        .connect_timeout(Duration::from_secs(2))
        .read_timeout(Duration::from_secs(30))
        .operation_timeout(Duration::from_secs(60))
        .build(),
)
```

**Pré-voo obrigatório.** Os nomes exatos dos tipos e dos métodos mudaram entre
versões do SDK. **Confirme contra a `1.134.0` que está no `Cargo.lock`, não contra a
sua memória** — `RequestChecksumCalculation` e `ResponseChecksumValidation` vivem em
`aws_sdk_s3::config`. Se um método não existir com esse nome, encontre o equivalente
na versão travada; não faça bump de versão.

**Riscos que você precisa avaliar antes de commitar, não depois:**
- `WhenRequired` desliga o CRC32 **opcional**. Operações que o exigem por protocolo
  continuam calculando. Confirme que nenhuma chamada nossa (multipart em especial)
  depende do checksum opcional para correção.
- A integridade não regride: o CAS re-verifica BLAKE3 na leitura
  (`r2_s3.rs:1121`) e antes do PUT (`:1316`). O CRC32 do SDK é redundante com isso.
  **Escreva essa justificativa no corpo do PR** — é o argumento que torna a mudança
  defensável.
- `read_timeout` precisa ser generoso o bastante para um GET grande. 30s é um chute
  informado; se você tiver um número medido, use o seu e diga qual é.

**Testes de aceitação:**
1. Round-trip PUT/GET contra R2 continua íntegro, com o teste existente de
   round-trip do storage passando sem edição.
2. Um endpoint que não responde falha por timeout em vez de pendurar — teste com um
   endpoint inalcançável e afirme que a chamada retorna `Err` dentro do orçamento.
3. `max_attempts(2)`: uma falha transitória ainda é retentada uma vez.

**Tipo de commit:** `fix(storage):`

---

### WP-D1+ — `P1` Lifecycle e runtime do container (4 itens, um arquivo)

**Arquivo:** `crates/corelink-container/src/main.rs`
**⚠️ É do WP-D1 do briefing anterior. Aplique a regra da §0.**

Quatro mudanças no mesmo arquivo, todas no caminho de boot/lifecycle. Agrupadas
porque a alternativa é três agentes brigando por `main.rs`. **Um commit por item.**
Os itens D1+.3 e D1+.4 vêm das DECISÕES C.1–C.3 da §6; leia-as antes.

#### D1+.1 — Graceful shutdown (M-X3)

**O defeito.** Grep por `SIGTERM`, `signal::unix`, `graceful` e
`with_graceful_shutdown` em `crates/corelink-container/src/main.rs`: **zero**. O
Cloudflare envia `SIGTERM` com 15 minutos de graça antes do `SIGKILL` num roll de
container. Como ninguém escuta, todo deploy corta conexões em voo — rajada de 502
para o cliente. A janela de graça existe e está sendo jogada fora.

**A correção.** `axum::serve(...).with_graceful_shutdown(...)` com um future que
aguarda `SIGTERM` (e `SIGINT` para dev), usando `tokio::signal::unix::signal`.
Adicione `tokio` feature `signal` se ainda não estiver.

**Detalhes que fazem a diferença entre um fix e um teatro:**
- Escolha um **deadline de drain** explícito (sugestão 30s) e force o encerramento
  depois dele. Sem deadline, uma conexão pendurada segura o processo até o `SIGKILL`
  e você não ganhou nada.
- Emita um log estruturado ao receber o sinal e outro ao terminar o drain, com a
  contagem de requests em voo. Sem isso é impossível provar em produção que
  funcionou.
- **Não** tente drenar trabalho de background que não seja request HTTP nesta
  passada. Escopo é o listener.

**Teste de aceitação:** um teste de integração que sobe o servidor, abre um request
de longa duração, envia `SIGTERM`, e afirma que (a) o request em voo **completa**, e
(b) uma conexão nova depois do sinal é recusada, e (c) o processo sai dentro do
deadline.

#### D1+.3 — Nagle ligado no listener (DECISÃO C.1)

Desabilite o Nagle no listener TCP (`set_nodelay`). Numa API request/response de
mensagens pequenas, Nagle mais delayed-ACK produz atraso fixo (~40ms clássico) sem
nenhum benefício compensatório. Não é especulativo — não existe cenário em que Nagle
ajude este formato.

Se o setup axum/hyper em uso não expuser o socket aceito, use um `Accept` customizado;
se isso ficar invasivo, **pare e reporte** em vez de reescrever o servidor por uma
linha.

**Teste:** o socket aceito tem `TCP_NODELAY`.

#### D1+.4 — Instrumentar o runtime (DECISÃO C.2/C.3) — NÃO muda comportamento

Emita, uma vez no boot (o `tracing_subscriber` já é inicializado em `main.rs:257`), um
log estruturado com: `std::thread::available_parallelism()`, a cota de CPU lida do
cgroup (`/sys/fs/cgroup/cpu.max` em v2; `cpu.cfs_quota_us`/`cpu.cfs_period_us` em v1),
a contagem de worker threads do tokio, e o pico observado de blocking threads.

**Portão de decisão, a escrever no corpo do PR:**
- Se `available_parallelism()` **já reflete** a cota do cgroup — e no Linux ele
  respeita cgroup v1 e v2 — então o achado de "tokio dimensionado pelos núcleos do
  host" está **REFUTADO**. Registre, feche, sem patch.
- Se reporta os núcleos do host: pinar `worker_threads` vira justificado e é um
  follow-up com o número medido.
- `max_blocking_threads`: só limite se o pico observado se aproximar do default de 512.

Para o allocator (mimalloc), o critério numérico fica definido **antes** de rodar:
só troque se o ganho passar de 8% num perfil de alto churn. Sem número que bata o
critério, o item morre e isso fica registrado.

**Este item existe para impedir que este plano cometa o erro que está corrigindo:**
patchar em cima de uma premissa que ninguém mediu.

**Tipo de commit:** `fix(container):` para D1+.1, `perf(container):` para D1+.3, `chore(observability):` para D1+.4

---

### WP-F2+ — `P2` Duas ineficiências no hot path do Worker

**Arquivo:** `worker/src/index.ts`
**⚠️ Este arquivo é do WP-F2 do briefing anterior. Aplique a regra da §0.**

#### F2+.1 — `importKey` por request (P-A6)

```
worker/src/index.ts:1781
const cryptoKey = await crypto.subtle.importKey("raw", keyBytes, {name:"HMAC",hash:"SHA-256"}, false, ["sign"]);
```

Toda verificação de HMAC de PAT decodifica o hex e importa a chave de novo. Grep por
cache de `CryptoKey` no arquivo: zero.

**Correção:** um `Map<string, Promise<CryptoKey>>` em escopo de módulo, chaveado pelo
hex da chave. Guarde a **Promise**, não a chave resolvida — isso coalesce importações
concorrentes sem lock. O escopo de módulo de um isolate é o tempo de vida certo: a
chave é imutável e uma rotação muda o hex, logo muda a chave do mapa.

**Cuidado:** o mapa é ilimitado por construção. Como as chaves possíveis são a
principal mais as irmãs de rotação (punhado, não ilimitado), isso é aceitável — mas
**escreva essa justificativa em comentário**, senão o próximo auditor abre um achado
de coleção sem limite.

#### F2+.2 — Cadeia de auth/quota serial (P-A8)

Em mais de 4000 linhas de `index.ts` existem **exatamente 2** `Promise.all`, ambos em
fan-out regional (`:2301`, `:2369`). A cadeia suspend → meter → tier → storage-SUM →
residency roda inteiramente em série, e só o storage-SUM realmente depende do tier.

**Correção:** `Promise.all` sobre suspend, meter, tier e residency; o storage-SUM
condicionado ao resultado do tier.

**Duas restrições que você não pode violar:**
- **Existe um teto de conexões no Worker.** Chamadas concorrentes saem em ondas de
  ~6. Quatro em paralelo cabe numa onda; não expanda para mais sem medir, ou a curva
  volta a ser linear e você não ganhou nada.
- **A ordem entre suspend e o resto é load-bearing** — existe um requisito registrado
  de que o gate de suspend precede a concessão. Paralelizar não pode permitir que uma
  operação prossiga com base num tier lido enquanto o suspend ainda estava pendente.
  Se você não conseguir preservar isso de forma óbvia, **paralelize só os três que
  são seguros e diga no PR qual ficou de fora e por quê**.

**Testes de aceitação:** número de round-trips do caminho medido cai; um tenant
suspenso continua sendo barrado antes de qualquer concessão; comportamento
byte-idêntico nos caminhos de erro.

**Tipo de commit:** `perf(worker):`

---

### WP-N1 — `P2` `stat()` do SDK JS baixa o blob inteiro para reportar o tamanho

**Arquivo exclusivo:** `sdks/js/src/client.ts`

**O defeito, e ele vem com a própria refutação embutida no repo:**

```ts
// sdks/js/src/client.ts:168-176
/**
 * Existence + size for a digest. Performs a `GET` (the container exposes no
 * lighter single-object probe), so `sizeBytes` is the exact byte length.
 */
async stat(digest: BlobDigest): Promise<StatResult> {
  const res = await this.rawRequest("GET", this.casPath(digest));
```

O doc-comment afirma que o container não expõe uma sonda mais leve. O SDK Python, na
**mesma rota**, faz exatamente isso:

```python
# sdks/python/corelink/client.py:241-250
"""``HEAD /v1/cas/{tenant}/{digest}`` (axum serves HEAD for the GET route)"""
response = self._http.head(url)
size = int(response.headers.get("content-length", "0") or "0")
```

Um `stat()` de um blob de 500MB baixa 500MB.

**Correção:** trocar por `HEAD` e ler `Content-Length`, espelhando o Python.
Preserve o mapeamento de 404/410 para `{exists:false, sizeBytes:0}` e o
comportamento de erro nos demais status. **Corrija o doc-comment** — ele é a razão
de o bug ter sobrevivido.

**Pré-voo:** confirme que `rawRequest` suporta `HEAD` sem tentar ler o corpo. Se não
suportar, ajuste-o — mas então confira se mais alguém o usa.

**Testes de aceitação:** `stat()` de um blob grande não transfere o corpo (afirme
sobre o método da request e sobre os bytes lidos); 404 e 410 continuam retornando
`exists:false`; paridade de resultado com o SDK Python para o mesmo digest.

**Tipo de commit:** `fix(sdk):`

---

### WP-N2 — `P2` Checkout: round-trips seriais no momento em que o dinheiro entra

**Arquivo exclusivo:** `crates/corelink-container/src/routes/tier_select.rs`

**O defeito.** O handler de checkout tem **15** `.await` na sua faixa principal e
**zero** `join!`, `try_join` ou `join_all` no arquivo inteiro. Cada hop é um
round-trip HTTPS ao D1 na ordem: audit → evict do lock → insert do lock → DPA →
active-sub → **Stripe (~400ms)** → audit → persist ×2 → release do lock.

**Três mudanças seguras, em ordem de risco crescente. Faça uma por commit.**

1. **Paralelizar DPA e active-sub.** São leituras independentes. `tokio::try_join!`.
   Risco baixo.
2. **`release_lock` fora do caminho de resposta.** Hoje o cliente espera por ele.
   É best-effort — o lock tem TTL de 60s e expira sozinho. Mova para depois da
   resposta e **documente explicitamente que virou best-effort**, com o TTL como
   backstop.
3. **Colapsar evict-expirados + insert do lock num único statement** (`DELETE` dos
   expirados + `INSERT ... RETURNING` numa CTE). Risco maior: é o lock que fecha o
   double-charge. **Só faça se conseguir preservar exatamente a semântica de
   aquisição**, e escreva o teste de concorrência antes.

**⛔ Não toque na ordem `audit → lock → DPA-primeiro → active-sub → Stripe`.** O
DPA-antes-de-tudo é uma invariante registrada (o `403 dpa_required` roda antes de
qualquer chamada ao Stripe, para todos os tiers). Paralelizar não pode fazer uma
chamada ao Stripe partir antes de o DPA ter respondido.

**Testes de aceitação:** contagem de round-trips no caminho feliz cai; double-checkout
concorrente continua produzindo exatamente uma cobrança; um tenant sem DPA continua
recebendo 403 **sem** que nenhuma chamada ao Stripe seja emitida (afirme sobre o
mock do Stripe, não sobre o status final).

**Tipo de commit:** `perf(billing):`

---

### WP-N3 — `P2` Sink de auditoria: dois INSERTs síncronos por operação

**Arquivo exclusivo:** `crates/corelink-container/src/storage/d1_audit_sink.rs`

**O defeito.** A trait `AuditSink::emit` é síncrona, então a escrita atravessa uma
ponte bloqueante:

```rust
// crates/corelink-container/src/storage/d1_audit_sink.rs:412-413
tokio::task::block_in_place(move || {
    tokio::runtime::Handle::current().block_on(async move { d1.query(&sql, &params).await })
})
```

Cada `emit` é um round-trip HTTPS ao D1 primário, inline no request path, e uma
operação de cache emite dois.

**Estado real, que o relatório não registrou:** o seam assíncrono **já existe e já
está parcialmente ligado** — `emit_cas_async`, `emit_cas_batch_async` e `emit_ac_async`
estão definidos aqui, e `r2_s3.rs:1184` e `:1429` já os usam para rodar a auditoria
concorrentemente com a operação R2. Você está estendendo algo existente, não criando.

**Escopo deste WP — mantenha dentro do arquivo:** implementar o INSERT multi-linha
real por trás de `emit_cas_batch_async` (uma ida de rede para N eventos em vez de N
idas), com os testes correspondentes.

**⛔ Fora de escopo, e é importante que fique fora:** ligar mais call sites de
`r2_s3.rs` ao seam assíncrono. Aquele arquivo pertence ao WP-C. Ligar mais emitters é
um follow-up para o dono do WP-C, e você deve **dizer isso no corpo do PR** para que
o trabalho não se perca.

**⛔ Não converta os emitters síncronos em fire-and-forget.** O `.map_err(...
AuditFailed)?` nos call sites é deliberado: implementa audit-before-mutation. Perder
o await enfraquece uma invariante de compliance para ganhar latência. Se você achar
que vale, é decisão do dono, não sua.

**Testes de aceitação:** N eventos em um batch produzem **um** statement e as N linhas
corretas; um batch vazio não emite statement nenhum; um erro no batch se propaga como
`AuditFailed` (fail-closed preservado).

**Tipo de commit:** `perf(audit):`

---

### WP-N4 — `P2` Contabilidade de bytes reserva no D1 por objeto, em série

**Arquivo exclusivo:** `crates/corelink-container/src/byte_accounting.rs`

**O defeito.**

```rust
// crates/corelink-container/src/byte_accounting.rs:567-574
fn block_on_accrue(...) {
    tokio::task::block_in_place(|| handle.block_on(acc.accrue(tenant, bytes, quota_seed)))
}
```

O decorator faz `reserve(len)` **antes** de cada `inner.write()`, e cada reserve é um
round-trip D1 dentro de um chokepoint único. Numa rota de batch com milhares de
objetos, são milhares de reserves em série.

**A correção — o padrão já existe no repo.** Espelhe `LeasedQuotaStore`
(`crates/corelink-container/src/tenant_quota.rs`, região ~`:435-512`): pré-reserve um
chunk durável (ordem de 64MiB), sirva as reservas seguintes da memória contra esse
lease, e reconcilie no drain.

**Restrições que a correção não pode quebrar:**
- O 402 sobre o cap tem de continuar disparando **antes** do PUT no R2. Um lease que
  concede mais do que o cap permite é um buraco de receita, não uma otimização.
- Crash com lease em aberto não pode conceder capacidade grátis permanentemente.
  Leia como o `LeasedQuotaStore` resolve isso e **copie a mesma abordagem**; não
  invente outra.
- Erro na reserva continua fail-closed.

**Leia o `LeasedQuotaStore` inteiro antes de escrever uma linha.** Este é o WP mais
sutil deste adendo: é dinheiro e é concorrência ao mesmo tempo. Se depois de ler você
achar que a semântica não transporta, **pare e reporte** — vale mais que um lease
mal-desenhado.

**Testes de aceitação:** ingest em batch faz O(chunks) round-trips, não O(objetos);
o cap continua produzindo 402 no objeto certo; crash com lease aberto não concede
capacidade além do cap depois da reconciliação; tenant sem lease disponível cai no
caminho durável.

**Tipo de commit:** `perf(quota):`

---

### WP-N5 — `P2` Linha quente: um UPSERT por request serializa o write lock do D1

**Arquivos exclusivos:** `worker/src/lib/quota.ts`, `migrations/d1/<NOVO>.sql`

**O defeito.**

```ts
// worker/src/lib/quota.ts:602-607
"INSERT INTO monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms) " +
  "VALUES (?1, ?2, 1, ?3) " +
  "ON CONFLICT(tenant_id, year_month) " +
  "DO UPDATE SET request_count = request_count + 1, updated_at_ms = ?3 " +
  "RETURNING request_count"
```

Toda request autenticada incrementa a **mesma linha** `(tenant, year_month)`. O D1 é
SQLite, que tem um único escritor — requests concorrentes do mesmo tenant enfileiram
no write lock.

**Note** que `monthlyRequestCountStatement` é compartilhado por
`incrementMonthlyRequestCount` e por `runQuotaBatch` de propósito (o doc-comment em
`:595-597` diz que é para os dois caminhos não divergirem na contagem).
**Qualquer mudança tem de preservar essa propriedade** — se você fizer um caminho
contar diferente do outro, criou um bug de billing.

**Três correções possíveis, escolha uma e justifique no PR:**
1. **Shard da linha:** `(tenant, year_month, shard)` com `shard = hash % 16`; a
   leitura soma os shards. Exige migration e ajuste em todo leitor da contagem.
2. **Agregação em memória com flush** a cada ~5s por isolate. Mais barato, mas perde
   contagem num descarte de isolate — inaceitável se a contagem for faturável.
3. **Contador em Durable Object.** Serializa por tenant em vez de por banco, mas
   adiciona um hop.

**Ponto de decisão que você precisa resolver antes de escolher:** esta contagem é
**faturável** ou é só um teto de proteção? Se for faturável, a opção 2 está fora.
Descubra lendo os consumidores da contagem e **diga no PR o que encontrou**.

Recomendação, se a leitura não for conclusiva: opção 1. É a única que não muda a
semântica de durabilidade.

**Testes de aceitação:** N incrementos concorrentes do mesmo tenant totalizam
exatamente N (sem update perdido); o caminho de `runQuotaBatch` e o de
`incrementMonthlyRequestCount` produzem contagens idênticas; a virada de mês continua
criando o bucket novo corretamente.

**Tipo de commit:** `perf(quota):`

---

### WP-N6 — `P2` SDKs não têm nenhum método batch, e o servidor tem três rotas

**Arquivos exclusivos:** `sdks/js/src/` (exceto `client.ts`, que é do WP-N1),
`sdks/python/corelink/` (exceto `client.py`, que o WP-N1 lê mas não edita)

**⚠️ Coordenação com o WP-N1:** o WP-N1 edita `sdks/js/src/client.ts`. Se os métodos
batch precisarem entrar nesse mesmo arquivo, **este WP espera o N1 mergear**. O N1 é
pequeno; não vale abrir os dois em paralelo no mesmo arquivo.

**O defeito.** Grep por `batch` em `sdks/js/src` e `sdks/python/corelink`: **zero
ocorrências**. O servidor expõe três rotas de batch, com contrato congelado:

```
crates/corelink-container/src/routes/cas.rs:100  CAS_BATCH_ROUTE       "/v1/cas/{tenant}/batch"
crates/corelink-container/src/routes/cas.rs:103  CAS_BATCH_READ_ROUTE  "/v1/cas/{tenant}/batch-read"
crates/corelink-container/src/routes/cas.rs:105  CAS_BATCH_EXISTS      "/v1/cas/{tenant}/batch-exists"
```

Toda integração custom paga round-trip por objeto onde o servidor aceita milhares por
chamada.

**Correção:** `putBatch` / `getBatch` / `existsBatch` nos dois SDKs. É plumbing — o
contrato do servidor está congelado e os testes do lado servidor já existem; leia-os
para derivar o formato exato do wire (framing por comprimento / `x-ndjson`).

**Não invente contrato.** Se algo no formato estiver ambíguo, o teste do servidor é a
fonte da verdade, não o doc-comment.

**Testes de aceitação:** round-trip de batch nos dois SDKs contra o formato do
servidor; batch vazio é no-op sem request; um item que falha no meio do batch produz
um erro que identifica **qual** item falhou; paridade de comportamento entre JS e
Python.

**Tipo de commit:** `feat(sdk):` — precisa de entrada no CHANGELOG.

---

### WP-N7 — `P2` Dieta de build

**Arquivo exclusivo:** `Cargo.toml` (raiz)

Dois itens verificados, ambos de baixo risco.

#### N7.1 — `[profile.dev] debug = true` é debug info completo

```toml
# Cargo.toml:703-705
[profile.dev]
opt-level = 0
debug = true
```

`debug = true` gera debug info completo. Para o que o desenvolvimento realmente usa —
backtraces com arquivo e linha — `debug = "line-tables-only"` basta e corta
significativamente o tempo de relink.

**Correção:** `debug = "line-tables-only"`. Se algum fluxo depender de debugging
passo-a-passo com inspeção de variáveis, isso o degrada — **diga no PR** que essa é a
troca, e como reverter localmente (override em `.cargo/config.toml` do dev).

#### N7.2 — `aws-config` compila a stack de SSO sem uso

```toml
# Cargo.toml:510
aws-config = { version = "1", default-features = false, features = ["behavior-version-latest", "rt-tokio", "credentials-process", "sso", "default-https-client"] }
```

A feature `sso` arrasta `aws-sdk-sso`, `aws-sdk-ssooidc` e `aws-sdk-sts`. O container
constrói a config do S3 **diretamente com credenciais estáticas**, contornando toda a
cadeia de provedores de propósito (o comentário em `r2_s3.rs:118-124` explica: as
sondas do IMDS custavam 60–90s de cold start).

**Pré-voo obrigatório:** prove que nada usa SSO antes de remover, com
`cargo tree -p corelink-server -i aws-sdk-sso` (`-i` = quem depende dele). Se algo
depender, **não remova** e reporte o quê.

**⛔ Não mexa em `[profile.release]`.** `lto = "fat"` é intencional para o binário
shipado e a reprodutibilidade de build depende dele.

**Teste de aceitação:** `cargo build -p corelink-server` continua verde; `cargo tree`
confirma que as crates de SSO saíram; nenhuma mudança de comportamento em runtime.

**Tipo de commit:** `chore(build):`

---

### WP-N8 — `P2` `findMissingBlobs` sem concorrência de probes

**Arquivo exclusivo:** `crates/corelink-bazel-bridge/src/find_missing.rs`

**Estado real — bem menor que o alegado.** O relatório diz "totalmente serial". Falso:
o seam de batch já existe.

```rust
// crates/corelink-bazel-bridge/src/find_missing.rs:179
if let Some(batch) = self.cas.exists_batch(&reqs) {
```

O loop serial em `:199-220` é o **fallback** para handlers que não sobrescrevem
`exists_batch`. O que falta é concorrência nos probes dentro do caminho de batch.

**Correção:** fanout com limite no adapter. O padrão já existe no repo
(`BATCH_READ_FANOUT = 16`, em `cas.rs` do container) — **reuse a mesma constante e o
mesmo formato de semáforo**, não invente um segundo padrão de concorrência.

**Restrições:**
- A **ordem da resposta tem de casar com a ordem da entrada**, e digests duplicados na
  request produzem entradas duplicadas na resposta. Isso é contrato REAPI e está
  documentado no rustdoc da função. Fanout concorrente **quebra ordem por
  construção** se você coletar por conclusão — colete por índice.
- Mantenha o fallback serial intacto para handlers sem `exists_batch`.

**Testes de aceitação:** ordem preservada com fanout ativo; duplicados preservados;
um digest com erro no meio não corrompe o resultado dos outros; o caminho de fallback
continua funcionando.

**Tipo de commit:** `perf(bazel):`

---

## 4. MAPA DE CONFLITOS

### 4.1 Propriedade de arquivos — cada arquivo em exatamente um WP

| WP | Arquivos |
|---|---|
| C+ | `crates/corelink-container/src/storage/r2_s3.rs` ⚠️ WP-C do briefing 1 |
| D1+ | `crates/corelink-container/src/main.rs` ⚠️ WP-D1 do briefing 1 |
| F2+ | `worker/src/index.ts` ⚠️ WP-F2 do briefing 1 |
| N1 | `sdks/js/src/client.ts` |
| N2 | `crates/corelink-container/src/routes/tier_select.rs` |
| N3 | `crates/corelink-container/src/storage/d1_audit_sink.rs` |
| N4 | `crates/corelink-container/src/byte_accounting.rs` |
| N5 | `worker/src/lib/quota.ts` · `migrations/d1/<novo-B>.sql` |
| N6 | `sdks/js/src/*` e `sdks/python/corelink/*` — EXCETO `sdks/js/src/client.ts`, que é do N1 |
| N7 | `Cargo.toml` |
| N8 | `crates/corelink-bazel-bridge/src/find_missing.rs` |
| S1 | `crates/corelink-container/src/routes/cas_scrub.rs` (novo) · `crates/corelink-container/src/routes.rs` |
| S2 | `crates/corelink-container/src/routes/cas.rs` · `crates/corelink-container/src/storage/r2_s3.rs` ⚠️ **último da fila nesse arquivo** (depois de C e C+) |
| S3 | `crates/corelink-container/src/routes/audit_archive.rs` |
| S6 | nenhum — só produz `reports/audits/2026-08-26-tail-verification.md` |
| S7 | `migrations/d1/<novo-A>.sql` |

Compartilhados por todos, como sempre: `CHANGELOG.md`, e `docs/internal/secrets-checklist.md`
se você adicionar env var.

### 4.2 Números de migration — coordene em tempo real

Até **três** migrations podem estar em voo simultaneamente: a do WP-D1 do briefing 1,
a `<novo-A>` do D1+, e a `<novo-B>` do N5. Já existem **duas** `0044_*` neste repo por
colisão de numeração paralela.

**Regra:** aloque o número **no momento do push**, não ao começar a escrever. Rode
`ls migrations/d1/ | tail -5` imediatamente antes de nomear o arquivo. Se dois
estiverem prontos ao mesmo tempo, pushe um, espere o merge, então numere o outro.

### 4.3 Dependências de ordem

```
WP-N1  ──►  WP-N6                     (se os métodos batch tocarem sdks/js/src/client.ts)
WP-C   ──►  WP-C+  ──►  WP-S2          (fila no r2_s3.rs; S2 é o ÚLTIMO)
WP-D1  ──►  WP-D1+                     (fila no main.rs — D1+ absorve nodelay e instrumentação)
WP-F2  ──►  WP-F2+
WP-S1  ──►  WP-S2                      (o ADR + o scrubber precedem o streaming — INEGOCIÁVEL)
```

A cadeia `C → C+ → S2` é a mais longa do plano (3 PRs em série no mesmo arquivo) e
determina o wall-clock. Comece por ela.

Nenhuma outra dependência.

---

## 5. PARALELIZAÇÃO

### A restrição é a máquina, não o código

Os runners de CI self-hosted **são o Mac pessoal do dono**. Cada push dispara ~13
workflows e custa ~US$ 0,15. Duas compilações `cargo` concorrentes na caixa já
produziram falhas de `lost-communication`. A máquina já travou por sobrecarga.

**Regra dura: no máximo 3 PRs abertos simultaneamente**, somando este adendo **e** o
briefing anterior. Se o briefing 1 já tem 3 em voo, este adendo espera.

### Escalonamento

```
FASE 1 — autoria paralela (cada agente no SEU worktree, ninguém pusha)
  Agente 1: WP-C+   config do AWS SDK          P1
  Agente 2: WP-D1+  graceful shutdown + índice P1
  Agente 3: WP-F2+  CryptoKey + quad           P2
  Agente 4: WP-N1   stat()→HEAD                P2
  Agente 5: WP-N2   checkout trims             P2
  Agente 6: WP-N3   audit batch                P2
  Agente 7: WP-N4   leased byte-accounting     P2
  Agente 8: WP-N5   hot row                    P2
  Agente 9: WP-N7   build diet                 P2
  Agente 10: WP-N8  find_missing fanout        P2
  Agente 11: WP-S1  scrubber + ADR              P1
  Agente 12: WP-S3  purga do audit_outbox       P1
  Agente 13: WP-S7  índice do sealed-tail       P1
  Agente 14+: WP-S6 varredura da cauda          P2  ← 6 sub-agentes, um por bloco

  Cada um: worktree novo em origin/main → teste VERMELHO → correção →
  gates locais → commit → PARA. Não pusha.

  WP-N6 começa a autoria depois que o N1 mergear.
  WP-S2 e WP-S4 começam a autoria depois dos seus antecessores mergearem.
  WP-S6 é leitura pura, zero conflito — dispare os 6 sub-agentes imediatamente
  e em paralelo com tudo o mais; ele não consome slot de PR até o fim.

FASE 2 — fila de push, máximo 3 em voo, prioridade primeiro
  Onda 1:  C+, D1+, N1          ← P1 de disponibilidade + o fix trivial
  Onda 2:  S1, S3, S7           ← scrubber+ADR, purga e índice: os P1 restantes
  Onda 3:  F2+, N7, N8
  Onda 4:  N2, N3, N5
  Onda 5:  N4, S6               ← S6 é doc, não consome CI pesado
  Onda 6:  N6                   ← exige N1
  Onda 7:  S2                   ← exige S1 (ADR) + C+ ; o de maior superfície

  Entre ondas: rebase em origin/main, resolver CHANGELOG mantendo AS DUAS entradas.
```

### Se um WP travar

Não bloqueia os outros. Vários destes têm ponto de escalação explícito no texto
(N4 se a semântica do lease não transportar; N5 se a contagem for faturável; N7.2 se
algo depender de SSO; C+ se os nomes dos tipos do SDK divergirem). **Entregue todos
os outros completos** e reporte o que ficou de fora e por quê. Reduzir escopo é
decisão do dono.

---

## 6. DECISÕES ARQUITETURAIS — TOMADAS (o dono delegou; execute)

O dono delegou estas quatro decisões com um mandato explícito: **caminho SOTA, sem
gambiarra, sem débito.** Elas estão decididas abaixo, com a justificativa completa,
e viraram WPs. Não são mais pontos de escalação — são trabalho.

O princípio que guiou as quatro: **uma otimização que enfraquece uma invariante não é
SOTA, é débito com juros.** Onde a otimização parecia exigir afrouxar uma garantia, a
decisão foi encontrar o desenho em que a garantia fica **mais forte**, não igual, e só
então otimizar. Duas das quatro viraram isso.

---

### DECISÃO A — Streaming do CAS: **SIM**, precedido por scrubber. Duas fases, um ADR.

**O estado.** `R2S3Client::get` (`crates/corelink-container/src/storage/r2_s3.rs:220`)
retorna `Result<Option<Vec<u8>>>` e materializa o corpo inteiro
(`:236-242` — `collect().into_bytes().to_vec()`, dupla alocação). O primeiro byte só
sai depois do download completo mais o re-hash BLAKE3 total (`:1121`). Para 500MB a
50MB/s isso é ~10s de TTFB onde ~0,3s é alcançável, e o pico de heap por GET
concorrente é o tamanho do blob — num container `instance_type = "basic"`
(`wrangler.toml:59`), a memória é o teto funcional antes da CPU.

**A objeção, e por que ela não sobrevive ao exame.** Servir-só-após-verificar é
`INV-CAS-INTEGRITY`. A leitura natural é "streaming enfraquece a integridade". Está
errada, por três razões:

1. **Num store content-addressed, o cliente já sabe o hash que pediu.** O digest *é*
   o endereço. Verificação client-side não é um extra — é o que todo cliente CAS real
   faz: Bazel REAPI verifica o digest, o docker/OCI verifica o layer digest, o sccache
   verifica, e os nossos dois SDKs podem verificar porque conhecem o hash pedido. A
   re-verificação server-side é **defesa em profundidade contra bitrot e adulteração
   no R2**, não o mecanismo primário de integridade.
2. **Um stream abortado não é uma resposta válida.** Se o hash incremental divergir no
   meio, fechamos o stream sem o chunk terminador. Em HTTP/1.1 chunked e em HTTP/2
   isso é erro de protocolo que o cliente **precisa** detectar — ele nunca aceita os
   bytes como resposta completa. É exatamente o comportamento do ByteStream do REAPI e
   da transferência de pack do Git.
3. **A cobertura de bitrot hoje é pior do que parece.** A verificação por leitura só
   cobre os blobs que alguém *leu*. Um blob escrito e nunca lido pode apodrecer no R2
   indefinidamente sem que ninguém saiba. É o inverso do que uma plataforma de
   storage-governance deveria oferecer.

**A decisão.** Duas fases, nesta ordem, **sem exceção**:

- **Fase A1 — scrubber de integridade em background (WP-S1).** Um varredor que lê e
  re-verifica blobs por amostragem contínua, independente do tráfego, emitindo evento
  de auditoria em divergência. Isso desacopla o sinal de bitrot das leituras.
- **Fase A2 — streaming com BLAKE3 incremental (WP-S2).** Só depois que A1 estiver em
  produção.

**Por que essa ordem é inegociável:** ela garante que a cobertura de integridade
**aumenta antes** de a verificação por leitura mudar. Em nenhum instante o sistema
fica com garantia menor que a de hoje. Inverter a ordem seria exatamente a gambiarra
que o mandato proíbe.

O ADR documenta a mudança de postura: de "verifica 100% do que é lido, 0% do que não
é" para "verifica 100% do que é escrito, amostra contínua de todo o corpus, e aborta
o stream em divergência". Isso é estritamente mais forte.

---

### DECISÃO B — Retenção do `audit_outbox`: **SIM**, e é MUITO menor do que eu estimei.

**Correção de fato sobre a minha própria avaliação anterior.** Eu havia registrado
isto como "trabalho de dias". Fui verificar antes de decidir e a maquinaria de
arquivamento **já existe, está montada e é dirigida por cron**:

```
crates/corelink-container/src/routes/audit_archive.rs   POST /_internal/audit/archive
  :28   "write R2 first, mark archived_at second" — ordem crash-safe
  :186  AUDIT_ARCHIVE_BATCH_LIMIT
  :395  UPDATE audit_outbox SET archived_at = ?1 WHERE id = ?2 AND archived_at IS NULL
  :715  colunas graváveis: archived_at, quarantined_at, quarantine_reason
apps/signup-worker/src/webhooks/audit_archive_cron.ts:94   o chamador
.github/workflows/audit-archive-lag.yml                    o monitor de lag
migrations/d1/0099, 0100                                   as colunas + índices
```

E, ao contrário dos crons de GC, **este funciona**: o signup-worker tem um handler
`scheduled()` de verdade (`apps/signup-worker/src/index.ts:135`).

**O que falta é uma coisa só: nada apaga as linhas já arquivadas.** Zero `DELETE FROM
audit_outbox` em todo o repositório, inclusive dentro do próprio `audit_archive.rs`.
As linhas ganham `archived_at` e ficam ali para sempre. A tabela cresce
monotonicamente mesmo com o arquivamento funcionando perfeitamente.

**A decisão: WP-S3 — purga das linhas comprovadamente arquivadas.** É seguro por
construção, e cada perna da segurança já existe:

- Os bytes estão duráveis no R2 antes de `archived_at` ser marcado (ordem R2-primeiro,
  `:28`), então uma linha com `archived_at NOT NULL` é, por invariante, recuperável.
- O resume da cadeia vem do `audit_chain_head`, que carrega hash e sequência — não das
  linhas do outbox.
- Existe um verificador diário do arquivo em R2 (`corelink_audit_chain::archive_producer`,
  citado em `audit_archive.rs:9`), então a integridade do destino é monitorada de forma
  independente.

Predicado da purga: `archived_at IS NOT NULL AND archived_at < now - GRACE`, com
`GRACE` generoso (sugestão 7 dias) e batch limitado, espelhando o padrão de
`AUDIT_ARCHIVE_BATCH_LIMIT`. Nunca apagar `quarantined_at IS NOT NULL`.

**Nota sobre a interação com o WP-D1+.2:** a purga **não** dispensa o índice. Enquanto
existir a janela de graça, e para todo o corpus não-arquivado, a query de resume
continua varrendo. Os dois entram; não são alternativas.

---

### DECISÃO C — Runtime do container: **um sim, dois medir-antes.** E "medir" virou WP, não desculpa.

Recuso tanto o patch especulativo quanto o adiamento indefinido. Os três itens têm
níveis de evidência diferentes e recebem tratamentos diferentes.

**C.1 — `set_nodelay` no listener: SIM, sem medição prévia (entra no WP-S4).**
Não é especulativo. O algoritmo de Nagle interagindo com delayed-ACK é uma patologia
conhecida e determinística em protocolos request/response de mensagens pequenas — o
sintoma clássico é um atraso fixo de ~40ms em respostas pequenas. Não existe cenário
em que Nagle ajude uma API deste formato. Custo: uma linha. Risco: nenhum.

**C.2 — Pinagem do runtime tokio: MEDIR PRIMEIRO (WP-S5), e eis o motivo honesto.**
O argumento intuitivo é que `#[tokio::main]` nu (`main.rs:255`) dimensiona o pool por
`available_parallelism()`, que veria os núcleos do **host** e não a cota do cgroup —
dezenas de threads para uma fração de núcleo. Isso derrubaria o desempenho por
thrash de scheduler.

**Só que `std::thread::available_parallelism()` no Linux respeita as cotas de CPU de
cgroup v1 e v2.** Ou seja, o tokio pode já estar dimensionado corretamente, e a
premissa do relatório é possivelmente falsa. Eu não vou patchar em cima de uma
premissa que não medi — isso é precisamente a classe de erro que os dois relatórios
de auditoria cometeram.

O WP-S5 mede: uma linha de log no boot com `available_parallelism()`, a contagem real
de worker threads e a cota lida do cgroup, mais um portão de decisão explícito. Se o
número estiver correto, o achado morre e isso fica registrado. Se estiver errado,
pinar vira trivial e justificado. Mesmo tratamento para `max_blocking_threads`, cujo
default de 512 × stacks de 2MB só é um problema se as threads forem realmente criadas.

**C.3 — Troca de allocator (mimalloc): MEDIR (parte do WP-S5), decidir depois.**
Ganho plausível em workload de alto churn, mas é o item mais dependente de perfil real
de todo este plano. Entra no mesmo harness de medição, com um critério numérico
definido **antes** de rodar. Sem número, não entra.

---

### DECISÃO D — CI: **nenhuma ação.** Confirmado, e o motivo real registrado.

O ranking #11 do relatório v2 pede flipar lanes para `ubuntu-latest`. Isso viola o
`zero-hosted-CI mandate`, escrito em 33 arquivos de workflow deste repositório, um
deles citando o mandato pelo nome. A premissa também está errada: `cas-canary.yml`
roda em `runs-on: corelink`, o fabric do próprio produto, não no Mac.

**Mas a dor que o relatório apontou é real** — o Mac do dono é o runner e está
sobrecarregado. A resposta SOTA não é comprar CI hospedado violando um mandato; é
**reduzir o trabalho desnecessário**, que já está planejado: o WP-H do briefing
anterior remove os dois crons que violam a regra do próprio repo e adiciona grupos de
concorrência aos 19 workflows sem eles. Esse é o alívio legítimo.

Registrado aqui para que nenhum agente futuro "conserte" isto.

---

### DECISÃO E — A cauda não-verificada: **converter, não descartar.**

O mandato é "sem débito". Uma lista de itens "não verificados" carregada de um
relatório para o próximo **é** débito — foi assim que o v2 herdou seis críticos
mortos do v1.

**WP-S6 — varredura de verificação.** Pega cada item que os relatórios v1/v2 alegaram
e que ninguém confirmou contra `main` — P-A7 (session de réplica não threadada), M-X1
(dois writes não-atômicos em `persist_pending_checkout`), MED-3 (pad de timing
duplicado), MED-4 (staleness de 60s em scope/find_only), o restante do Tier B, todos
os MICRO e todos os LOW das waves 2–3 — e produz, para cada um, **um dos três**:
CONFIRMADO com `arquivo:linha` em `main`, REFUTADO com a evidência, ou JÁ-CORRIGIDO
com o commit que o corrigiu.

Sem correção de código. A saída é um documento e, para os confirmados, uma proposta de
WP. Isto é deliberadamente uma tarefa de leitura: é rápida, é paralelizável ao extremo
(um agente por bloco de itens), não toca em arquivo nenhum e portanto **não colide com
nada**. É o WP que impede a próxima auditoria de começar com uma dívida herdada.

---

## 6-BIS. OS CINCO WPs QUE AS DECISÕES GERARAM

*(as decisões C.1–C.3 não viraram WP próprio — foram absorvidas pelo WP-D1+, itens .3 e .4, para não criar três donos do mesmo `main.rs`)*

### WP-S1 — `P1` Scrubber de integridade em background (pré-requisito da Fase A2)

**Arquivos exclusivos:** novo módulo `crates/corelink-container/src/routes/cas_scrub.rs`,
registro em `crates/corelink-container/src/routes.rs`

Varredura contínua por amostragem que lê blobs do R2, re-verifica o BLAKE3 contra a
chave, e emite evento de auditoria em divergência. Modelado no que já funciona neste
repo: rota `POST /_internal/cas/scrub` com auth de consumidor interno, limite de lote
por env var, cursor persistido para retomar, e chamador cron no signup-worker
(espelhe `apps/signup-worker/src/webhooks/audit_archive_cron.ts`).

**Requisitos que não são negociáveis:**
- **Cursor durável.** Um scrubber que sempre recomeça do início nunca cobre a cauda.
  Persista a posição.
- **Amostragem com cobertura demonstrável.** O objetivo é "todo blob é verificado a
  cada N dias", não "verificamos alguns". Emita a métrica de cobertura, senão você
  construiu teatro.
- **Divergência é evento de auditoria, não `panic!`.** Um blob podre não pode derrubar
  o scrubber nem a instância.
- **Orçamento de I/O limitado.** Isto compete com tráfego de cliente numa caixa
  `basic`. Lote pequeno, cadência baixa, cancelável.
- **Escrever ADR-CAS-STREAM-INTEGRITY** documentando a mudança de postura de
  integridade. O WP-S2 depende deste ADR estar mergeado.

**Testes:** blob íntegro não gera evento; blob adulterado gera exatamente um evento com
o digest; o cursor sobrevive a restart; o limite de lote é respeitado.

**Commit:** `feat(cas):` — precisa de entrada no CHANGELOG e de ADR.

---

### WP-S2 — `P1` Streaming do CAS com BLAKE3 incremental (Fase A2)

**Arquivos exclusivos:** `crates/corelink-container/src/storage/r2_s3.rs` ⚠️ **mesmo
arquivo do WP-C e do WP-C+**, `crates/corelink-container/src/routes/cas.rs`

**⛔ DEPENDE DE: WP-S1 mergeado (com o ADR) E WP-C/WP-C+ mergeados.** Este é o último
WP da fila no `r2_s3.rs`. Não comece antes; é o de maior superfície de conflito de
todo o plano.

Adicione um método de leitura em streaming ao lado do `get` atual — **não substitua o
`get`**, que tem muitos chamadores. O novo caminho: `ByteStream` do R2 → hasher BLAKE3
incremental → `axum::body::Body`, com abort do stream em divergência.

**Requisitos:**
- **Aborte sem o chunk terminador.** O cliente precisa ver truncamento, não uma
  resposta completa e curta. Teste isso explicitamente — é a perna de integridade
  inteira.
- **Evento de auditoria na divergência**, com o digest, antes do abort.
- **Piso de tamanho.** Blobs pequenos não ganham nada com streaming e o caminho atual é
  mais simples. Aplique o streaming acima de um limiar (sugestão 1MiB) e mantenha o
  caminho bufferizado abaixo dele. Menos código novo no caminho quente.
- **Reads com range** aproveitam o mesmo caminho, mas atenção: uma range read **não
  pode** verificar o hash do blob inteiro. Documente que ranges dependem da garantia do
  scrubber, não da verificação por leitura — e faça essa distinção no código, não só no
  comentário.
- Preserve BYOK: para tenant ativo o corpo é ciphertext e a decifragem precede a
  verificação (`r2_s3.rs:1104-1112`). Se o streaming não compuser com o caminho BYOK,
  **restrinja o streaming a tenants não-BYOK nesta fase e diga isso no PR** — meio
  caminho documentado é melhor que um caminho BYOK sutilmente errado.

**Testes:** TTFB de blob grande cai por ordem de grandeza; blob adulterado produz stream
truncado e evento de auditoria, e um cliente que valida rejeita; blob pequeno segue o
caminho antigo; round-trip BYOK inalterado.

**Commit:** `perf(cas):`

---

### WP-S3 — `P1` Purga das linhas já arquivadas do `audit_outbox`

**Arquivo exclusivo:** `crates/corelink-container/src/routes/audit_archive.rs`

O arquivamento funciona (R2 primeiro, `archived_at` depois, cron vivo, monitor de lag).
Falta a purga. Adicione uma fase de delete ao final do handler existente:

```sql
DELETE FROM audit_outbox
 WHERE archived_at IS NOT NULL
   AND archived_at < ?1          -- now - GRACE
   AND quarantined_at IS NULL
 LIMIT ?2
```

**Requisitos:**
- `GRACE` por env var, default 7 dias. **Registre a var na matriz de secrets**
  (`docs/internal/secrets-checklist.md`), senão o portão fica vermelho.
- **Nunca apague linhas em quarentena.** Elas são exatamente as que precisam de
  investigação humana.
- Lote limitado, reportando quantas linhas restam, espelhando o `incomplete` que o
  drain já usa.
- Emita evento de auditoria da própria purga: quantas linhas, qual janela. Uma purga
  silenciosa de dados de auditoria é o oposto de auditável.
- **Deletar depois de arquivar, dentro da mesma invocação, mas como passo separado.**
  Se a purga falhar, o arquivamento não pode reverter.

**Testes:** linha arquivada além da graça é apagada; dentro da graça não; em quarentena
nunca; a purga é idempotente; o resume da cadeia continua correto depois de uma purga
(este é o teste que importa — construa a cadeia, arquive, purgue, e prove que
`resolve_resume` ainda devolve a cabeça certa a partir do `audit_chain_head`).

**Commit:** `feat(audit):`

---

### WP-S7 — `P1` Índice do sealed-tail do `audit_outbox` (H9)

**Arquivo exclusivo:** `migrations/d1/<NOVO>.sql` — zero conflito de código

**O defeito — provadocom o query planner, não por leitura.** A query de resume do
sealed-tail é:

```sql
-- crates/corelink-container/src/routes/audit_drain.rs:869-874
SELECT sequence_number, chain_hash FROM audit_outbox
 WHERE tenant_id = ?1 AND region = ?2
   AND emitted_at IS NOT NULL AND sequence_number IS NOT NULL
 ORDER BY sequence_number DESC LIMIT 1
```

Rodada contra a DDL real com os 4 índices reais de `audit_outbox`:

```
SCAN audit_outbox
USE TEMP B-TREE FOR ORDER BY
```

Varredura completa mais ordenação, de hora em hora, por partição `(tenant, region)`,
contra uma tabela que **nunca encolhe** — não existe nenhum `DELETE FROM
audit_outbox` em todo o repositório.

**Por que os 4 índices existentes não ajudam** — e este é o ponto que o relatório não
viu. Dois são do predicado errado:

```sql
idx_audit_outbox_pending      ON (enqueued_at)                        WHERE emitted_at IS NULL
idx_audit_outbox_chain_order  ON (tenant_id,region,enqueued_at,id)    WHERE emitted_at IS NULL
```

E os dois mais novos têm as colunas certas mas um conjunto a mais no predicado:

```sql
-- migrations/d1/0099_audit_outbox_archived_at.sql:32
idx_audit_outbox_unarchived        ON (tenant_id,region,sequence_number)
  WHERE emitted_at IS NOT NULL AND archived_at IS NULL
-- migrations/d1/0100_audit_outbox_quarantine.sql:76
idx_audit_outbox_unarchived_active ON (tenant_id,region,sequence_number)
  WHERE emitted_at IS NOT NULL AND archived_at IS NULL AND quarantined_at IS NULL
```

O SQLite só usa um índice parcial quando o `WHERE` da query **implica** o predicado
do índice. A query não restringe `archived_at`, então os dois são recusados.
Comprovado nos dois sentidos: acrescentando `AND archived_at IS NULL` à query, o
plano vira `SEARCH ... USING INDEX idx_audit_outbox_unarchived`.

**A correção — índice aditivo, e NÃO mexer na query:**

```sql
CREATE INDEX IF NOT EXISTS idx_audit_outbox_sealed_tail
    ON audit_outbox(tenant_id, region, sequence_number)
    WHERE emitted_at IS NOT NULL AND sequence_number IS NOT NULL;
```

Verificado: com ele o plano vira `SEARCH audit_outbox USING INDEX`.

**⛔ Por que NÃO adicionar `AND archived_at IS NULL` à query em vez do índice**, que é
a alternativa que parece mais barata: isso **excluiria linhas arquivadas do resume do
sealed-tail**. O resume é o que impede a cadeia de auditoria de bifurcar depois de um
crash. Mudar o conjunto de linhas que ele enxerga é uma mudança de correção
disfarçada de otimização. Não faça.

**Fora de escopo, reporte:** a tabela não tem nenhum consumidor de retenção — o
crescimento é ilimitado por construção. O índice trata o sintoma. Arquivar para o
`R2_AUDIT_BUCKET` e depois deletar é trabalho maior (e é seguro, porque o
`audit_chain_head` carrega o hash e a sequência de resume), mas é decisão do dono.

**Teste de aceitação:** um teste que aplica a migration e afirma, via `EXPLAIN QUERY
PLAN`, que a query de `resolve_resume` usa `SEARCH ... USING INDEX` e não `SCAN`.
Esse é o teste — não um benchmark.

**Tipo de commit:** `perf(audit):`

---

### WP-S6 — `P2` Varredura de verificação da cauda herdada

**Arquivos exclusivos:** nenhum. Só produz
`reports/audits/2026-08-26-tail-verification.md`

Zero risco de conflito — é leitura pura. **Paralelize ao extremo:** um agente por
bloco de itens, todos simultâneos, sem esperar ninguém.

Itens a resolver, cada um com veredito contra `origin/main`:

| Bloco | Itens |
|---|---|
| 1 | P-A7 session de réplica não threadada; M-X1 dois writes não-atômicos em `persist_pending_checkout` |
| 2 | MED-3 pad de timing duplicado (`crates/corelink-worker/src/middleware/auth.rs`); MED-4 staleness de 60s em scope/find_only |
| 3 | Tier B do v2 ainda não verificado: `verifyPatHmacMulti` serial, KV gets sem `{type:"json",cacheTtl}`, Clerk fallback serial, `to_vec` extra em `bazel_v2`/`turbo_v8`, sha256 duplo no PUT do Bazel |
| 4 | Tier B DO/lifecycle: EventLogDO com 2 puts, `tmeta` em 3 chaves KV, `runner_mint` com 3 reads seriais, `customer_d1` com 4–5 RTTs, alarme de 30s do ReplicationCoordinator, ausência de escape presigned-R2 |
| 5 | Todos os MICRO do v2 e os LOW das waves 2–3 |
| 6 | A lista LOW inteira do relatório v1 (auth ×11, billing ×5, cache ×4, infra ×7, worker ×7) |

**Formato de saída, por item, sem exceção:**

```
ID | CONFIRMADO arquivo:linha | REFUTADO + evidência | JÁ-CORRIGIDO + commit
```

**Regras:** nada de "provavelmente" — se você não conseguiu decidir, o veredito é
`INDECIDÍVEL` mais o que faltou. Nenhuma alteração de código. Para cada CONFIRMADO,
proponha o WP (arquivo, correção, teste) sem implementá-lo.

**Commit:** `docs(audit):`

---

## 7. TABELA-RESUMO

| WP | P | Título | Arquivos | Depende de |
|---|---|---|---|---|
| C+ | P1 | Timeouts + checksums + retry no AWS SDK | 1 | WP-C (fold ou after) |
| D1+ | P1 | Lifecycle+runtime do container (shutdown, nodelay, instrumentação) | 1 | WP-D1 (fold ou after) |
| F2+ | P2 | Cache de CryptoKey + quad paralelo | 1 | WP-F2 (fold ou after) |
| N1 | P2 | `stat()` → HEAD no SDK JS | 1 | — |
| N2 | P2 | Trims no checkout | 1 | — |
| N3 | P2 | INSERT multi-linha no sink de auditoria | 1 | — |
| N4 | P2 | Byte-accounting com lease | 1 | — |
| N5 | P2 | Linha quente do contador mensal | 2 | — |
| N6 | P2 | Métodos batch nos SDKs | vários | **N1** |
| N7 | P2 | Dieta de build | 1 | — |
| N8 | P2 | Concorrência de probes no findMissing | 1 | — |
| S1 | P1 | Scrubber de integridade + ADR | 2 | — |
| S2 | P1 | Streaming do CAS com BLAKE3 incremental | 2 | **S1 + C+** |
| S3 | P1 | Purga das linhas arquivadas | 1 | — |
| S6 | P2 | Varredura da cauda herdada | 0 | — |
| S7 | P1 | Índice do sealed-tail do `audit_outbox` | 1 | — |

**Definição de pronto:** os 16 PRs mergeados via `pre-merge-gate-check.sh --merge`,
os cinco portões verdes em `main` (`validate_specs` 484/0, `validate_secrets_matrix`
`code_only=0`, `secrets-checklist-verify` sem drift, `backlog_verify` sem
DRIFTED/STALE, `validate_okf` verde), e um relatório nomeando cada item da §6 que
continua aberto.

---

## 8. RASTREABILIDADE

| Achado do v2 | Destino |
|---|---|
| P-S1 zero streaming | **WP-S1 + WP-S2** (decidido: scrubber primeiro, depois streaming) |
| P-S2 config do AWS SDK | **WP-C+** |
| P-S3 Argon2id nos adapters | §1 REFUTADO — já em `main` desde o PR #1031 |
| P-A1 sink de auditoria síncrono | **WP-N3** (parcialmente já endereçado pelo seam async) |
| P-A2 findMissing serial | **WP-N8** (escopo reduzido — o seam de batch já existe) |
| P-A3 `runQuotaBatch` | §1 REFUTADO — conectado |
| P-A4 byte-accounting | **WP-N4** |
| P-A5 tombstone + Argon2id no GET | §1 metade REFUTADA (o comentário é honesto); a outra metade é amortizada pelo cache do NativePatGate — fora de escopo |
| P-A6 CryptoKey | **WP-F2+** |
| P-A7 session de réplica não threadada | **WP-S6 bloco 1** |
| P-A8 quad serial | **WP-F2+** |
| P-A9 checkout serial | **WP-N2** |
| P-A10 SDKs sem batch | **WP-N6** |
| P-A11 `stat()` baixa o blob | **WP-N1** |
| H9 índice do `audit_outbox` | **WP-S7** (índice) + **WP-S3** (purga) |
| H10 linha quente | **WP-N5** |
| M-X1 D1 dois writes não-atômicos | **WP-S6 bloco 1** |
| M-X2 reaper lê valor stale | §1 REFUTADO em magnitude |
| M-X3 sem graceful shutdown | **WP-D1+.1** |
| M-X4 PagerDuty no cold start | §1 REFUTADO — fora do grafo do container |
| Tier B `[profile.dev]`, `aws-config sso` | **WP-N7** |
| Tier B nodelay | **WP-D1+.3** (decidido: entra sem medição) |
| Tier B tokio/allocator | **WP-D1+.4** (decidido: medir com portão de decisão) |
| Tier B `d1_http` sem timeout | §1 REFUTADO — já tem |
| Tier B `corelink-ops`/rusqlite | §1 REFUTADO para o binário do servidor |
| CI → `ubuntu-latest` | §6 DECISÃO D — nenhuma ação; alívio real já é o WP-H do briefing 1 |
| Tier B restante, MICRO, LOW das waves 2–3 | **WP-S6 blocos 3–6** |

### Nada fica como "não verificado"

O mandato é sem débito. Toda a cauda que os relatórios v1/v2 alegaram e que ninguém
confirmou contra `main` está atribuída ao **WP-S6**, que produz para cada item um
veredito de três valores — CONFIRMADO com `arquivo:linha`, REFUTADO com evidência, ou
JÁ-CORRIGIDO com o commit. Nenhum item sai desta campanha com o rótulo "não
verificado": foi carregar essa cauda de um relatório para o outro que fez o v2 herdar
seis críticos mortos do v1.

Itens que este adendo REFUTOU (§1) não vão para o WP-S6 — já estão decididos.

*Verificado contra `origin/main` @ `8cd0f920` e contra a API de produção da Cloudflare
em 2026-08-25. O plano de query do WP-D1+.2 foi obtido executando `EXPLAIN QUERY PLAN`
contra a DDL real de `audit_outbox` com os quatro índices reais; os três planos
(atual, com o conjunto extra, e com o índice novo) estão transcritos na seção do WP.*
