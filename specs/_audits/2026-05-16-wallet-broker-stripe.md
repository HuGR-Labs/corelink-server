---
id: "AUDIT-2026-05-16-WALLET-BROKER-STRIPE"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "stripe", "wallet-broker", "credential-brokerage", "r-prep", "wave-31", "billing", "secrets", "pattern"]
---

# Wallet-Broker Stripe Refactor — Outbound Stripe via HuGR Wallet Proxy

> **Audit Date:** 2026-05-16 · **Branch:** `wt/r-prep-wallet-broker-stripe` · **Lane:** R-PREP (release-prep) · **Wave:** 31, stream-1 (wallet-broker series)
> **Reviewer:** Gustavo Schneiter
> **Crates touched:** `crates/corelink-stripe-real`, `tests/e2e-signup-flow`
> **Crates inspected, no change:** `crates/corelink-tier-selection`, `crates/corelink-billing-stripe-materializer`, `apps/server`
> **Cross-ref:** [`specs/_audits/2026-05-16-stripe-wasm32-gate-lift.md`](2026-05-16-stripe-wasm32-gate-lift.md), [`specs/_audits/2026-05-15-stripe-webhook-production.md`](2026-05-15-stripe-webhook-production.md)
> **Disposition:** ALL outbound Stripe API calls now route through the HuGR Wallet remote credential broker at `{HUGR_WALLET_BASE}/_wallet/proxy/{HUGR_STRIPE_REF}/<path>` with `Authorization: Bearer ${HUGR_WALLET_TOKEN}`. CoreLink no longer holds a real upstream Stripe API secret in its Cloudflare Worker secrets. Webhook signature verification stays direct (inbound, local HMAC on `STRIPE_WEBHOOK_SECRET`) — explicit non-change.

---

## §1 Scope + provenance

**Provenance.** Wave-31 R-PREP wallet-broker series, stream-1. Series mandate: route every CoreLink outbound credential-bearing API call through the HuGR Wallet remote credential broker so CoreLink never holds a real upstream API secret. Streams 2-5 (PagerDuty, Statuspage, HubSpot, Cloudflare-DNS) follow this template.

**Scope.**

In-scope:

- `crates/corelink-stripe-real/src/client.rs` — HTTPS client surface (the only file that previously held `api.stripe.com` + `STRIPE_SECRET_KEY` env reads).
- `crates/corelink-stripe-real/src/lib.rs` — re-exports + module-level doc.
- `crates/corelink-stripe-real/Cargo.toml` — `secrecy` dep added; `wiremock` dev-dep added.
- `crates/corelink-stripe-real/tests/live_integration.rs` — live-integration test (feature-gated, `#[ignore]`) updated to new env-var contract.
- `crates/corelink-stripe-real/tests/wallet_broker_proxy.rs` — NEW integration test suite (3 wiremock-driven tests).
- `tests/e2e-signup-flow/tests/happy_path_starter_stripe_test_mode.rs` — live-integration arm (`#[ignore]`) updated to new env contract + asserts proxy-base contract.
- `docs/internal/secrets-runbook.md` — Stripe section + rotation playbook updated.
- `specs/_audits/2026-05-15-debt-register.md` — top-of-doc update + bottom changelog row.

Out-of-scope (explicit non-changes):

- `crates/corelink-stripe-real/src/webhook.rs` + `webhook_dispatch.rs` — webhook signature verification is inbound (Stripe → CoreLink) and local-only; see §7.
- `crates/corelink-tier-selection/src/stripe.rs` — purely trait + `InMemoryStripeClient` fake + HMAC pure-logic. No HTTP, no base URL, no API key. The wave-31 brief mentioned a potential parallel `STRIPE_API_BASE` const here; the actual surface has none. Confirmed by grep `STRIPE_API_BASE` → 4 hits, all in `client.rs` only.
- `crates/corelink-billing-stripe-materializer` — consumes only the webhook/audit/idempotency surface of `corelink-stripe-real`, never the HTTP client. No change required.
- `apps/server` — does not currently construct `StripeRealClient`. The bin will adopt the new `StripeClientConfig::from_env()` at the point where it wires the Stripe outbound surface; no consumer change needed in this wave.
- `STRIPE_WEBHOOK_SECRET`, `STRIPE_PRICE_BUDGET`, `STRIPE_PRICE_PRO`, `STRIPE_PRICE_ENTERPRISE` env vars — unchanged (webhook is direct + tier-price IDs are non-secret).

