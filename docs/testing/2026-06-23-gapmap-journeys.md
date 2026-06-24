# CoreLink customer-lifecycle / journey GAP MAP — 2026-06-23

> **Read-only audit.** Maps every customer lifecycle stage + persona against what
> the user-simulation suites actually DRIVE, vs what merely GATES (never runs in
> practice) or is ABSENT. Brutally honest: a suite that compiles 60 journeys but
> GATES 40 of them in every real run is not 60 journeys of coverage.

## Suites audited
| Suite | Nature | What it can prove |
|---|---|---|
| `tests/e2e-user-journeys/` (Rust, 18 modules, ~70 journeys) | **black-box** HTTP+PAT vs a deployed endpoint | the data/auth/billing-edge contract — IF the persona tokens are provisioned |
| `scripts/e2e-real-client/run.sh` + `lib/` | **black-box real CLIs** (docker/cargo+sccache/brew/curl bazel/turbo) vs PROD, real Clerk signup | the real-tool conformance moat |
| `scripts/e2e-real-client/provision-and-run-suite.sh` | provisions personas the real way, then runs the Rust suite | which personas actually get tokens |
| `tests/e2e-pilot-onboarding/` (5 tests) | **IN-MEMORY fakes + internal crates** (`src/harness.rs` = in-memory D1/CAS) | the lifecycle ORCHESTRATION logic — NOT the live system |
| `tests/e2e-signup-flow/` (6 tests) | **IN-MEMORY fakes + internal crates** (`corelink-signup`, etc.) | signup orchestration logic — NOT the live system |

**Structural finding #1:** offboarding, 30-day grace, DSR finalize, the audit
chain, and tier-select→checkout are covered ONLY by the two in-memory suites
(`pilot-onboarding`, `signup-flow`). Those import `corelink-*` crates and wire
in-memory D1/R2 fakes — they validate the pure orchestration logic, but a passing
in-memory test says NOTHING about whether the deployed worker+container actually
do it. They are unit/integration tests wearing an "e2e" name.

**Structural finding #2:** the only script that provisions personas the real way
(`provision-and-run-suite.sh`) provisions **exactly 4 of the 12 personas**: P1
(RW), P6 (TenantB), P2 (RO), P4 (revoked). Everything keyed to P3 Admin, P5
Expired, P7/P8/P9/P10 tier plans, P11 PastDue **GATES in every real run** because
no env var is ever set. The Rust suite "supports" those personas; the operator
never feeds them.

---

## Lifecycle-stage × coverage-status

Legend: **REAL** = a journey actually runs end-to-end in a normal provisioned run ·
**GATED** = a journey exists but never runs without out-of-band creds/flags the
provision script does not supply · **SIM-ONLY** = covered only by an in-memory
suite (not against the live system) · **ABSENT** = no journey at all.

### Onboarding
| Stage | Status | Where / why |
|---|---|---|
| Clerk signup → org=tenant → PAT provision | **REAL** | `run.sh` bootstraps 2 real users via Clerk Backend API + polls private_metadata (the genuine path). Also SIM in pilot `test_01`. |
| First cache write/read | **REAL** | `run.sh` probe_native_cas; Rust `cas::cache_miss_then_hit`. |
| `/v1/users/me` identity reflection | **REAL** | `identity::onboarding_ping`. |
| **Multi-seat / team onboarding** | **ABSENT** | no journey creates a 2nd seat under ONE tenant. |
| **Inviting teammates** (`POST /v1/customer/team/invite`) | **ABSENT** | URL builder exists in the harness doc table; **no journey calls it**. `team`/`invite` appears nowhere in any `journeys/*.rs`. |

