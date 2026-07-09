# CoreLink Capability Inventory — code-verified source of truth

**Date:** 2026-07-09 · **Baseline:** `main` @ 97f32f2f · **Method:** enumeration (not keyhole-grep) — for every capability: `ls` the owning modules, list the FULL route/fn/enum surface, count tests, trace the wiring (entry → handler → store), and run an **adversarial disproof probe** against every `ABSENT`/`PARTIAL` verdict. Verdicts read from **code bodies, not doc-comments**. Worktrees excluded.

> **Why this file exists.** "What's left for go-live?" kept getting different answers because it was re-audited ad-hoc with shallow greps. This is the enumerated answer, checked into the repo. The `docs-reality` CI gate (`scripts/validate_docs_reality.py`) keeps docs/CLI claims honest against the code so this does not silently drift again.

## Verdict legend
- **BUILT** — logic complete AND wired into a live path (entry→handler→store), fail-closed where required. May be gated on prod secrets/env (that is an operator launch step, not missing code).
- **PARTIAL(seam)** — logic built + tested, but a deliberate last-mile is deferred: a prod driver (DO singleton / Tower layer), an activation writer, a deploy/mount, or a build feature-flag. NOT "absent".
- **ABSENT** — genuinely not implemented.

---

## TL;DR — the honest headline

**The entire core product is BUILT and WIRED, fail-closed.** The launch surface — 9 cache surfaces, the full money path (checkout → Stripe webhook → `subscription_state='active'` → quota ceiling), auth/PAT/tenant-isolation, GDPR erase + Ed25519 attestation + audit hash-chain, residency, runners control-plane, and the admin-ui — is done, gated only on prod secrets.

**The "endless flood" was three finite things, not a bottomless product:**
1. **Docs/marketing drift** — onboarding recipes wrong (SHA-256 vs BLAKE3), ~20 CLI commands and 3 SDKs documented that weren't shipped, BYOK "4 providers at GA" overclaim. *(cross-cutting; the biggest real gap; in-flight fix wave + the new docs-reality gate)*
2. **Deliberate prod-wiring seams on ENTERPRISE/scale features** — BYOK activation-writer + real-KMS build flag; multi-region prod DO-singleton driver; failover Tower layer; OTel export construction; DSR customer-portal mount. Logic built + tested; activation deferred by design.
3. **Dev-surface polish** — CLI commands + SDK packaging/publish.

Nothing in the self-serve money/cache/erase path is ABSENT.

---

## Cluster 1 — Cache surfaces (the product)
Spine: worker `matchRoute` (`worker/src/index.ts:575-670`) → tenant DO → container `routes::build_with_factory` (`routes.rs:671-846`) → shared CAS/AC trait objects → `storage/r2_s3.rs` `R2S3Client` on R2.

| Capability | Verdict | Proof (routes · tests) | Seam |
|---|---|---|---|
| native CAS (put/get) | **BUILT** | 6 routes `cas.rs:644` · 46 tests | — |
| native AC | **BUILT** | 3 routes `ac.rs:444` · 36 tests | — |
| Bazel REAPI v2 `/bazel/v2` | **BUILT** | 4 routes `bazel_v2.rs:293` · 34 tests | *stock `bazel --remote_cache=http` 404s (needs REAPI client); doc-recipe issue, not surface* |
| Turborepo `/v8/artifacts` | **BUILT** | 4 routes `turbo_v8.rs:1051` · 43 tests | — |
| sccache/cargo `/cargo` | **BUILT** | adapter `cargo/server.rs:126` · 34 tests | env-gated fail-closed |
| OCI `/v2` | **BUILT** | 5 routes `oci/server.rs:40` · 61 tests | env-gated (`CORELINK_OCI_TOKEN_KEY`) |
| npm `/npm` | **BUILT** | `npm/server.rs:66` · 59 tests | env-gated |
| pip `/pip` | **BUILT** | `pip/server.rs:48` · 65 tests | env-gated |
| brew `/brew` | **BUILT** | `brew/server.rs:81` · 43 tests | env-gated; `_public` needs `tenant_storage_state` row |

**All 9 BUILT end-to-end.** (Adversarial note: empty `.route(` grep on cargo/oci/npm/pip/brew was OVERTURNED — routes live in `corelink-adapter-host` mounted via `nest_service`. This is the exact keyhole trap that produced earlier false "absent" calls.)