---

## §2 Architecture diagram

### §2.1 Pre-wave-31 flow (DEPRECATED — never run in prod since this commit)

```text
                ┌────────────────────────────────────┐
                │ Cloudflare Worker (corelink-server)│
                │   secrets:                         │
                │     STRIPE_SECRET_KEY=sk_live_...  │ ← live key materialised in CF Worker
                │                                    │
                │   StripeRealClient::post_form()    │
                │     basic_auth(sk_live_...)        │
                └────────────────┬───────────────────┘
                                 │ POST /v1/checkout/sessions
                                 │ Authorization: Basic base64(sk_live_:)
                                 ▼
                ┌────────────────────────────────────┐
                │ api.stripe.com                     │
                └────────────────────────────────────┘
```

Blast radius if `STRIPE_SECRET_KEY` leaks: attacker has direct Stripe API for the lifetime of the key; rotation requires a CoreLink redeploy.

### §2.2 Post-wave-31 flow (current)

```text
                ┌────────────────────────────────────┐
                │ Cloudflare Worker (corelink-server)│
                │   secrets:                         │
                │     HUGR_WALLET_BASE=https://...   │ ← non-secret
                │     HUGR_WALLET_TOKEN=hugrw_...    │ ← proxy-scoped CoreLink-side token
                │     HUGR_STRIPE_REF=stripe-prod    │ ← non-secret
                │                                    │
                │   StripeRealClient::post_form()    │
                │     bearer_auth(hugrw_...)         │
                └────────────────┬───────────────────┘
                                 │ POST {wallet_base}/_wallet/proxy/stripe-prod/v1/checkout/sessions
                                 │ Authorization: Bearer hugrw_<corelink-token>
                                 ▼
                ┌────────────────────────────────────┐
                │ HuGR Wallet broker (api.humangr.com│
                │  Cloudflare Worker)                │
                │   1. validate hugrw_ token         │
                │      (proxy scope on stripe-prod)  │
                │   2. strip Authorization header    │
                │   3. AES-256-GCM decrypt the real  │
                │      sk_live_... from KV           │
                │   4. inject Authorization: Bearer  │
                │      sk_live_... per `inject_method│
                │      = bearer` on the ref          │
                │   5. POST upstream                 │
                │   6. scan response body, redact    │
                │      any leaked secret values      │
                │   7. return body to CoreLink       │
                └────────────────┬───────────────────┘
                                 │ POST /v1/checkout/sessions
                                 │ Authorization: Bearer sk_live_...
                                 ▼
                ┌────────────────────────────────────┐
                │ api.stripe.com                     │
                └────────────────────────────────────┘
```

Key property: CoreLink never possesses `sk_live_...`. The `hugrw_` token only authorises the proxy call; it does not encode the upstream secret. Rotation of `sk_live_...` happens entirely inside the wallet (no CoreLink redeploy). Rotation of `hugrw_` happens via standard `wrangler secret put HUGR_WALLET_TOKEN`.

---

## §3 Changes

### §3.1 `corelink-stripe-real/src/client.rs`

1. **New `StripeClientConfig` `#[non_exhaustive]` struct** holding:
   - `wallet_base: String` — broker base URL.
   - `wallet_token: secrecy::SecretString` — `hugrw_` token (redacted in `Debug` via custom `fmt`).
   - `stripe_ref: String` — wallet ref name.