### Money path
| Stage | Status | Where / why |
|---|---|---|
| `GET /v1/customer/billing` state shape | **REAL** | `billing::billing_state`, `dashboard::*`. |
| tier-select → checkout URL (happy) | **GATED** | `billing::checkout_session_authed` needs `CORELINK_E2E_STRIPE_TEST=1` + a Clerk session bearer — never set by the provision script. tier_select is Clerk-session auth, not PAT. |
| tier-select **unauthed → denied** | **REAL** | `billing::checkout_unauthed_denied` (cred-free negative). |
| Stripe webhook signature **forgery → 400** | **GATED** (soft) | `billing::webhook_unsigned_rejected` runs only if `CORELINK_E2E_SIGNUP_WORKER_ENDPOINT` set; 503 (unconfigured) gates. Not in provision script. |
| Stripe webhook → **tier upgrade reflected** | **GATED** | `billing::webhook_tier_upgrade_simulation` needs the whsec + sub/cust ids; write-only secret means a real run almost always 400-gates. |
| Cancel webhook → **entitlement revoked on data plane** | **GATED** | `billing::webhook_subscription_cancel_simulation` — same gate; also leaves tenant canceled (must be throwaway). |
| Billing portal hand-off | **GATED** (soft) | `billing::billing_portal` — 404 for a non-Stripe tenant gates (the e2e signup tenants have no Stripe customer). |
| past-due → data-plane **denied** | **GATED** | `billing::past_due_data_plane_denied` needs `CORELINK_E2E_PAT_PASTDUE` — never provisioned. |
| Quota hard-cap → clean 402/429 | **GATED** | `quota::*` needs `CORELINK_E2E_QUOTA_TEST=1` + a NEAR-LIMIT tenant. A fresh e2e tenant is nowhere near its cap, so even with the flag it reports "cap unobserved". |
| Quota under-cap **serves** | **REAL** | `quota::under_cap_serves` (the positive half runs). |
| Runners-tier purchase → `runners_entitlement` seed | **ABSENT** | no journey buys the Runners add-on or asserts the entitlement seed. |
| **Dunning / past_due → recover → active** | **ABSENT** (live) | the matrix lists it; only the upgrade webhook sim exists, no recover-from-past_due transition. |
| **Cancel → downgrade-to-Free** (served as Free after) | **ABSENT** (live) | cancel sim only asserts a DENY, never that the tenant continues on Free. SIM-ONLY in pilot `test_05` (offboarding) but that's full offboarding, not cancel→Free. |
| **Refund / chargeback / proration** | **ABSENT** | no journey, no sim. |
| **Upgrade / re-purchase** (Solo→Pro mid-life) | **ABSENT** | no tier-change-while-active journey. |

