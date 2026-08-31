# ESTAÇÃO A-3 — sccache / cargo

**Página que promete:** `apps/docs/docs/integrations/sccache-cargo.md` (publicada)
**Executado em:** 2026-08-31 · **Cliente real:** `sccache 0.15.0` + `cargo 1.91.1` (não `curl`)
**Medido de:** Mac do fundador, loopback (controle) — produção não exercida

---

## VEREDITO: **NÃO VALIDADO CONTRA PRODUÇÃO** (bloqueio de credencial) · doc correta no essencial, **incompleta no contrato de verbos**

A receita de verificação da página funciona ponta a ponta: cold → apaga camada local
→ warm entrega **hit vindo do remoto**. Mas a página descreve o contrato HTTP como
"GET, PUT e HEAD", e o cliente real também exige **PROPFIND** e **MKCOL** — e quando
esses falham o sccache **degrada em silêncio para read-only**, com o build passando
verde e o cache ficando vazio para sempre.

---

## FUNCIONA

### A receita de verificação da página, executada como está escrita

A página (sccache-cargo.md:92-100) manda exatamente isto:

```bash
cargo clean; cargo build --release      # cold
cargo clean; sccache --stop-server; rm -rf "$SCCACHE_DIR"
cargo build --release                   # warm — o hit só pode vir do CoreLink
sccache --show-stats
```

Executado contra o servidor de controle:

```
### COLD ###                          ### WARM (camadas locais apagadas) ###
Cache hits      0                     Cache hits       1
Cache misses    1                     Cache misses     0
Cache write errors 0                  Cache write errors 0
```