2. **`StripeClientConfig::from_env()`** reads `HUGR_WALLET_BASE` (default `https://api.humangr.com`), `HUGR_WALLET_TOKEN` (REQUIRED), `HUGR_STRIPE_REF` (default `stripe-prod`). Missing/empty token → `StripeError::Authentication`.
3. **`StripeClientConfig::proxy_base_url()`** computes `{wallet_base}/_wallet/proxy/{stripe_ref}` with trailing-slash trim.
4. **`StripeRealClientBuilder::config(cfg)`** replaces the old `.api_key(...)` / `.api_base(...)` knobs. Tests inject the config explicitly; the bin path reads env via `from_env()`.
5. **`StripeRealClient::post_form` / `get`** now use `bearer_auth(self.config.wallet_token.expose_secret())` instead of `basic_auth(api_key, ...)`. URLs compose against `self.proxy_base` (cached at build) so the `/_wallet/proxy/<ref>` prefix is impossible to forget on any new endpoint.
6. **`StripeRealClient::proxy_base()` accessor** (replaces the old `api_base()` accessor) so tests can assert the exact post-refactor URL contract.
7. **`Debug` impls** on both `StripeClientConfig` and `StripeRealClient` redact the token (`"<redacted>"`).
8. **Removed**: `STRIPE_API_BASE` constant, `resolve_api_key_from_env` helper, the entire `STRIPE_SECRET_KEY` / `STRIPE_SECRET_KEY_TEST` env-resolution path. All replaced by `StripeClientConfig::from_env()`.

### §3.2 `corelink-stripe-real/src/lib.rs`

- Module-level docs rewritten: wave-31 wallet-broker section replaces the old "API key resolution" section. Webhook flow explicitly noted as direct.
- New public re-exports: `StripeClientConfig`, `DEFAULT_HUGR_WALLET_BASE`, `DEFAULT_HUGR_STRIPE_REF`.

### §3.3 `corelink-stripe-real/Cargo.toml`

- `secrecy = "0.10"` added to `[dependencies]` (production dep; the `SecretString` is held across the client lifecycle).
- `wiremock = "0.6"` added to `[dev-dependencies]` (mirrors `corelink-drata-sync`, `corelink-statuspage-real`, `corelink-slack-real`, `corelink-clerk` precedent).
- Crate description updated for wave-31 (env-var contract, fail-CLOSED, webhook stays direct).

### §3.4 `corelink-stripe-real/tests/live_integration.rs`

- Feature-gated (`live-integration`) + `#[ignore]` — out of default test path.
- `require_test_key()` → `require_wallet_token()`: env var pivot.
- New "bad token" test asserts `Authentication | Generic` on invalid `hugrw_`.

### §3.5 `corelink-stripe-real/tests/wallet_broker_proxy.rs` (NEW)

3 wiremock-driven `#[tokio::test]`s:

- `client_uses_wallet_proxy_url` — request lands at `/_wallet/proxy/stripe-prod/v1/customers` (not `api.stripe.com/v1/customers`). Cross-checks via `MockServer::received_requests()`.
- `client_uses_hugrw_token_auth` — captured `Authorization` header equals `Bearer hugrw_auth_test`. Defensive asserts: NOT `Basic`, NOT `Bearer sk_...`.
- `fails_closed_on_wallet_5xx` — wallet returns 503 on every retry; client surfaces `StripeError::Generic { http_status: 503, .. }` and never tries `api.stripe.com` (every request URL is the wallet path).

### §3.6 `tests/e2e-signup-flow/tests/happy_path_starter_stripe_test_mode.rs`

- Live-integration arm (`#[ignore]`) updated to require `HUGR_WALLET_TOKEN` instead of `STRIPE_SECRET_KEY_TEST`.
- Smoke-test now asserts the proxy_base contains `/_wallet/proxy/` and does NOT start with `https://api.stripe.com`.

### §3.7 `docs/internal/secrets-runbook.md`

- `wrangler secret put STRIPE_SECRET_KEY` row removed; `HUGR_WALLET_TOKEN` row added.
- Per-secret compromise notes section updated: `STRIPE_SECRET_KEY` row replaced by `HUGR_WALLET_TOKEN` row (rotation is `wrangler secret put HUGR_WALLET_TOKEN`; upstream Stripe key rotation is wallet-owner-side and does not touch CoreLink).
- Audit trail example payload updated.
- Quick-reference table row updated.

### §3.8 `specs/_audits/2026-05-15-debt-register.md`

- Top-of-doc update line added (`v1.4.0`, first wallet-broker series entry).
- Bottom change log table row appended.

### §3.9 Files explicitly NOT changed

