# CoreLink user-simulation test suite — MASTER gap map (consolidated)

> **2026-06-23/24.** Consolidates three independent read-only audits (the 3
> kagebunshin) of our user-journey / user-simulation suites:
> - [`gapmap-surfaces.md`](2026-06-23-gapmap-surfaces.md) — protocol/operation coverage per surface.
> - [`gapmap-journeys.md`](2026-06-23-gapmap-journeys.md) — customer-lifecycle / persona coverage.
> - [`gapmap-quality.md`](2026-06-23-gapmap-quality.md) — assertion strength / false-confidence.
>
> Deduplicated + ranked by blast radius. Brutally honest by mandate: the failure
> mode we refuse is *claiming coverage we don't have*.

## The one-paragraph truth

The suites are **broad in shape and thin in what actually runs+asserts**. Two
structural problems dominate everything else:

1. **GREEN-by-vacuum.** The journey runner exits 0 unless `fail>0`, and a normal
   run GATES most journeys (only 4 of 12 personas are provisioned; Stripe/DSR/quota
   creds never supplied). A "GREEN" can mean *"ran almost nothing."* The real-client
   `run.sh` likewise SHIPs when bootstrap finds no Clerk secret. This is precisely
   the owner's "green CI ≠ validated."
2. **The moat has zero positive proof.** The single load-bearing product claim —
   *team A's public dep is served to team B as a cache HIT* (network-effect, the
   COGS+margin story) — is **not tested anywhere**. We prove the negative (B can't
   read A's private; `_public` can't be poisoned) six ways; we never prove the
   positive that the whole thesis rests on.

Everything below is real but ranks under those two.

---

## Consolidated findings — ranked by blast radius

Severity = customer/revenue/compliance impact if it regressed undetected.
"Seen by" = which audit(s) flagged it (S=surfaces, J=journeys, Q=quality).

### 🔴 P0 — false confidence + the moat (fix before trusting any "green")

| # | Gap | Seen by | Why it's P0 |
|---|---|---|---|
| **M1** | **Runner exits 0 / banner GREEN even when every journey GATED** (`main.rs:115-132`); `run.sh` SHIPs on no-Clerk-secret bootstrap (`run.sh:117-124`) | Q | A green run can have asserted ~nothing. Undermines trust in *all* the coverage below. Fix: a floor — N positive assertions must run or the suite RED. |
| **M2** | **Positive cross-team `_public` dedup HIT (A writes → B HITs) — ABSENT in every suite** | S, J | The entire economic/network-effect thesis is unproven. A regression that silently per-tenant-isolates public bytes passes every test and kills the margin invisibly. |
| **M3** | **`expect_denied` accepts 404** (`harness.rs:326`, ~50 sites) — a broken/renamed/unmounted route scores as "secure" | Q | Security probes (edge-mint, priv-esc, internal introspect) become structurally un-failable; they assert a route's *absence*, never its *gate*. |
| **M4** | **Webhook→tier transitions all GATED** (upgrade / past_due-recover / cancel→Free) — whsec write-only, sub/cust ids never provisioned | J, Q | The "billing served without payment" launch-blocker can regress with zero green-run signal. The money path is dark. |

### 🟠 P1 — real-client fidelity + money/runner value paths

| # | Gap | Seen by | Why |
|---|---|---|---|
| **M5** | **real-client suite is curl-not-CLI for cas/bazel/turbo/identity/brew**; only docker + cargo/sccache use a real binary; shell paths never compare bytes | S, Q | The documented "curl passes while the real CLI fails" trap. "Simulate a user" that isn't the user. |
| **M6** | **`run.sh` grades a known-bad 502 as PASS** (`brew auth PASS … HTTP 502` in committed `last-run.json`) — a 502 is the exact `_public` fail-closed signature the Rust suite calls a hard FAIL | Q | Contradictory verdicts across suites; a real outage shape is rubber-stamped SHIP. |
| **M7** | **Bazel `findMissingBlobs` + Bazel AC read/write — never exercised** (real round-trip GATES without the bazel CLI) | S, J | `findMissingBlobs` is the FIRST op every `bazel build` hits; AC is what makes a build a HIT not a re-run. We test a raw blob PUT/GET, not what makes Bazel fast. |
| **M8** | **Runner admit → over_cap → reject (concurrency-entitlement) — ABSENT**; no journey buys the Runners add-on or asserts the `runners_entitlement` seed | J | Runners = the expansion engine, billing = concurrency-flat. Revenue leakage (free unlimited) or false-rejects (angry customer) hide here. Ties to the seed handler just shipped. |
| **M9** | **CAS batch plane (`batch` write, `batch-exists`) — helpers exist, no journey calls them**; `batch-read` only touched negatively (DSR) | S | The hot path built specifically to fix the 3–7s CAS blocker (bulk PUT + budget-lease). Zero real-user coverage on the perf/correctness ops. |
| **M10** | **8 of 12 personas never run** (P3 Admin, P5 Expired, P7-P10 tiers, P11 PastDue) — provision script feeds only P1/P2/P4/P6 | J, Q | Tier-limit, past-due-deny, expired-PAT-deny, admin-surface assertions all dark. |

