# Wave 32 — Production Deploy Spec (2026-05-22)

> **Doc kind:** wave-scope spec (evidence; _audits/ excluded from canonical schema validation).
>
> **Owner:** Gustavo Schneiter (Security + Release Lead).
>
> **Authored:** 2026-05-22 by Claude Opus 4.7 after Bucket-2 credential setup completed (8/8 creds in `.env.local`, all validated against live SaaS APIs).
>
> **Trigger:** user mandate "Junte o bucket 2 com todas as tasks necessarias para fazer o deploy real. TUdo sota, rigor maximo, organizado, sem gambiarras."
>
> **Charter compliance:** SOTA bar 8.5/10 per audit doc; trust-but-verify per wave-30 protocol; staging-before-prod for any irreversible action; rollback documented per phase; zero gambiarra (Phase B halts the wave if Worker shim implementation surfaces architectural blockers).

## 1. Scope

Convert the engineering-complete CoreLink corpus (wave-30 SEAL: 198 INVs / 143 sealed / 61 CRITICAL TLA-VERIFIED) into a **running production service** on Cloudflare. The engineering-side artefacts (Rust crates, spec corpus, integration tests, audit chain, TLA specs, charter constraints) are assumed correct; this wave delivers the **deployment glue + operator artifacts** required for real customer traffic.

Out of scope for Wave 32:
- Neon × 5 multi-region shadow sink (Bucket 3; deferred per `2026-05-16-pre-cutover-state-snapshot.md §4`).
- LFPDPPP MX attorney sign-off, AWS Artifact PDF, pentest engagement, pilot signups — all operator/vendor-bound per DEBT register §11.
- `STRIPE_SECRET_KEY` rotation from `sk_test_` → `sk_live_` — handled at GA cutover (T-7d) by Owner via dashboard reveal; Stripe CLI only ships `rk_live_` restricted keys by design.
- BYOK customer-side enrollment — per-pilot, post-deploy.

## 2. Pre-flight state (verified 2026-05-22)