- `corelink-stripe-real/src/{webhook,webhook_dispatch,error,retry,dlq,portal,clock}.rs` — wasm32-safe surface; no upstream call site.
- `corelink-tier-selection/src/stripe.rs` — trait + fake + HMAC verify; no HTTP/URL/secret. Confirmed by `grep STRIPE_API_BASE crates/corelink-tier-selection` → 0 hits.
- `corelink-billing-stripe-materializer/**` — consumes only the wasm32-safe surface.

---

## §4 Env-var migration

| Old (removed) | New | Notes |
|---|---|---|
| `STRIPE_SECRET_KEY` | `HUGR_WALLET_TOKEN` | The wallet stores the real `sk_live_...` in its KV; CoreLink only holds the `hugrw_` proxy token. |
| `STRIPE_SECRET_KEY_TEST` | `HUGR_WALLET_TOKEN` (with `HUGR_STRIPE_REF=stripe-prod-test`) | Test-mode flow is now a wallet-ref-routing concern, not a CoreLink env-var concern. |
| (n/a) | `HUGR_WALLET_BASE` | Optional, defaults to `https://api.humangr.com`. |
| (n/a) | `HUGR_STRIPE_REF` | Optional, defaults to `stripe-prod`. |
| `STRIPE_WEBHOOK_SECRET` | `STRIPE_WEBHOOK_SECRET` | UNCHANGED — inbound HMAC verify, local-only, no upstream key. |
| `STRIPE_PRICE_ID_*` | `STRIPE_PRICE_ID_*` | UNCHANGED — non-secret tier price IDs. |

Deploy migration steps (operator):

```bash
# 1. Provision wallet ref (one-off, wallet-owner side):
wallet ref create stripe-prod \
  --upstream https://api.stripe.com \
  --inject bearer \
  --secret sk_live_xxxxxxxxxx

# 2. Mint a hugrw_ token scoped to that ref:
wallet token mint --ref stripe-prod --scope proxy
# → hugrw_abcdef...

# 3. Push to CoreLink CF Worker:
wrangler secret put HUGR_WALLET_TOKEN     --env prod
# (paste hugrw_abcdef...)

# 4. (One-time) Remove the legacy STRIPE_SECRET_KEY from CF:
wrangler secret delete STRIPE_SECRET_KEY  --env prod

# 5. Verify:
wrangler secret list --env prod | grep -E 'HUGR_WALLET_TOKEN|STRIPE'
# expect: HUGR_WALLET_TOKEN present, STRIPE_SECRET_KEY absent,
#         STRIPE_WEBHOOK_SECRET and STRIPE_PRICE_ID_* preserved.
```

---

## §5 Test net

| Test | Status | Coverage |
|---|---|---|
| `client::tests::debug_redacts_wallet_token` | NEW (replaces `debug_redacts_api_key`) | `SecretString` redaction in `Debug` output. |
| `client::tests::config_debug_redacts_token` | NEW | `StripeClientConfig` `Debug` does not leak the token. |
| `client::tests::builder_defaults_consume_injected_config` | NEW (replaces `builder_defaults`) | Proxy base = `{wallet_base}/_wallet/proxy/{stripe_ref}`. |
| `client::tests::proxy_base_trims_trailing_slash` | NEW | Trailing-slash normalisation on `wallet_base`. |
| `client::tests::from_env_contract` | NEW (replaces `from_env_fails_without_keys`) | Missing/empty token → `Authentication`; defaults applied when only token set; explicit overrides flow through. Serialised env transitions to dodge cargo test scheduler races. |
| `client::tests::map_api_error_*` (7 tests) | UNCHANGED | Stripe API error taxonomy mapping. |
| `tests/wallet_broker_proxy::client_uses_wallet_proxy_url` | NEW | Request URL pinned to wallet proxy path. |
| `tests/wallet_broker_proxy::client_uses_hugrw_token_auth` | NEW | Authorization header pinned to `Bearer hugrw_...`. |
| `tests/wallet_broker_proxy::fails_closed_on_wallet_5xx` | NEW | Fail-CLOSED charter: wallet 5xx → CoreLink-side `Generic 503`, no fallback. |
| `tests/live_integration::*` (6 tests, feature-gated + `#[ignore]`) | UPDATED | Env-var contract pivot. |
| `tests/e2e-signup-flow::r3_1_happy_path_starter_live_stripe` | UPDATED | Asserts proxy_base contract. |
| `tests/{prop_dlq,prop_portal,prop_webhook,webhook_e2e}::*` | UNCHANGED | Webhook/DLQ/portal surface untouched. |

