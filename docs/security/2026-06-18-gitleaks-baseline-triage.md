# gitleaks baseline triage — 2026-06-18

First full-history `gitleaks detect` over the repo (1650 commits, ~72 MB), with
`.gitleaks.toml` (default ruleset + custom `whsec_` / internal-auth-hex rules +
fixture/spec allowlist). This is the launch-hardening secret sweep.

## Result: 1 real (handled) + 20 historical false positives. No live key leaked.

The PR-diff gate (`gitleaks.yml` on `pull_request`, `base..head`) is **clean** —
every finding below is in HISTORY, not in any new commit, so normal PRs are green.
The numbers here are from the manual full-history dispatch mode.

### The one real finding — HANDLED
- **Stripe webhook signing secret** `whsec_…` in
  `docs/operator/stripe-checkout-e2e-2026-05-29.md:73` (commits `b59c4862`,
  `02df9a17`, 2026-05-30). The default gitleaks ruleset did NOT flag this — our
  custom `stripe-webhook-secret` rule caught it.
  - **Action taken:** redacted in HEAD (working tree no longer carries it).
  - **⚠️ OWNER ACTION (rotation):** the secret is in git history = disclosed to
    anyone with repo read. The doc itself records it was already *stale/wrong*
    (the endpoint `we_1Tca…` it belonged to was deleted), so it is almost
    certainly already dead — but **confirm in the Stripe dashboard that this
    webhook secret is rotated/revoked**. A full git-history purge (filter-repo)
    is owner-gated and low-priority given the secret is stale; redaction +
    rotation-confirm is the proportionate response.

### The 20 false positives (all historical, all benign)
| Count | Rule | Where | Why FP |
|---|---|---|---|
| 4 | generic-api-key | `…/cli/src/{auth,lib}.rs`, `…/signup.rs` (OLD paths) | `token_id = "…"` — token_id is a PUBLIC PAT identifier, not a secret. Gone from HEAD (CLI moved `crates/corelink-cli`→`tools/cli`; literals removed). |
| ~8 | generic-api-key | `docs/security/2026-06-13-CAA-360-audit-report.md` | Disclosed/example PAT ids quoted in an audit report. |
| ~8 | curl-auth-user / -header | `scripts/e2e-stripe-checkout.sh`, e2e docs | `curl -u`/`-H "Authorization:"` with `${STRIPE_…}` env-var interpolation — no literal secret. |

### Coverage note (the lesson)
The default gitleaks ruleset does **not** include Stripe webhook secrets
(`whsec_`). `.gitleaks.toml` adds that rule (and a `CORELINK_*_AUTH_KEY` /
`PAT_SIGNING_KEY` 64-hex rule). The allowlist deliberately keeps operational
trees (`docs/operator/`, `scripts/`, all `src/`) IN scope — the real leak was in
`docs/operator/`, so muting docs wholesale would have hidden it.

### Sweep completeness
Separately confirmed via `git grep`: **0** `sk_live_` / `rk_live_` (live Stripe),
**0** `sk_test_` in tracked files (test keys live only in gitignored `.env.local`),
**0** AWS `AKIA…`, **0** GitHub `gh[pousr]_…` PATs. The only `-----BEGIN … PRIVATE
KEY-----` hits are doc-comments in `crates/corelink-dpa-acceptance/src/jwt.rs`
explaining PEM formats (no key material).
