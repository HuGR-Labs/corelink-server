# → hugit TL: storage do engine pronto — o que falta do vosso lado pra sair do fixture

**De:** TL do CoreLink (plataforma) · **Para:** TL do hugit (o motor: `hugit-serve`) · **Via:** owner ·
**Data:** 2026-06-13 · **Contexto:** githugr (janela) está no ar em `GITHUGR_MODE=fixture`; o motor
`hugit-serve` precisa ler de storage durável pra virar real. Este doc fecha o lane de storage.

---

## TL;DR

A plataforma **já provisionou o storage**. Falta **só o vosso lado**: (1) escrever 1 snapshot real
e (2) ligar o read path do motor nesse bucket. Sem o snapshot, o motor sobe e serve vazio.

| O quê | Quem | Estado |
|---|---|---|
| Bucket R2 dedicado | CoreLink | ✅ **provisionado** (`corelink-githugr-engine`) |
| Padrão de chave definido | CoreLink | ✅ `<tenant_id>/<repo>.json` |
| Credencial de acesso scoped | CoreLink | ⏳ gero quando me pedir (ver §3) |
| `state.rs` lê `<repo>.json` do bucket | **hugit** | ⏳ |
| **1º snapshot real escrito** no bucket | **hugit** | ⏳ ← *isto é o que destrava "dado real"* |

---

## 1. O que a plataforma entregou

- **Bucket R2:** `corelink-githugr-engine` (criado 2026-06-13, na conta CF da família, Standard,
  vazio). É um bucket **dedicado de leitura do motor** — NÃO é o CAS per-tenant do CoreLink.
- **Padrão de chave (contrato):** **`<tenant_id>/<repo>.json`**, onde `tenant_id` é o **UUID do claim
  Clerk** (`publicMetadata.tenant_id`) — **sem** derivação TDK (isso é só pro CAS per-tenant, que é P2).
  Ex.: `7f3a…-uuid/meu-repo.json`.

## 2. O que o hugit precisa fazer

1. **`state.rs` lê `<repo>.json` do bucket.** Vocês já toparam avaliar essa fonte de leitura R2 — é
   exatamente esse o caminho: dado o `tenant_id` (do contexto autenticado) + o `repo`, lê o objeto
   `<tenant_id>/<repo>.json` do `corelink-githugr-engine`. Fail-honest: objeto ausente → página-casa,
   nunca dado fabricado (vocês já fazem isso).
2. **Escrever o 1º snapshot real.** O hugit exporta um snapshot do event-log (vocês são donos do
   formato `<repo>.json`) e grava em `<tenant_id>/<repo>.json`. **Sem 1 objeto real, o motor serve
   vazio** — então este é o passo que tira o `engine.githugr.com` do fixture.

## 3. Acesso ao bucket — escolha o mecanismo

O `engine.githugr.com` é o 2º container. Containers CF não têm binding R2 nativo (isso é Worker-level),
então o caminho é **credencial S3 R2 scoped ao bucket**:

- **Eu gero um par S3 (Access Key ID + Secret) read+write escopado SÓ ao `corelink-githugr-engine`**
  (least-privilege — NÃO dou as creds account-wide do CoreLink, que veriam todos os CAS buckets).
  Me pede (via owner) que eu gero e mando pelo canal seguro do owner. Endpoint S3:
  `https://<account_id>.r2.cloudflarestorage.com` (te passo o account_id junto).
- **Alternativa:** se o motor for fronteado por um Worker vosso, dá pra usar binding R2 nativo nesse
  Worker (`bucket_name = "corelink-githugr-engine"`, mesma conta, zero credencial) e o Worker
  repassa pro container. Vocês escolhem.

## 4. O que NÃO é pra agora (P2)

- **Ler o CAS per-tenant real do CoreLink** (`<region>/<tenant_prefix(TDK)>/<digest>`) — isso exige a
  derivação TDK (segredo) + um PAT. O caminho: o motor troca a sessão Clerk por um PAT em
  `POST https://corelink-api.humangr.com/v1/session/exchange` (seam C, já vivo) e lê o CAS com ele.
  **P2** — pro launch, o bucket dedicado basta.
- **Runners (resultado de CI real):** não existe read path ainda (fase-3). A tela de checks fica
  fixture/hermética por enquanto.

## 5. Fronteiras (cristalino)

- **CoreLink** é dono do bucket + da credencial scoped + do contrato de chave.
- **hugit** é dono do formato `<repo>.json` + de escrever o snapshot + do read path no `state.rs`.
- **githugr** (janela) não toca storage — só exibe o que o motor serve.

## 6. Done quando

`state.rs` lendo `<tenant_id>/<repo>.json` + 1 snapshot real escrito → `engine.githugr.com` serve dado
real (sai do fixture). Me pede a credencial scoped quando for ligar.

— roteado via owner; sem dependência `path`/`git` entre os repos.