## Cluster 2 — Auth / tenancy / billing / quota (the money + access core)
| Capability | Verdict | Proof | Seam |
|---|---|---|---|
| Clerk auth | **BUILT** (fail-closed) | `clerk_auth.ts:105`, issuer-pin fatal if unset; 31+22 tests | TS gate vs Rust analytics must not drift on azp policy |
| PAT mint + verify | **BUILT** (HMAC+Argon2id) | `native_pat_gate.rs:156`, fatal-boot if absent `main.rs:301`; 57+6+29+38 tests | verify cache TTL=5s (revoked PAT works ≤5s) |
| Multi-tenancy isolation | **BUILT** (HMAC R2 prefix) | `auth_tenant.rs:13`, `r2_s3.rs:799`, fail-closed if `R2_TDK_HEX` unset; 10+27 tests | container trusts worker tenant header (compensated by PAT re-verify) |
| Stripe checkout / tier_select | **BUILT** (real client) | `tier_select_checkout.rs:162` real `StripeRealClient`, verified vs Stripe TEST; 34 tests | "WP-B/todo" module header is **STALE** — body implemented |
| Self-serve signup | **BUILT** (Svix-verified) | `webhooks/clerk.ts:472` auto-provision; 18+30 tests | — |
| Stripe webhook + downgrade | **BUILT** (sig-first) | `webhooks/stripe.ts:1187`, `subscription_state='active'` gate; 75+15 tests | **R1 runner-seed gap CLOSED** (`seedRunnersEntitlement:984`) |
| Quota / hard-cap | **BUILT** (3 axes) | byte-accounting + `$`-ceiling + worker cap; `cas.rs:889`; 11+23+40 tests | worker monthly-request cap fail-OPEN on D1 error (backstopped by container caps); `quota/fsm` skeleton unwired |
| Tiering / price map | **BUILT** | `TIER_PRICE_ENV_TABLE main.rs:142`, hard-fail boot if price unset; 49 tests | abandoned checkout ≠ paid quota (correct) |
| Dedup / `_public` | **BUILT** | R2 key convergence `r2_s3.rs`, mig 0073 live; 21 tests | `_public` uncapped + plaintext by frozen policy (digest-gated) |

**Money path wired end-to-end, fail-closed. Zero ABSENT.**

## Cluster 3 — Privacy / compliance
| Capability | Verdict | Proof | Seam |
|---|---|---|---|
| DSR erasure (destructive erase) | **BUILT + ACTIVATED** | `routes/dsr.rs` 5 handlers + 12 backends, real R2 hard-delete; cust entry `POST /v1/customer/account/delete`; ~145 tests | prod secrets (erase key + D1 + Queue) |
| DSR customer portal `/v1/privacy/dsr/*` | **PARTIAL — built, NOT mounted** | `corelink-dsr` `DsrEndpointDO` 6 rights, 127 tests; admin-ui client posts to it | crate not a dep of any deployed binary; internal `/_internal/dsr/*` + account/delete cover live surface |
| Residency / region pin | **BUILT + WIRED** | `residency_guard` layered `main.rs:863`, 409 fail-closed; ~146 tests | actual multi-region routing = topology |
| **BYOK (4 providers + envelope + revocation)** | **PARTIAL (activation-seam)** | 4 providers construct via `byok_orchestrator build_active:223` (NOT AWS-only); envelope crypto WIRED into live CAS store (`r2_s3.rs:684` encrypt on `state=='active'`); ~369 tests | **(1)** default features `= []` → prod ships `InMemoryFake` unless `byok-*-real` flag; **(2) NO activation writer** of `tenant_byok_config` (mig 0081 H5 writer deferred). Wired but cannot switch on. |
| Audit chain + emitter | **BUILT + WIRED + DRAINED** | `audit_drain.rs` mounted `main.rs:734`, hourly cron `crons=["0 * * * *"]`; BLAKE3 chain + Ed25519 head; 209+60 tests | chain only as fresh as last drain; verify prod drain env |
| Erasure attestation (Ed25519) | **BUILT + WIRED** | signed on verify `dsr.rs:722`, **served** `GET /v1/public/attestation/{id}` `main.rs:753`; 41 tests | operator env (`ERASURE_ATTESTATION_*` + R2 bucket) |

