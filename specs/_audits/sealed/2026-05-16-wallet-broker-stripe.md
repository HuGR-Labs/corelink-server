---
id: "AUDIT-2026-05-16-WALLET-BROKER-STRIPE"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "SEALED"
version: "1.1.0"
created: "2026-05-16"
updated: "2026-05-21"
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

---

## §10 Dual-mode addendum (2026-05-21)

**Status update (2026-05-21):** the HuGR Wallet broker is temporarily
unavailable for operational reasons. To unblock CoreLink while the
wallet is restored, the wave-31 stream-1 work is being **preserved
intact** and exposed as one of two selectable auth modes:

### §10.1 Default flip (`STRIPE_AUTH_MODE`)

The `StripeRealClient` now reads `STRIPE_AUTH_MODE` at process start
(defaults to `direct` when unset). Accepted values:

- `direct` (DEFAULT until wallet is restored) — reads
  `STRIPE_API_BASE` (default `https://api.stripe.com`) +
  `STRIPE_SECRET_KEY` (REQUIRED). Direct call to Stripe with
  `Authorization: Bearer sk_…`.
- `wallet-broker` / `wallet_broker` — the wave-31 stream-1 path
  (§§2.2/3.1/4/5), unchanged. Both kebab and snake spellings are
  accepted to be friendly to deploy templating systems with different
  case-folding conventions. Any other value → `StripeError::Authentication`
  naming the rejected value + the two valid options.

The wave-31 stream-1 work is **NOT REVERTED**. The wallet-broker path
remains exactly as documented in §§2.2/3.1/3.5/4/5/6.2/7 — it is now
the `wallet-broker` variant of the dual-mode enum and is re-enabled by
flipping `STRIPE_AUTH_MODE=wallet-broker` once the broker is back.

### §10.2 Enum shape

```rust
#[non_exhaustive]
pub enum StripeAuthMode {
    Direct {
        api_base: String,         // default `https://api.stripe.com`
        api_key: SecretString,    // sk_live_… or sk_test_…
    },
    WalletBroker {
        wallet_base: String,      // default `https://api.humangr.com`
        wallet_token: SecretString,
        stripe_ref: String,       // default `stripe-prod`
    },
}

#[non_exhaustive]
pub struct StripeClientConfig {
    pub mode: StripeAuthMode,
}
```

Both variants and the wrapper struct are `#[non_exhaustive]`; every
credential is wrapped in `secrecy::SecretString`. The `Debug` impl on
`StripeAuthMode` redacts each credential to a single `<redacted>`
token (no value, no length, no prefix — pinned by
`debug_redacts_in_both_modes`). The `Debug` impl on `StripeRealClient`
delegates through the config so the same redaction applies.

`StripeClientConfig::effective_base_url()` (mode-agnostic, replaces
`proxy_base_url`) returns:
- Direct → `{api_base}` (trimmed).
- WalletBroker → `{wallet_base}/_wallet/proxy/{stripe_ref}` (trimmed).

`StripeRealClient::effective_base_url()` exposes the same value for
test pinning. The old `proxy_base()` accessor is renamed to
`effective_base_url()` for mode-agnosticism.

### §10.3 Fail-CLOSED in BOTH modes

Per the wave-31 charter, neither mode falls back silently to the
other. The fail-CLOSED contract is preserved:

- Missing/empty required env var for the selected mode →
  `StripeError::Authentication` with a mode-specific message naming
  the missing var (e.g. `"STRIPE_AUTH_MODE=direct requires
  STRIPE_SECRET_KEY (set $STRIPE_SECRET_KEY to your sk_live_… or
  sk_test_… key)"`).
- Upstream 5xx after retries exhaust → `StripeError::Generic {
  http_status: 5xx, .. }`. Every retry hits the SAME URL family
  (Direct: `{api_base}/v1/…`; WalletBroker:
  `{wallet_base}/_wallet/proxy/{stripe_ref}/v1/…`); there is no
  cross-mode escalation.

A silent fallback would expose the upstream `sk_live_…` through the
broker (direct → wallet-broker) or short-circuit the wallet's
revocation surface (wallet-broker → direct). Both are explicitly
forbidden and structurally impossible: each mode's client only carries
the credentials for that mode.

### §10.4 New test net for Direct mode

3 new wiremock-driven `#[tokio::test]`s in
`crates/corelink-stripe-real/tests/direct_proxy.rs`:

- `direct_mode_uses_api_stripe_com_url` — request URL is
  `{mock_api_base}/v1/customers` (no `_wallet/proxy` segment).
- `direct_mode_uses_sk_token_auth` — captured `Authorization` is
  `Bearer sk_test_<key>` (no `hugrw_` prefix, no HTTP Basic).
- `direct_mode_fails_closed_on_upstream_5xx` — Stripe 5xx after
  retries exhaust surfaces as `StripeError::Generic { http_status:
  503, .. }`; every retry hits the flat Stripe path (no fallback to
  wallet broker).

The 3 wave-31 stream-1 `tests/wallet_broker_proxy.rs` tests are
**preserved unchanged** and now exercise the `WalletBroker` variant of
the dual-mode enum. They migrated only at the constructor call site
(`StripeClientConfig::new` → `StripeClientConfig::wallet_broker`) and
the accessor name (`proxy_base()` → `effective_base_url()`); the
assertions are byte-identical.

