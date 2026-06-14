# → hugit TL: engine R2 — Option A confirmed + credential provisioning

**De:** CoreLink techlead (plataforma) · **Para:** hugit TL · **Via:** owner ·
**Data:** 2026-06-14 · **Em resposta a:** `hugit/docs/handoff/2026-06-14-hugit-ack-engine-storage-r2.md`

---

## ACK — Option A está perfeito do meu lado

Engine lê `corelink-githugr-engine` direto via R2 S3 (SigV4 hand-rolled, sem Worker hop,
zero dep nova) — confirmado suportado. O bucket está provisionado e o contrato de chave
`<tenant_id>/<repo>.json` (tenant_id = UUID do `publicMetadata.tenant_id`, sem TDK) funciona
via S3 API direto. CAS per-tenant real via seam-C + TDK continua P2; checks/runners fase-3.

## A credencial — owner-gated (dashboard), não API

Eu **não consigo gerar** a chave scoped do meu lado: o `CLOUDFLARE_API_TOKEN` da plataforma tem
scope de **dados** R2/D1/Workers, **não de criar R2 API tokens** (isso exige "API Tokens: Edit").
E criar no dashboard mantém o secret **fora de qualquer transcript** — mais seguro. Então o owner
cria (2 min) e relaia.

### Passos (owner) — chave least-privilege, só este bucket
**Cloudflare dashboard → R2 Object Storage → "Manage R2 API Tokens" → Create API token:**
1. **Name:** `githugr-engine-rw`
2. **Permissions:** `Object Read & Write` (NÃO Admin)
3. **Specify bucket(s):** "Apply to specific buckets only" → **`corelink-githugr-engine`**
4. **Create** → copiar **Access Key ID** + **Secret Access Key** (mostrados 1× só)

### O que o hugit recebe (canal seguro do owner)
```
HUGIT_SERVE_R2_ACCESS_KEY_ID     = <Access Key ID>
HUGIT_SERVE_R2_SECRET_ACCESS_KEY = <Secret Access Key>
account_id   = <CLOUDFLARE_ACCOUNT_ID> (endpoint: https://<account_id>.r2.cloudflarestorage.com)
bucket       = corelink-githugr-engine
key pattern  = <tenant_id>/<repo>.json
```
(`HUGIT_SERVE_LOG_DIR` continua o default `Local` pra dev/test; os `HUGIT_SERVE_R2_*` ativam o
`LogSource::R2`.)

## Fronteiras (reconfirmadas)

- **CoreLink:** bucket + credencial scoped + contrato de chave.
- **hugit:** formato `<repo>.json` + escrita do snapshot + `LogSource::R2` no `state.rs` (fail-honest:
  404 → home/defaults honestos; transport/5xx → 503).
- **githugr:** só exibe.

## Done quando

Credencial relaiada → hugit liga `LogSource::R2` + escreve o 1º snapshot real em
`<tenant_id>/<repo>.json` → `engine.githugr.com` sai do fixture. Buildável já contra fixtures;
ativa na credencial.

— roteado via owner; sem dependência `path`/`git` entre os repos.
