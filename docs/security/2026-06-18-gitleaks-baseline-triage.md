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

### Update 2026-08-03 — the "false positives" were RULE DEFECTS, and are now fixed

PR #1002 put this sweep on a **weekly cron** (`17 5 * * 6`, first firing
2026-08-08), which turns "20 historical FPs triaged out of band" from a note in
a doc into a gate that is RED every Saturday. Re-measured on `fcc517c3`: **17
findings / 2,561 commits / 85.47 MB, exit 1.**

Sixteen of the seventeen were traced to three detection defects in the rules
themselves — not to anything about the files they landed in — and `.gitleaks.toml`
now fixes them at the rule level. **After: 1 finding.** The three defects:

1. `curl-auth-user` fired on `-u "${VAR}:"` — a shell variable REFERENCE. Upstream
   already allowlists the mirror case (`user:$VAR`) and simply has no rule for a
   credential in the user position with an empty password.
2. `curl-auth-header` fired on `Authorization: Bearer corelink_pat_...` — a value
   elided with an ellipsis, i.e. a token ending in `.`.
3. `generic-api-key` fired on ascending character walks (`ABCDEFGH01234567`,
   `0123456789abcdef`), on a value that states its own size in prose
   (`my-20-char-internal-k`), and on a PagerDuty `dedup_key`.

**No path, commit, file or literal was allowlisted** to achieve this: every
predicate is a property of the value, so it holds repo-wide and for future
commits too. The rule set was re-proved in the other direction against a planted
commit carrying a `whsec_`, an `AKIA…`, a `ghp_…`, an `sk_live_…`, a literal
`curl -u "user:pass"`, a real 3-segment bearer token, a 64-hex
`CORELINK_INTERNAL_AUTH_KEY` and an AWS secret-access-key — detection is
byte-identical before and after the change (10 caught, 0 lost).

⚠️ **Note for anyone triaging this sweep in future: editing the fixture at HEAD
does nothing.** `gitleaks detect` scans the COMMIT GRAPH, so a finding lives in
the commit that introduced it forever. Three of the 17 were reported at
`apps/server/src/routes/signup.rs` and `crates/corelink-cli/src/{auth,lib}.rs` —
paths that do not exist at HEAD at all. Only a rule change (or a history rewrite)
moves this number.

**The one survivor is the real one** — the `whsec_` at
`docs/operator/stripe-checkout-e2e-2026-05-29.md:73`, commit `b59c4862`. It is
deliberately NOT suppressed. The sweep will stay red until the OWNER ACTION above
(rotation confirm) is done, and that is the point: one real item, visible.

### Sweep completeness
Separately confirmed via `git grep`: **0** `sk_live_` / `rk_live_` (live Stripe),
**0** `sk_test_` in tracked files (test keys live only in gitignored `.env.local`),
**0** AWS `AKIA…`, **0** GitHub `gh[pousr]_…` PATs. The only `-----BEGIN … PRIVATE
KEY-----` hits are doc-comments in `crates/corelink-dpa-acceptance/src/jwt.rs`
explaining PEM formats (no key material).