| Asset | State |
|---|---|
| Cloudflare account | `6a1fc1c6...` (Gustavo's primary); 8 workers + 6 D1 + 6 KV + 3 R2 already exist (wallet + acme-* legacy) |
| Cloudflare zone | `humangr.com` zone `73f57f6d...` active; 18 DNS records (wallet stack); **zero `corelink.*` records** |
| CF token scope | `Account: Workers Scripts/KV/R2/D1 Edit` + `Zone: DNS/Workers Routes Edit + Zone Read` on humangr.com; **missing `Pages: Edit`** — token bump required for Phase F |
| `worker/src/index.ts` (Worker shim) | **DOES NOT EXIST**; root `wrangler.toml` comment says "a implementar depois" — Phase B |
| Durable Object class | **DOES NOT EXIST** in TypeScript; only bindings declared in wrangler.toml — Phase B |
| `Dockerfile` (Container) | Exists (2026-04-23); multi-stage Rust 1.82-slim-bookworm → debian:bookworm-slim; gRPC on `:50051`; not yet built or pushed |
| Migrations (`migrations/d1/`) | 52 .sql files; `check_migrations_additive.py` scans multiple dirs → 59 total; all additive per INV-AUTH-MIGRATION-ADDITIVE |
| BetterStack page | Exists `id=247652` "Human Guardrail"; subscribable=false; custom_domain=null; 0 components |
| Stripe (test) | sk_test_ from CLI in `.env.local`; webhook endpoint TBD post-deploy |
| PagerDuty | Routing key validated via `/v2/change/enqueue` (no page triggered) |

## 3. Nine-phase plan

```
                          [A. BetterStack]   (independent, today)
                                 │
                                 ▼ (sign-off)
                          [Decision Gate A→B]
                                 │
                ┌────────────────┼────────────────┐
                ▼                                 ▼
       [B. Worker shim + DO]              [C. CF infra provision]
       worker/src/index.ts                 D1/KV/R2 via API; token bump (Pages)
       worker/src/durable_object.ts        wrangler.toml ID populate
                │                                 │
                └────────────────┬────────────────┘
                                 ▼
                          [Decision Gate B+C → D]
                                 │
                                 ▼
                       [D. Migrations + secrets]
                       apply 52 d1 migrations
                       wrangler secret put × N
                                 │
                                 ▼
                       [E. Container deploy]
                       docker build + push to CF
                       DO instantiation smoke
                                 │
                                 ▼
                          [Decision Gate E → F+G+H]
                                 │
              ┌──────────────────┼──────────────────┐
              ▼                  ▼                  ▼
       [F. Pages deploys]  [G. DNS production]  [H. Smoke + cutover]
       apps/docs           7-10 CNAMEs           end-to-end test
       apps/admin-ui       cert verify           GA-readiness refresh
              │                  │                  │
              └──────────────────┼──────────────────┘
                                 ▼
                       [I. Sign-off + tag]
                       audit doc, GA-GATE-CRITERIA,
                       tag `corelink-prod-deploy-v1`
```

## 4. Phase-by-phase scope + gates + rollback

### Phase A — BetterStack live (~2-4h, independent)

**Scope:**
- PATCH page 247652: `subscribable=true`, `timezone=UTC`, `custom_domain=status.corelink.humangr.com`, branding (logo URL, primary/secondary colours per `marketing/launch/STATUS-PAGE-SPEC.md`).
- Create 4 component groups (Customer-Facing API · Compliance & Audit · Identity & Auth · Infrastructure).
- Create 5 components (CAS API · Audit Export · DSR Pipeline · Auth (Clerk JWT) · BYOK Provider Matrix) attached to groups; initial status `operational`; `display_uptime=true`.
- Create 6 incident templates (SEV-0 audit-chain · SEV-1 failover · SEV-1 DSR · SEV-2 audit-latency · SEV-2 shadow-lag · maintenance BYOK rotation), idle.
- DNS: CNAME `status.corelink.humangr.com → hugrl.betteruptime.com` (proxy OFF; TTL Auto).
- Wait Atlassian-equivalent TLS issuance (typ. ≤15 min).

**Gates (all green to seal):**
- `curl -sI https://status.corelink.humangr.com` → HTTP 200 + valid TLS chain.
- `curl -s https://status.corelink.humangr.com/api/v2/summary.json` (or BetterStack equivalent) returns 5 components.
- `validate_specs.py` + `validate_references.py` still green (audit doc valid).

**Rollback:**
- Remove DNS CNAME → reverts to BetterStack-side `hugrl.betteruptime.com` subdomain. Page itself stays.
- PATCH page back to `subscribable=false` if needed (idempotent).

**Audit doc:** `specs/_audits/sealed/2026-05-22-w32-phaseA-betterstack-live.md`.

### Phase B — Worker shim + Durable Object (~16-24h, requires Phase A sign-off)

**Scope:**
- Author `worker/src/index.ts` (~500-1500 LOC TypeScript): HTTP entry point, route table, auth middleware, request forwarding to DO, error mapping.
- Author `worker/src/durable_object.ts` (~200-500 LOC): `CoreLinkServer` DO class, container start/stop/health, request multiplexing to gRPC :50051, lifecycle telemetry to PagerDuty/audit chain.
- Author `worker/tsconfig.json`, `worker/package.json` (vite/wrangler-compatible), `worker/tests/` (vitest, target ≥70% coverage on the shim).
- Update root `wrangler.toml`: uncomment `main = "worker/src/index.ts"`, finalize bindings (placeholder IDs replaced in Phase C).
- Verify wasm32/TS surface compiles: `wrangler dev` local smoke against mock container.

**Gates:**
- `cd worker && pnpm test` ≥70% coverage; all green.
- `wrangler dev --local` boots without error; `curl http://localhost:8787/health` returns 200.
- `cargo build -p corelink-server` (Container binary) builds clean in `release` profile (same as Dockerfile assumes).
- `validate_specs.py` + `validate_references.py` green.

**Rollback:**
- Revert `worker/` directory and `wrangler.toml` `main` line. No infra touched yet.

**Audit doc:** `specs/_audits/2026-05-22-w32-phaseB-worker-shim.md`.

**Hard pause trigger:** if Cloudflare Containers beta surfaces a deal-breaker (e.g., DO ↔ container IPC contract changed, billing model unworkable, Rust binary fails to start in CF runtime), HALT the wave. Document the blocker in the audit doc §7. Do NOT attempt workarounds.

### Phase C — CF infra provision (~4-8h, parallel to Phase B)

**Scope:**
- Token bump: add `Account: Cloudflare Pages: Edit` to existing token OR mint a separate `corelink-pages-deploy` token. Owner-decided based on blast-radius preference.
- API-provision via CF REST: 1 D1 database `corelink-prod-d1`, 5 KV namespaces (`corelink-prod-jwks-kv`, `corelink-prod-cache-kv`, `corelink-prod-rate-limit-kv`, `corelink-prod-session-kv`, `corelink-prod-pilot-signup-kv`), 6 R2 buckets per `wrangler.toml` (`corelink-cas-prod`, `corelink-ac-{sam,iad,lhr,nrt,syd}`).
- Populate `wrangler.toml` placeholder IDs with real values from provisioning.
- `scripts/provision-cf-corelink-prod.sh` — idempotent re-runnable wrapper for the API calls; idempotent by name-match-then-PATCH pattern (mirrors `statuspage-bootstrap.sh` from wave-28).

**Gates:**
- `curl https://api.cloudflare.com/.../accounts/$ACCOUNT/d1/database` shows `corelink-prod-d1`.
- All 5 KV + 6 R2 listable via API.
- Diff `wrangler.toml` shows NO placeholder strings remaining.
- `wrangler whoami` confirms token can read all provisioned resources.

**Rollback:**
- API DELETE each resource. Script `scripts/teardown-cf-corelink-prod.sh` written alongside.

**Audit doc:** `specs/_audits/2026-05-22-w32-phaseC-cf-provision.md`.

### Phase D — Migrations + secrets (~2-4h, requires B+C green)

**Scope:**
- Apply 52 D1 migrations to `corelink-prod-d1` via `wrangler d1 migrations apply corelink-prod-d1 --remote`.
- `wrangler secret put` for: `STRIPE_SECRET_KEY`, `PAGERDUTY_ROUTING_KEY`, `BETTERSTACK_API_TOKEN`, `STRIPE_WEBHOOK_SECRET` (after Phase E creates the endpoint), HMAC keys (generated fresh via `openssl rand -hex 32`).
- Verify against `docs/internal/secrets-checklist.md` matrix — every row must have a corresponding `wrangler secret list` entry post-Phase-D.

**Gates:**
- `wrangler d1 execute corelink-prod-d1 --remote --command="SELECT count(*) FROM sqlite_master WHERE type='table'"` returns >= expected (compute from 52 migrations).
- `wrangler secret list --env prod` matches secrets-checklist.md row-for-row.

**Rollback:**
- Migrations are additive per INV — no rollback path; instead, delete + re-provision D1 from Phase C if catastrophe.
- Secrets: `wrangler secret delete <NAME>` per entry.

**Audit doc:** `specs/_audits/2026-05-22-w32-phaseD-migrations-secrets.md`.

### Phase E — Container deploy (~8-16h, requires D green)

**Scope:**
- `docker build -t corelink-server:prod .` locally; verify binary starts via `docker run` smoke.
- Push to Cloudflare Containers registry (per CF Containers beta docs).
- `wrangler deploy` of the Worker + DO + Container triplet.
- DO instantiation smoke: invoke a `/health` endpoint on the deployed Worker → verify it reaches the DO → DO starts container → container responds 200.
- Gradual deploy: 5% canary first (requires Workers Paid plan ≥ $5/mo); verify metrics clean for 10 minutes; ramp to 100%.

**Gates:**
- `curl https://corelink.gustavoschneiter.workers.dev/health` (default workers.dev subdomain pre-DNS) returns 200.
- CF Container metrics in dash show container running, no crash loops.
- DO storage shows session state initialized (smoke key written + read).

**Rollback:**
- `wrangler rollback` to previous deploy (instant). For first deploy, `wrangler delete` removes everything.
- DO state survives `wrangler rollback`; if DO state is corrupted, `wrangler delete --force` purges it (data loss).

**Audit doc:** `specs/_audits/2026-05-22-w32-phaseE-container-deploy.md`.

### Phase F — Pages deploys (~4-8h, parallel to G+H)

**Scope:**
- `pnpm --filter @corelink/docs build` (4 locales) + `wrangler pages deploy apps/docs/build --project-name corelink-docs`.
- `pnpm --filter corelink-admin-ui build` + `wrangler pages deploy apps/admin-ui/dist --project-name corelink-admin-ui`.
- Custom domains: `docs.corelink.humangr.com` → corelink-docs Pages; `app.corelink.humangr.com` → corelink-admin-ui Pages.

**Gates:**
- Both Pages projects show `success` build state.
- `curl https://docs.corelink.humangr.com` returns 200 + Docusaurus index.html.
- `curl https://app.corelink.humangr.com` returns 200 + admin UI shell.

**Rollback:**
- `wrangler pages deployment delete` or `wrangler pages project delete`.

**Audit doc:** `specs/_audits/2026-05-22-w32-phaseF-pages.md`.

### Phase G — DNS production (~1-2h)

**Scope:**
- Add CNAME records for all CoreLink subdomains pointing at the deployed Worker / Pages / BetterStack:
  - `api.corelink.humangr.com` → Worker route
  - `app.corelink.humangr.com` → Pages corelink-admin-ui
  - `docs.corelink.humangr.com` → Pages corelink-docs
  - `signup.corelink.humangr.com` → Worker route
  - `admin.corelink.humangr.com` → Worker route
  - `acme-dev.corelink.humangr.com`, `staging.corelink.humangr.com`, `sandbox.corelink.humangr.com`, `go.corelink.humangr.com` per wave-29 inventory.
- All proxied (orange cloud) EXCEPT `status.corelink.humangr.com` (set in Phase A, must be DNS-only).
- Worker routes bound via `wrangler.toml` `[[routes]]` blocks.

**Gates:**
- `dig +short api.corelink.humangr.com` returns CF IPs; HTTPS cert valid.
- `curl https://api.corelink.humangr.com/health` returns 200 from Worker.

**Rollback:**
- Delete DNS records via CF API.

**Audit doc:** `specs/_audits/2026-05-22-w32-phaseG-dns.md`.

### Phase H — Smoke + cutover (~4-8h)

**Scope:**
- Run `bash scripts/pre-cutover-weekly-verify.sh` against the live production deploy.
- `bash scripts/ga-cutover-prod-dressrun.sh` (wave-24 dressrun script) end-to-end.
- Refresh `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` with post-deploy state — replace "engineering-CLOSED, deploy-pending" with "deploy-COMPLETE".
- Statuspage-init dressrun re-run (`statuspage-init-dressrun.sh`) against REAL BetterStack page (not mock).
- Update `specs/_compliance/GA-GATE-CRITERIA.md` checklist rows for: status page, worker, container, pages, DNS, secrets.

**Gates:**
- Every script returns exit 0.
- Every GA-GATE-CRITERIA row that the wave touched is signed.
- Adversarial review pass on the deploy state (independent Sonnet agent, SOTA bar 8.5/10).

**Rollback:**
- Roll back via Phase E rollback (Worker) + Phase F rollback (Pages) + Phase G rollback (DNS). Status page (Phase A) stays — it does not depend on the deploy.

**Audit doc:** `specs/_audits/2026-05-22-w32-phaseH-smoke-cutover.md`.

### Phase I — Sign-off + tag (~2-4h)

**Scope:**
- Comprehensive wave-32 closure audit doc at `specs/_audits/2026-05-22-w32-closure.md`.
- DEBT-016 (Statuspage) flipped to CLOSED.
- DEBT-027 (Pilot signups) status uplifted to `signup-infra-deployed` (signups themselves still Owner-bound).
- New top-of-doc entry in DEBT register: `> **2026-05-22 update (v1.5.0):** Wave 32 prod deploy SEALed; status/api/app/docs/signup/admin live on *.corelink.humangr.com`.
- `git tag corelink-prod-deploy-v1` + tag annotation.
- Memory update: `corelink_prod_deploy_sealed_20260522.md`.

**Gates:**
- All Phase A-H audit docs present and reference each other consistently.
- DEBT register version bumped + changelog row.
- `validate_specs.py` + `validate_references.py` green.

**Audit doc:** `specs/_audits/2026-05-22-w32-closure.md`.

## 5. Dispatch model

- **Fase A**: orchestrator-direct (small scope, API calls only).
- **Fases B, C**: parallel Sonnet agents in worktrees; Owner approves merge after independent adversarial review.
- **Fase D**: orchestrator-direct (sequential, irreversible — no agent autonomy).
- **Fase E**: orchestrator-direct with explicit Owner go-ahead per sub-step (irreversible container push).
- **Fases F, G, H**: parallel Sonnet agents; Owner approves merge sequentially.
- **Fase I**: orchestrator-direct (audit doc + tag).

Every agent dispatch carries this audit doc as charter context. Trust-but-verify per `feedback_synchronous_agents` memory.

## 6. Decision gates (Owner approval required between)

- **A → B**: after status page live, Owner reviews cost + time estimates before committing Container deploy.
- **B+C → D**: after Worker shim + infra exist (both reversible), Owner approves the irreversible migrations + secret writes.
- **E → F+G+H**: after Container running, Owner approves DNS cutover.

## 7. Hard pause triggers

(per `corelink_autonomous_execution_charter` memory § "8 hard pause triggers" model, scoped to Wave 32)

1. CF Containers beta surfaces architectural blocker in Phase B/E.
2. Container image size > CF Containers limit; binary rewrite needed.
3. Worker shim TS implementation surfaces gRPC ↔ HTTP impedance mismatch requiring core Rust crate changes.
4. D1 migration apply fails mid-stream (partial migration state).
5. Token scope insufficient for Phase F (Pages) AND token bump fails for any reason.
6. CF account billing surprise (e.g., R2 bandwidth, Container minute pricing > 2× estimate).
7. Custom domain TLS issuance fails on humangr.com subdomain across all retry attempts.
8. Adversarial review on Phase H closure scores < 7.5/10 (SOTA bar miss).

On any trigger: HALT, document in current phase audit doc §7, escalate to Owner.

## 8. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

**End of Wave 32 production-deploy spec.**