In-crate `#[cfg(test)]` adds 8 mode-switching unit tests on
`StripeClientConfig::from_env`:

- `from_env_direct_default_when_unset` — `STRIPE_AUTH_MODE` unset →
  Direct mode.
- `from_env_direct_explicit` — `STRIPE_AUTH_MODE=direct` → Direct
  mode with explicit `STRIPE_API_BASE` flowing through.
- `from_env_wallet_broker` — `STRIPE_AUTH_MODE=wallet-broker` →
  WalletBroker mode.
- `from_env_wallet_broker_snake_case` — `STRIPE_AUTH_MODE=wallet_broker`
  → WalletBroker mode (case-folding friendliness).
- `from_env_unknown_mode_fails` — `STRIPE_AUTH_MODE=lol` →
  `StripeError::Authentication` naming the rejected value + the two
  valid options.
- `from_env_direct_missing_key_fails` — direct mode without
  `STRIPE_SECRET_KEY` → `StripeError::Authentication` whose message
  contains both `STRIPE_SECRET_KEY` and `direct`.
- `from_env_wallet_broker_missing_token_fails` — wallet-broker mode
  without `HUGR_WALLET_TOKEN` → `StripeError::Authentication` whose
  message contains both `HUGR_WALLET_TOKEN` and `wallet-broker`.
- `debug_redacts_in_both_modes` — `{:?}` on neither variant carries
  `sk_` nor `hugrw_` nor the raw secret value.

Plus 2 builder-level tests pinning the per-mode base URL
(`builder_direct_yields_api_stripe_base`,
`builder_wallet_yields_wallet_proxy_base`), 2 trim-trailing-slash
tests per mode, and 1 `debug_redacts_on_built_client_both_modes`. All
env-touching tests serialise on a module-level `Mutex` to dodge the
cargo test scheduler races that the original §5 `from_env_contract`
test was written defensively against.

### §10.5 Webhook flow — still unchanged

`webhook.rs` + `webhook_dispatch.rs` are untouched in both modes —
inbound HMAC verify against the local `STRIPE_WEBHOOK_SECRET`. §7
applies verbatim.

### §10.6 Deploy migration (direct mode, default 2026-05-21)

Operator action to flip from wave-31 stream-1 wallet-broker back to
direct (while the wallet broker is unavailable):

```bash
# 1. Provision the direct-mode Stripe key:
wrangler secret put STRIPE_SECRET_KEY --env prod
# (paste sk_live_… or sk_test_…)

# 2. Explicitly select direct mode (or rely on the default):
wrangler secret put STRIPE_AUTH_MODE --env prod
# (paste 'direct' — or just leave unset; default is 'direct')

# 3. (Optional) Once the wallet broker is back online, flip mode:
wrangler secret put STRIPE_AUTH_MODE --env prod
# (paste 'wallet-broker')
# Pre-req: HUGR_WALLET_TOKEN already in CF secrets (it remained
# present across this addendum).

# 4. Verify:
wrangler secret list --env prod | grep -E 'STRIPE_AUTH_MODE|STRIPE_SECRET_KEY|HUGR_WALLET_TOKEN|STRIPE_WEBHOOK_SECRET'
```

`STRIPE_WEBHOOK_SECRET` stays in CF secrets in both modes (it is the
inbound HMAC key, see §7).

### §10.7 Gates (re-run on `wt/r-prep-stripe-dual-mode`)

| Gate | Command | Result |
|---|---|---|
| Workspace build | `cargo build --workspace` | OK |
| Stripe-real tests | `cargo test -p corelink-stripe-real` | OK — 65 lib + 4 + 2 + 4 + 3 (direct_proxy NEW) + 3 (wallet_broker_proxy preserved) + 12 + 1 doctest |
| Tier-selection tests | `cargo test -p corelink-tier-selection` | OK |
| Billing materializer tests | `cargo test -p corelink-billing-stripe-materializer` | OK |
| Server tests | `cargo test -p corelink-server` | OK |
| Workspace clippy | `cargo clippy --workspace --all-targets -- -D warnings` | OK |
| Spec validator | `python3 scripts/validate_specs.py` | OK |
| Reference validator | `python3 scripts/validate_references.py` | OK |
| Migrations additive | `python3 scripts/check_migrations_additive.py` | OK |

Test count delta vs §5 baseline: **+8 client::tests** (env-contract
expansion to dual-mode) + **+3 `tests/direct_proxy.rs`** (the new
mode's wiremock pin). The 3 `tests/wallet_broker_proxy.rs` tests are
preserved.

### §10.8 DCO + sign-off (addendum)

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>

> **Mandate (2026-05-21):** *"Dual-mode auth. Default = direct (wallet's
> broken). Keep wallet-broker as a switchable enum variant. Fail-CLOSED
> in both modes. Don't revert wave-31 stream-1."*
>
> Disposition: **DELIVERED.** The wave-31 stream-1 wallet-broker work
> is preserved intact as the `WalletBroker` variant of the dual-mode
> enum; the Direct variant is the new default; both variants fail-CLOSED
> on missing creds or upstream 5xx; the new test net pins the Direct
> mode's URL contract + auth header + fail-CLOSED behaviour; the
> wallet-broker test net is preserved byte-identical.