## Cluster 4 — Multi-region / observability / runners
| Capability | Verdict | Proof | Seam |
|---|---|---|---|
| Multi-region replication (logic) | **BUILT** | coordinator + replica-worker + region + failover + rollout; `replication_status()`, 24h failback; ~211 tests | every prod impl is `InMemory*` **by design** (docs: prod = CF DO singleton) |
| Multi-region — triggered in prod | **PARTIAL(seam)** | — | container links only the `Region` enum; **no cron/DO-alarm invokes the coordinator**. Needs the prod DO singleton + driver |
| Failover / residency routing | **PARTIAL(seam)** | `FailoverRouter` + health probes, in-memory doubles | prod Tower layer + real health probes not wired |
| Rollout controller | **PARTIAL(seam)** | full FSM + budget + auto-rollback, 43 tests | same DO/driver seam |
| Observability — OTel export | **PARTIAL(seam)** | exporters `DatadogExporter`/`OtelCollectorExporter`/`GrafanaCloudExporter` library-complete, 196 tests; SLO *primitives* ARE wired | `corelink-telemetry` is NOT a container dep; container = `tracing_subscriber` stdout + Workers-Logs. Exporters unconstructed |
| Runners control-plane (mint/revoke/allowlist/entitlement) | **BUILT + WIRED** | `handleRunnerMint/Revoke` routed+dispatched `index.ts:1969`; allowlist + `runners_entitlement` ceiling enforced at mint; customer read routes | live secret provisioning = operator |
| Runners compute fabric | **out-of-repo** | — | the separate `corelink-runners` phase-3 project (correct) |

## Cluster 5 — CLI / SDK / admin-ui
- **CLI (`corelink`)** — **13 real command groups BUILT** (`Ls Get Put Stat Bench Doctor Version Config Audit Whoami Login Ac RunbookDrill`, `tools/cli/src/main.rs:64`). **~20 documented commands ABSENT** (import, bazel-init, the `cas`/`tenant`/`admin` families, `ci mirror`, `ping`, `init`, `audit tail/list/prove`, `config apply`). `audit export` prod path stubbed (`--fixture` only). *(in-flight PR #708 builds bazel-init/audit-tail/config-apply/cas-get done + several partial.)*
- **SDKs** — Python: TWO colliding packages (thin `corelink-py` = health/pat/signup vs PyO3 `corelink_py` = CAS get/put/stat). JS `@corelink/client` = the `corelink-wasm` crate (real source + matching API) but **unbuilt/unpublished**; `sdks/js/` is an empty vapor scaffold. Go = cgo skeleton, **no `go.mod`** → `go get` impossible. **None published.** *(in-flight PR #704 Python-async, #707 JS build these out.)*
- **Admin-UI** — most complete surface: all customer (keys/usage/billing/audit/team/runners/workspaces/account) + admin (tenants/audit/ops) pages wired to real `/v1/customer/*` + `/v1/admin/*`. Residual not-wired sub-fields ($-ceiling BE-7, BYOK config BE-8, last_used_at BE-5, team-resend BE-6) render as honest teaching Callouts — never faked.

---

## What this means for go-live (the bounded residual)

**Self-serve SMB launch product (cache + storage-governance) = BUILT.** To ship it honestly, the only REAL blocking work is **making the docs/marketing match the wired code** (onboarding recipes, remove promises for unshipped CLI/SDK, fix the BYOK overclaim). Operator launch steps (prod secrets: erase key, `R2_TDK_HEX`, price ids, Clerk issuer, BYOK build-flag) are deploy config, not missing code.

**Deliberate prod-wiring seams — activate when the feature is sold, not launch-blocking for self-serve:**
- BYOK: build with a `byok-*-real` feature + add the `tenant_byok_config` activation writer (enterprise/contract).
- Multi-region: prod DO-singleton + a scheduled driver for the coordinator (enterprise/scale).
- Failover Tower layer; OTel export construction; DSR customer-portal mount.

**Reconcile (non-blocking):** stale comments — `tier_select_checkout` "todo" header, `internal_pat.rs:29-35` "no Argon2id" — and the unwired `corelink-billing/quota/fsm` skeleton (superseded by the 3 live quota axes).

_Last verified: 2026-07-09 by 5 parallel enumeration audits (cache · auth/billing · privacy · replication/runners · CLI/SDK), each with adversarial disproof. Keep honest via `scripts/validate_docs_reality.py`._
