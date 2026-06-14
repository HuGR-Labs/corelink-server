# corelink-server → corelink-runners: envelope-credential (Option A) — CoreLink side CONFIRMA

**De:** corelink-server techlead (via owner relay) · **Para:** corelink-runners techlead ·
**Data:** 2026-06-12 ·
**Re:** hugit techlead reply `hugit/docs/handoff/2026-06-12-to-corelink-runners-envelope-reply.md`
(decisão **Option A — mesmo tenant PAT** pro `CaptureHook` credential seam §13.2) ·
**Owner ratification:** ✅ Gustavo, 2026-06-12

---

## TL;DR — Option A não exige NADA novo no corelink-server

A decisão do hugit (envelope-subscriber usa **o mesmo tenant Bearer PAT que adquiriu o
lease**) é **100% compatível** com o auth que o CoreLink já expõe. **Zero trabalho novo no
nosso lado** pra essa decisão. Podem proceder com o flip do `leases.rs` + o §13.2 M1
wiring sem nos esperar.

## Por que Option A "just works" no CoreLink

O endpoint de introspecção que vocês vão chamar — `POST /internal/v1/auth/introspect`
(PR **#261**, contrato congelado em
`docs/handoff/2026-06-12-corelink-runners-auth-seam-response.md`) — resolve **qualquer**
tenant PAT pro seu `tenant_id`. Como o acquire e o envelope-poll usam o **mesmo** PAT:

- Mesmo PAT → mesmo `tenant_id` → mesma tenancy nos dois (acquire + envelope). ✓
- **Fail-closed que vocês pedem (item 4) já é o comportamento do endpoint:** credential
  errado/desconhecido → `{valid:false}` → 401; backend fora → 503 → seu `Err(Unreachable)`.
  **Nunca dreno cross-tenant, nunca admissão anônima.** Vocês não precisam construir essa
  postura — ela já vem do `PatVerifier`.
- **Sem credential separado/derivado (P2 n/a):** não há nova auth API do CoreLink, nenhuma
  emenda de tipo frozen (`AcquireResponse`), nada. O PAT no wire layer é a única auth.

## Confirmações pontuais aos itens da §2

1. ✅ **CaptureHook credential = acquire tenant PAT** está correto pro hugit — promover de
   "assumption" pra "ratified" no `leases.rs` de vocês: **endossado do lado CoreLink.**
2. ✅ **Procedam com o §13.2 M1 wiring** — nada no corelink-server bloqueia.
3. ✅ **Não adicionem** Option B/C pro hugit — concordamos; o modelo é um-PAT-por-tenant
   (ADR-0002), confirmado no nosso auth model (`specs/03_architecture/auth_model.md`).
4. ✅ **Fail-closed (503 wrong cred / 404 ownership)** — mantido; é exatamente nossa postura.
5. ✅ **IntentMetrics conformance vector (§13.4)** — owner/hugit-gated, fora do nosso escopo.

## A ÚNICA dependência (sequenciamento, não bloqueio de design)

O go-live do §13.2 depende do **tenant P2 do hugit estar provisionado no CoreLink**
(`hugit/docs/handoff/2026-06-08-corelink-p2-tenant-request.md`). Do nosso lado isso é o
endpoint **admin-pilot create** (`POST /v1/admin/pilots`, PR **#260**) + a chave live
(owner, task #46). Sequenciem o §13.2 go-live contra esse provisioning — avisamos via owner
quando o tenant do hugit estiver mintado.

---

**Resumo:** decisão ratificada, CoreLink-side confirmado, **nada a construir no
corelink-server**. Sigam com o flip + §13.2 M1. Ping via owner se algo aqui divergir do que
o PR #24 de vocês assumiu.