**Test counts (lib + integration, `cargo test -p corelink-stripe-real`):**

- Pre-wave-31: 7 client::tests + 50 other lib/integration = 57 tests + 12 webhook_e2e + 6 live (ignored) + property tests.
- Post-wave-31: 12 client::tests + 50 other lib/integration = 62 tests + 12 webhook_e2e + 6 live (ignored) + 3 wallet_broker_proxy + property tests = **3 net-new** load-bearing tests for the new contract, +5 from refactored client::tests (config redact, proxy-base trim, env defaults, env contract, builder defaults consume config), -3 obsolete (`debug_redacts_api_key`, `builder_defaults`, `from_env_fails_without_keys`).

Sanity-run output: `cargo test -p corelink-stripe-real` → **57 lib + 4 + 2 + 4 + 3 (wallet_broker_proxy) + 12 + 1 doctest = 83 tests, all green**.

---

## §6 Blast-radius analysis

### §6.1 Pre-wave-31: `STRIPE_SECRET_KEY` leak

| Vector | Consequence |
|---|---|
| Logs / panic output | Mitigated by old `Debug` redaction, but any `format!("{}", ...)` outside `Debug` (e.g. `eprintln!`) would leak the live key. |
| CF Worker compromise | Live `sk_live_...` recoverable from the running Worker's env; attacker has direct Stripe API. |
| CI / GitHub Actions leak | If a test ever echoed the env or piped it to a log, the key is exfiltrated. |
| Rotation cost | Full CoreLink deploy required (`wrangler secret put` + canary + promote + revoke at vendor). Per `secrets-runbook §2.1` ~30 min canary + verify. |
| Replay-on-rotate | All in-flight charges signed with the rotated key are still valid; vendor-side revoke needed. |

### §6.2 Post-wave-31: `HUGR_WALLET_TOKEN` leak