### Daily use
| Stage | Status | Where / why |
|---|---|---|
| Real CI run: warm cache → fast rebuild (Bazel) | **GATED** | `bazel::*` needs `CORELINK_E2E_RUN_SLOW=1` + bazel on PATH. Real-client `probe_bazel` does a raw REAPI blob round-trip (REAL) but not a 2-build hit. |
| Parallel builds (concurrency correctness) | **REAL** | `concurrency::*` (same-blob fan-out, distinct-blob, read-during-write). Strong. |
| Cache miss→hit round-trip per surface | **REAL** | cas/ac/turbo/oci; cargo/npm/pip/brew are GATED or public-only (raw-HTTP can't drive WebDAV). |
| **Runner admit → over_cap → reject** | **ABSENT** | introspect journeys cover happy/deny of the *introspect endpoint* only. No journey drives the concurrency-entitlement admit/reject the runner fabric enforces (Runners = separate entitlement axis, memory: Option B). |
| Dashboard read APIs (usage/overview/billing/keys) | **REAL** | `dashboard::*`, incl. empty-period edge + RO-keys-deny. |
| Dashboard **leases / history** surfaces | **ABSENT** | no journey reads a leases or build-history surface. |

### Multi-tenant
| Stage | Status | Where / why |
|---|---|---|
| Isolation NEGATIVE (B can't read A) | **REAL** | `security::cross_tenant_isolation` (per surface, with secret-byte leak check), `cas::tenant_isolation`, real-client `probe_native_cas` isolation, dashboard tenant-scoping, tenant-path-spoof. **Excellent.** |
| **Positive shared-cache value (A→`_public`→B HIT)** | **ABSENT** | the network-effect moat — the entire economic thesis — is NEVER positively tested. Adapters only assert a `_public` GET returns 200 (no 502); nothing proves tenant A's public write is HIT by tenant B (dedup/cross-tenant warm). |
| `_public` poisoning resistance | **REAL** | `abuse::cache_poison_public_shared_rejected`. |

### Offboarding / compliance
| Stage | Status | Where / why |
|---|---|---|
| DSR account-delete request → accepted | **GATED** | `dsr::dsr_request_then_content_gone` / `dsr_full_flow_self_driven` need a Clerk DSR session + a throwaway tenant + flags. Never provisioned. |
| Erased content → 410 Gone (tombstone) | **GATED** | `dsr::erased_read_is_gone` needs `CORELINK_E2E_TOMBSTONED_HASH` (an already-erased fixture). Never provisioned. |
| Erasure verification (batch-read fails closed) | **GATED** | `dsr::batch_read_erased_is_gone` — same fixture gate. |
| Internal erase NOT customer-reachable | **REAL** | `dsr::internal_erase_not_customer_reachable` (cred-free deny). |
| Full account-deletion flow + legal-hold preserve | **GATED** / **SIM-ONLY** | `dsr::account_deletion_flow_gated` is a permanent gate (internal transport). SIM in pilot `test_04`. |
| **30-day grace before purge** | **SIM-ONLY** | pilot `test_05` (in-memory: premature-finalize refused, complete after 30d). Never against live. |
| **Tenant offboarding → zero residual** | **SIM-ONLY** | pilot `test_05` in-memory only. |
| **Data export** (user-driven, GDPR Art.20) | **ABSENT** | only the *audit* export exists; no customer DATA export journey. |
| Audit-log export + offline chain re-derive | **REAL** (shape) / **SIM** (full chain) | Rust `audit::*` re-derives row SHAPE black-box (the customer sink, not the request chain). Full HMAC chain integrity is SIM-only (pilot `test_03`). |
| `run.sh` end-of-run DSR-deletes its test users | **REAL** | the only place a live Clerk DELETE→erasure fires (cleanup trap). But it asserts nothing about the erasure outcome. |

### Personas (provisioned + driven?)
| Persona | Driven in a normal run? | Why |
|---|---|---|
| P1 RW | **YES** | provisioned. |
| P2 RO | **YES** | provisioned (`customer_key cache:read`). |
| P4 Revoked | **YES** | provisioned (create→revoke). |
| P6 TenantB / cross-tenant attacker | **YES** | 2nd Clerk user provisioned. |
| P3 Admin | **NO** | `CORELINK_E2E_PAT_ADMIN` never set → admin-surface journeys gate. |
| P5 Expired | **NO** | no expired-PAT mint path in the script → all P5 deny-cells gate. |
| P7 Free / P8 Solo / P9 Pro / P10 Enterprise | **NO** | no tier-specific PATs provisioned → tier-limit journeys gate (all e2e tenants are whatever the signup default tier is). |
| P11 PastDue | **NO** | no past-due tenant provisioned → the billing-integrity deny gates. |
| P12 Anonymous | **YES** | no token needed (deny probes). |
| Header-forgery attacker (matrix P11) | **PARTIAL** | `abuse::native_plane_forgery_denied` covers garbage/tampered/cross/oversized bearer, but the matrix's `x-corelink-tenant-id`/`x-corelink-scope` header-INJECTION strip is **not** a dedicated journey. |
| Legal-hold tenant (matrix P12) | **NO** | only SIM (pilot). |

**Note:** the persona enum in `personas.rs` (P1..P12) does NOT match the
JOURNEY-MATRIX.md persona table (also P1..P12 but different roles — e.g. code-P11
= PastDue, doc-P11 = header-forgery). Drift between the plan and the code.

---

## Prioritized list of missing customer journeys (ranked, with WHY)

1. **Positive shared-cache HIT across tenants (A writes `_public`/public dep → B gets a HIT).**
   WHY: this IS the product's economic thesis and network-effect moat (CLAUDE.md:
   "more customers → fuller cache → faster+cheaper for everyone"). It is the single
   most load-bearing value claim and there is ZERO positive test of it. We test
   that B *cannot* read A's private bytes 6 ways; we never test that B *can* and
   *should* get A's public bytes as a HIT.

2. **Webhook→tier transition driven for real (upgrade AND past_due-recover AND cancel→Free).**
   WHY: the money path. Today every webhook journey GATES because the whsec is
   write-only and the provision script never supplies sub/cust ids. The "billing
   served without payment" launch-blocker (memory) can regress and no green run
   would catch it. Needs a dedicated Stripe-test tenant wired into the provision
   script with its sub/cust ids + the test whsec.

3. **Tier-plan + past-due + expired persona provisioning.**
   WHY: 8 of 12 personas never run. The tier-limit, past-due-deny, and
   expired-PAT-deny assertions are all dark. Provision a Free, a paid, a
   past-due, and an expired PAT in `provision-and-run-suite.sh` (the expired one
   needs a short-TTL mint path or a clock fixture).

4. **Team-invite + multi-seat lifecycle.**
   WHY: SMB teams are the target buyer; "invite a teammate" is table-stakes
   onboarding and is completely untested (the route builders exist but no journey
   calls `POST /v1/customer/team/invite`, no second-seat-under-one-tenant flow,
   no seat-removal). A broken invite path ships invisibly.

5. **Runner admit → over_cap → reject (concurrency-entitlement enforcement).**
   WHY: Runners is the post-launch expansion engine and billing = concurrency-flat
   (memory). The introspect journeys only prove the *introspect endpoint* answers;
   nothing proves the runner fabric actually ADMITS under the cap and REJECTS over
   it. This is where revenue leakage (free unlimited concurrency) or false-rejects
   (angry customer) would hide.

### Secondary holes (worth a line each)
- DSR live end-to-end is fully gated: no provisioned tombstone fixture, no DSR
  Clerk session → the GDPR erasure end-state is never asserted against live.
- Offboarding / 30-day grace / data-export are SIM-only (in-memory), never live.
- Quota hard-cap effectively never observed (needs a near-limit tenant; provision
  script makes fresh, far-below-cap tenants).
- Header-injection priv-esc (`x-corelink-tenant-id`/`-scope` strip) has no
  dedicated journey despite being a known attack class (memory: admin priv-esc).
- cargo/sccache, npm, pip, brew cross-tenant + write paths are GATED (raw-HTTP
  can't drive WebDAV) — only the real-client `run.sh` exercises cargo for real,
  and that GATES on this Mac's sccache TLS bug, so it rarely actually runs.
- `corelink-pilot` / `signup-flow` persona/role drift vs the matrix doc.