### 🟡 P2 — assertion strength + lifecycle holes (real, lower blast radius)

| # | Gap | Seen by |
|---|---|---|
| M11 | `ac.rs::divergent_body_reput` named "→ 409 integrity guard" but accepts BOTH 409 and last-write-wins (tautology) — advertises AC-poisoning coverage it doesn't have (`ac.rs:141,179-199`) | Q |
| M12 | No real npm/pip/turbo client (brew never installs); npm tarball integrity (SHA1/SHA512) reject path never driven | S |
| M13 | Team-invite + multi-seat lifecycle ABSENT (`POST /v1/customer/team/invite` route exists, no journey) — SMB teams are the target buyer | J |
| M14 | DSR live end-to-end fully GATED (no tombstone fixture, no DSR session) — GDPR erasure end-state never asserted live; offboarding/30-day-grace/data-export SIM-only (in-memory) | J |
| M15 | Turbo happy-path self-disabled via `turbo_storage_finding_gate` over a known live 500 — a paying Turbo customer's core flow is un-asserted | S, Q |
| M16 | `audit.rs` passes on empty rows; `rate_limit_present` can't detect an absent limiter; `sleep 4` revoke race; introspect HTTP-000 short-timeout class | Q |
| M17 | OCI chunked PATCH / `tags/list` / `_catalog`→401 / RO-push-deny / cross-tenant repo isolation — all absent; CAS list (D-8) + AC refs list (D-7) contract ops untested | S |
| M18 | Quota hard-cap effectively never observed (fresh tenants are far below cap); header-injection priv-esc (`x-corelink-tenant-id/-scope` strip) has no dedicated journey | J |
| M19 | pilot-onboarding + signup-flow are in-memory fakes wearing an "e2e" name — must NOT count as real-user coverage; real-Stripe path is `#[ignore]` | S, J, Q |

---

## What is genuinely strong (so the map isn't misread)

- `cas.rs` / `ac.rs` / `concurrency.rs` do **real byte-for-byte round-trips** (PUT then GET-and-compare).
- The entire `oci.rs` J1–J9 suite asserts **real protocol + Content-Length byte-equality** — the best-built journey set.
- `pat_lifecycle.rs` revoke-deny **correctly refuses 404** (the model fix for M3).
- Cross-tenant **isolation (negative)** is excellent — per-surface, with secret-byte leak checks.
- `cargo`/`sccache` is the **best-covered adapter** (real client, rich journeys) when the Mac's sccache TLS bug doesn't gate it.

---

## Recommended closure order (cheapest-trust-per-unit first)

1. **M1 + M3** (harness honesty): floor-assertion gate so GATED-everything goes RED; split `expect_denied` so security probes require a real 401/403 (not 404). *Small, unblocks trusting everything else.*
2. **M2** (the moat journey): provision 2 tenants, A PUTs a deterministic public artifact → B GETs = **HIT** (native CAS + the adapters that dedup to `_public`). *The single highest-value missing test.*
3. **M10 + M4 + M8** (provisioning + money + runners): extend `provision-and-run-suite.sh` to mint the missing personas + a Stripe-test tenant (sub/cust ids + test whsec) + a Runners-tier purchase asserting the entitlement seed.
4. **M5 + M6 + M7** (real-client fidelity): drive cas/bazel/turbo with real clients (or honestly relabel as synthetic); fix the 502-as-PASS grader; add `findMissingBlobs` + Bazel AC.
5. **P2 batch** (M9, M11–M19) as a follow-up wave.

> Nothing here is a *prod* bug — these are gaps in what we can *prove*. The product
> passed 6 Opus security/code reviews (0 Critical/0 High). This map is about closing
> the distance between "ships correctly" and "we have a test that proves it stays correct."