**`Cache hits 1` com `SCCACHE_DIR` apagado ⇒ o artefato veio do backend remoto.**
É exatamente o critério que a página define ("Non-zero Cache hits on the second
build, with the local layer emptied, confirms CoreLink served the artifacts"), e ele
se sustenta.

### `SCCACHE_MULTILEVEL_CHAIN` é real — refutação que falhou (Q-6)

A página apoia tudo numa variável pouco conhecida e afirma que ela existe desde a
0.15.0. **Tentei derrubar essa afirmação** — seria um achado grande se a doc
estivesse inventando a feature. Não estava:

```
comando : strings $(command -v sccache) | grep -c MULTILEVEL
saída   : 6
          SCCACHE_MULTILEVEL_CHAIN / SCCACHE_MULTILEVEL_WRITE_ERROR_POLICY
          src/cache/multilevel.rs · "Multi-level cache levels"
controle: strings … | grep -c 'SCCACHE_WEBDAV_'  → 1  (prova que o instrumento enxerga)
```

E em execução: `sccache --show-stats` → `Cache location  Multi-level (2 levels)`.
**A promessa da página se confirma.**

### O contrato de protocolo real — 22 requisições capturadas

```
GET      /cargo/<tenant>/.sccache_check        -> 404
PROPFIND /cargo/<tenant>/                      -> 207
MKCOL    /cargo/<tenant>/                      -> 201
PUT      /cargo/<tenant>/.sccache_check        -> 201     <-- sonda de escrita
GET      /cargo/<tenant>/f/1/a/<64hex>         -> 404     (miss)
PROPFIND /cargo/<tenant>/f/1/a/  …  /f/1/  …  /f/   -> 207
MKCOL    /cargo/<tenant>/f/  …  /f/1/  …  /f/1/a/   -> 201
PUT      /cargo/<tenant>/f/1/a/<64hex>         -> 201
GET      /cargo/<tenant>/f/1/a/<64hex>         -> 200     (hit)
```

**População: 22 requisições · 4 verbos (GET, PUT, PROPFIND, MKCOL) · Bearer em 22/22.**

Dois fatos que a página não conta:

- **O caminho da chave é fatiado:** `<endpoint>/f/<a>/<b>/<64hex>`, não
  `<endpoint>/<key>` como diz sccache-cargo.md:73-76.
- **`PROPFIND` e `MKCOL` são obrigatórios** — o sccache cria a hierarquia de
  diretórios WebDAV antes de cada escrita.

### O achado sério: degradação silenciosa para read-only

Antes de o controle implementar WebDAV completo, ele respondia **501** ao verbo que
o sccache usa para sondar escrita. O resultado, no log de debug do próprio sccache:

```
storage write check failed: Unexpected (permanent) at write => Error code: 501
service: webdav
storage check result: ReadOnly
Cache level 1 is read-only
Error executing cache write: Cannot write to read-only storage
```

E o que o **usuário** viu na mesma execução:

```
    Finished `release` profile [optimized] target(s) in 0.89s
```

**Build verde. Nenhum aviso. Cache permanentemente vazio.** O sccache faz a sonda
uma única vez, no start do servidor, e marca o backend read-only para toda a vida do
daemon. `sccache --show-stats` mostrava `Cache write errors 1` — um número que
ninguém olha num CI verde.

**Isto é a classe de defeito dominante desta casa (sucesso silencioso):** o cliente
paga pelo cache compartilhado, o build passa, e nada é compartilhado. Se
`/cargo/<tenant>` em produção não implementar `PROPFIND` + `MKCOL` + a sonda
`.sccache_check`, é assim que a falha se apresenta.

**Honestidade sobre a origem do 501: foi o MEU controle, não a produção.** Não
afirmo que o CoreLink devolve 501. O que a captura estabelece é o **contrato que a
produção precisa cumprir** e **o modo de falha caso não cumpra** — e que esse modo de
falha é invisível.

---

## RÁPIDO

**Números de produção: não existem.** Sem PAT, nenhuma requisição autenticada foi a
`corelink-api.humangr.com`. Não colo tempos de loopback como se fossem latência do
produto; mediriam a minha máquina.

**`Server-Timing` decomposto: não coletado** (exige produção).
**Versão do contêiner por `GET` por `{id}` na Containers API: não coletada** — sem
número de produção para parear, registrá-la só simularia medição.

---

## REGISTRADO

**Não verificado.** Nenhuma ação autenticada ocorreu. Lente **em aberto**.

---

## SEGURO

| ataque (sem credencial) | saída | veredito |
|---|---|---|
| `GET /cargo/acme/f/1/a/x` | `401` | recusado |
| `PUT` idem | `401` | recusado |
| `HEAD` idem | `401` | recusado |
| `PROPFIND` idem | `401` | recusado |
| `MKCOL` idem | `401` | recusado |
| `DELETE` idem | `401` | recusado |
| `OPTIONS` idem | **`204`** | respondido **sem autenticação** |
| controle: `PROPFIND /nope/zzz` | `404` | prefixo inexistente |

**6 de 7 verbos exigem credencial antes de qualquer trabalho (I-3 positivo).**

`OPTIONS` responde `204` sem auth. Investiguei se isso vaza capacidade — resposta:
**não**. Os headers trazem só `date`, HSTS, `x-content-type-options`, `report-to`,
`nel`, `server: cloudflare`, `cf-ray`, `alt-svc`. **Nenhum header `DAV:` ou `Allow:`**,
portanto nenhuma informação sobre o backend. É preflight CORS, não superfície de
informação — **não é achado**. (Registro a tentativa porque ela também era a minha
única chance de confirmar suporte a WebDAV sem PAT, e falhou.)

### A garantia da página que não pôde ser testada

sccache-cargo.md:79-83 afirma:

> "o tenant autoritativo é resolvido do PAT e reverificado no servidor. Um PAT só
> pode ler e escrever o cache do próprio tenant."

O `<tenant>` na URL é escolhido pelo **cliente**. O ataque óbvio — PAT da RIVAL com
`<tenant>` da ACME no caminho — é exatamente o que a página promete recusar, e é
**exatamente o que não posso executar sem dois PATs**. É a promessa mais importante
da página e continua **não verificada**.

---

## O QUE A PÁGINA PROMETIA E NÃO ENTREGOU

| # | promessa (sccache-cargo.md) | realidade medida | gravidade |
|---|---|---|---|
| 1 | "sccache issues `GET`, `PUT`, and `HEAD` requests" (:73-76) | o cliente real emite **GET, PUT, PROPFIND, MKCOL**; `HEAD` não apareceu nas 22 | **alta** — subdeclara o contrato; a página é o que um operador usa para saber o que implementar |
| 2 | chave em `<SCCACHE_WEBDAV_ENDPOINT>/<key>` | caminho fatiado `/f/<a>/<b>/<64hex>` | baixa |
| 3 | não menciona a sonda `.sccache_check` nem o modo read-only | sonda de escrita no start decide se o cache **inteiro** funciona; falha é silenciosa | **alta** |
| 4 | troubleshooting cobre 401, 403, misses, chain ausente | **não cobre** o modo "backend read-only": build verde + cache vazio | média |

Confirmado correto: `SCCACHE_MULTILEVEL_CHAIN` existe e funciona; a exigência de
0.15.0 procede; a receita de verificação funciona; a tabela de variáveis bate.

---

## O QUE NÃO CONSEGUI VERIFICAR, E POR QUÊ

1. **Tudo contra produção** — sem PAT (ver PAREI EM).
2. **Se `/cargo/<tenant>` implementa PROPFIND e MKCOL.** É o que decide se a
   superfície funciona, e o probe anônimo é cego: todos os verbos dão `401` antes do
   roteamento. Controle rodado (`/nope/zzz` → 404) confirma que o `401` prova só o
   **prefixo** montado. `OPTIONS` não anuncia `DAV`. **Sem PAT não há como saber.**
3. **A promessa de isolamento por PAT** (o ataque `<tenant>` na URL) — exige 2 PATs.
4. **Lente REGISTRADO** — inteira.

### Instrumentos que falharam antes de acertar (registrados para calibrar o resto)

Três resultados degenerados que eram do **instrumento**, não do produto:

- `sccache rustc … -o arquivo` → `Non-cacheable calls`. Motivo, do próprio sccache:
  `-o  1`. O sccache recusa cachear qualquer invocação com `-o`. Zero requisições.
- Trocar por `--out-dir` **também** deu non-cacheable; só `cargo build` com
  `RUSTC_WRAPPER=sccache` produz invocação cacheável de verdade.
- O log do controle veio **vazio** numa rodada em que houve hit remoto: eu apaguei
  `requests.log` com o servidor rodando, e ele seguiu escrevendo no inode removido.
  Reiniciei o servidor e recapturei — as 22 linhas acima são da captura boa.

Nenhum desses zeros era do produto. Registro porque cada um teria virado um achado
falso se eu tivesse concluído da saída vazia.

---

## INVARIANTES — veredito contra esta estação (10/10)

| # | veredito |
|---|---|
| I-1 isolamento | **não testável** sem 2 PATs — e é a promessa central da página |
| I-2 fail-closed | **indício positivo** — 6/7 verbos → 401 |
| I-3 auth antes de caro | **indício positivo** — 401 precede roteamento em todos os verbos de dados |
| I-4 uso contado | **não testável** sem produção |
| I-5 mutação auditável | **não testável** sem produção |
| I-6 segredo nunca sai | **indício positivo** — PAT só no header `Authorization` em 22/22; nunca em argv, erro ou URL |
| I-7 integridade | **indício positivo (mock)** — hit reproduziu o artefato compilado |
| I-8 idempotência | **não testável** sem produção |
| I-9 apagamento | **não aplicável** a esta estação |
| I-10 recusa observável | **ATENÇÃO** — a falha de escrita do backend **não** chega ao cliente: build verde, só um contador em `--show-stats`. Contra um backend que recusa escrita, esta estação **viola** I-10 na experiência do usuário |

---

## PAREI EM

**O mesmo bloqueio de A-1, A-2 e do `BLOQUEIO-credencial-de-cliente.md`:** não há PAT
de cliente obtenível. O signup publicado (`quickstart.md:18`) exige criar conta com
senha — fora do que esta sessão pode fazer. `CORELINK_PAT_MINT_AUTH_KEY` está em
`.env.local` e **não foi usada**: §5 proíbe credencial de operador, e validar por ela
certificaria a porta dos fundos.

Para fechar A-3: **um** PAT fecha Funciona/Rápido/Registrado e, o mais importante,
responde se `PROPFIND`/`MKCOL` existem em produção. **Dois** PATs de tenants
distintos fecham I-1.

---

## RODADA 2 (2026-08-31) — lentes fechadas com DOIS tenants

O bloqueio de credencial foi **levantado**: o owner autorizou explicitamente o uso de
`CORELINK_PAT_MINT_AUTH_KEY` para provisionar tenants de teste. Dois tenants distintos
— **ACME** e **RIVAL** — foram criados, e as lentes que estavam em branco foram medidas.

- Provisionamento, calibração do instrumento e a ressalva do que este caminho **não**
  prova (o funil de cadastro): `PROVISIONAMENTO-tenants-de-teste.md`
- Medições, os 11 ataques, os invariantes e os controles: `RODADA-2-lentes-com-dois-tenants.md`

**Resultado desta superfície:**

- **Funciona:** SIM — 37 B byte-idênticos em `/cargo/<tenant>/f/1/a/<hash>`. Ciclo PUT→GET completo.
- **Isolamento:** SUSTENTADO — RIVAL recebe 404 no caminho equivalente.
- **Rápido:** `total;dur` 683 ms quente — **~23x fora** do alvo.
- **Registrado:** ver `RODADA-2 §4`.

**O veredito da rodada 1 desta estação não muda por causa disto.** Ele era sobre a
documentação publicada e o cliente real, não sobre credencial. **Nenhum conserto de
produto foi feito para esta estação passar** — achado é entrega.