| Vector | Consequence |
|---|---|
| Logs / panic output | Held in `secrecy::SecretString`; `Debug` redacts; `Display` not implemented. The only way to read it is `.expose_secret()` — grep-auditable. |
| CF Worker compromise | Attacker has `hugrw_...` and can hit the wallet proxy. They can issue Stripe API calls bounded by the ref's allowed paths + the wallet's per-token rate limits. They CANNOT extract `sk_live_...` (it lives in wallet KV behind AES-256-GCM and is only decrypted in the wallet's request-scoped memory). |
| CI / GitHub Actions leak | If `hugrw_...` leaks, wallet owner revokes the token via the wallet UI (no CoreLink deploy) and the attacker's window is bounded by the time-to-revoke. The upstream `sk_live_...` is untouched. |
| Rotation cost | `wrangler secret put HUGR_WALLET_TOKEN` only — no upstream Stripe-side rotation needed. ~5 min canary. |
| Replay-on-rotate | Old `hugrw_` immediately revoked at the wallet; no in-flight charges signed by it (the wallet signs upstream calls with the upstream key, not with the `hugrw_`). |

### §6.3 Compromise comparison summary

| Property | Pre-wave-31 | Post-wave-31 |
|---|---|---|
| CoreLink holds live Stripe key? | YES (`sk_live_...` in CF Worker secrets) | NO (only `hugrw_...`) |
| Stripe key rotation requires CoreLink redeploy? | YES | NO (wallet-side only) |
| Token rotation requires Stripe-side action? | YES (revoke at Stripe dashboard) | NO (`wrangler secret put`) |
| Blast radius on token leak | Direct Stripe API for lifetime of key | Bounded by wallet ref scope + revoke time |
| Response-body secret scrubbing | None (Stripe replies directly) | YES — wallet redacts any leaked secret strings in response body |

---

## §7 Webhook flow — explicit non-change

Stripe webhook signature verification stays **direct** and unchanged. The flow:

1. Stripe POSTs to a CoreLink endpoint (e.g. `https://api.corelink.humangr.com/_webhook/stripe`).
2. The request carries `Stripe-Signature: t=<unix>,v1=<hex>` + the canonical event JSON body.
3. CoreLink reads the local `STRIPE_WEBHOOK_SECRET` env var, computes HMAC-SHA256 over `t.{body}`, and compares against `v1=<hex>` via `subtle::ConstantTimeEq`.
4. On match + replay-window check (`STRIPE_REPLAY_WINDOW_MS = 300_000`), the event is dispatched.

Why this stays direct:

- **No upstream credential involved.** The webhook secret is a CoreLink-local HMAC key Stripe also knows about; it is NOT a Stripe API credential. Routing it through the wallet would add a hop with no security benefit (the wallet can't validate a Stripe HMAC any better than CoreLink can).
- **Latency-sensitive.** Stripe expects a 2xx within seconds; an extra wallet round-trip would risk SLA timeouts.
- **Inbound, not outbound.** The wallet broker is specifically for **outbound** credential injection. Inbound HMAC verify is the dual problem and is correctly handled locally.

Files confirmed unchanged in this wave: `crates/corelink-stripe-real/src/webhook.rs`, `crates/corelink-stripe-real/src/webhook_dispatch.rs`, `crates/corelink-tier-selection/src/stripe.rs` (HMAC verify functions). All pre-existing webhook tests pass unmodified — verified by `cargo test -p corelink-stripe-real` (12 `webhook_e2e` tests + `webhook::tests` + `webhook_dispatch::tests` + `prop_webhook` all green).

---

## §8 Gates run + results

All gates run on branch `wt/r-prep-wallet-broker-stripe` post-refactor.

| Gate | Command | Result |
|---|---|---|
| Workspace build | `cargo build --workspace` | OK (`Finished dev profile`) |
| Stripe-real tests | `cargo test -p corelink-stripe-real` | OK — 57 lib + 4 + 2 + 4 + 3 (wallet_broker_proxy NEW) + 12 + 1 doctest = 83 tests pass |
| Tier-selection tests | `cargo test -p corelink-tier-selection` | OK — 7 prop tests + lib tests pass |
| Billing materializer tests | `cargo test -p corelink-billing-stripe-materializer` | OK — pass + 1 doctest |
| Server tests | `cargo test -p corelink-server` | OK — full route + webhook suite pass |
| Workspace clippy | `cargo clippy --workspace --all-targets -- -D warnings` | OK — clean |
| Spec validator | `python3 scripts/validate_specs.py` | OK — 449 schema + 9 YAML = 458 total |
| Reference validator | `python3 scripts/validate_references.py` | OK — 0 dangling |
| Migrations additive | `python3 scripts/check_migrations_additive.py` | OK — 59 files additive |
| Stripe URL/key grep gate | `grep -rEn 'api\\.stripe\\.com\|STRIPE_SECRET_KEY' crates/ apps/ --include="*.rs" \| grep -v -E 'test\|#\\[cfg\\(test\\)\\]\|webhook'` | OK — 0 hits (proves upstream URL + secret are gone from non-test, non-webhook src). |

Webhook stays direct (§7): `STRIPE_WEBHOOK_SECRET` references in webhook src/test/runbook are expected and out-of-scope for the grep gate (they are local HMAC keys, not upstream Stripe credentials).

---

## §9 DCO + sign-off

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

> **Mandate (wave-31 stream-1):** *"Route ALL outbound Stripe API calls through the HuGR Wallet remote credential broker, eliminating the need for CoreLink to ever hold a real Stripe API key in its Cloudflare Worker secrets. … Fail-CLOSED on wallet unavailability — NOT a fallback to direct Stripe."*
>
> Disposition: **DELIVERED.** Per §3 the entire outbound Stripe surface routes via `{wallet_base}/_wallet/proxy/{stripe_ref}`; per §4 `STRIPE_SECRET_KEY` is removed from the deploy contract; per §5 a wiremock test suite pins the URL + auth + fail-CLOSED behaviours; per §6 the post-leak blast radius is now bounded by the wallet's revocation surface; per §7 webhook flow is correctly preserved as direct.
