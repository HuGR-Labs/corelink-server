# Code findings — OKF final multi-lens audit (2026-06-28)

**For:** the CoreLink repo tech lead.
**From:** OKF wiki audit (10-lens final pass). These are **code** items surfaced while auditing
the architecture wiki; the wiki itself has been corrected separately (commit `0c44977b` on
`feat/okf-sota-on-main`). This report touches **no code** — it hands the code items to you to decide
and implement.

Severity legend: **HIGH** = correctness/compliance risk that can ship a real failure; **MED** =
latent / defense-in-depth gap; **LOW** = hygiene / latent footgun. One HIGH, one MED, three LOW.
(CF-5 added 2026-06-28 from the post-merge severe independent audit.)

---

## CF-1 [HIGH · GDPR/compliance] — DSR erase-set is incomplete; `VerifiedComplete` over-attests

**Where:** `crates/corelink-container/src/routes/dsr/adapter_d1.rs`
- `TENANT_ID_TABLES` (the per-tenant DELETE set, ~`:54-79`)
- `remaining_rows()` (the post-erase verification sweep, ~`:174-186`)
- attestation/decision: `crates/corelink-container/src/routes/dsr/orchestrator.rs:503-552`
  (emits `ErasureDecision::VerifiedComplete` + `completed.v1` when `failed_count == 0`)

**What's wrong:** the erase-set was ratified with ADR-S11-013 (2026-06-11) and **was not updated as
new tenant-keyed tables landed afterward.** Concretely, `team_member` (migration **0074**, ADR-S33-001)
stores `tenant_id` + a **raw Clerk `user_id`** + `email_hash` — and it is in **neither** the DELETE set
**nor** the `remaining_rows` verification scan. So on a GDPR Art.17 erasure for a *team* tenant, the
seat PII **survives**, while the pipeline still emits `VerifiedComplete` (the verifier never queries the
table, so `failed_count` stays 0). **This is an over-attestation of a compliance guarantee** — the worst
shape of erasure bug (silent + certified-complete).

**Class bug (not a one-off):** the same omission affects every tenant-keyed table added after the
2026-06-11 freeze that wasn't back-added to the set —
`session_exchange_throttle.clerk_sub` (0068, a principal id), `pilot_tenants` (0065),
`runners_entitlement` (0070), `monthly_request_counts` (0071). None are in the erase-set **or** the
`RETAIN_SET`.

**Why the guard didn't catch it:** the only drift test (`adapter_d1.rs:259-270`) checks for
**duplicates within the hardcoded list** — it does **not** enumerate the migration tables, so it
structurally cannot detect an *omitted* table. (`RETAIN_SET` is `#[cfg(test)]`-only, so there is no
runtime completeness assertion either.)

**Recommended fix (your call on policy edges):**
1. **Classify** the ~14 post-ratification tenant-keyed tables per the existing **ADR-S11-013** policy
   (the policy already exists — most are mechanical erase-vs-retain decisions; `abuse_score_history`
   is the one genuine retain-vs-erase **owner/legal edge-call**). Add the erase-set members to
   `TENANT_ID_TABLES` **and** the `remaining_rows` scan.
2. **Add a runtime migration-vs-erase-set drift gate** — enumerate the tenant-keyed tables from the
   migrations (or a registry) and assert every one is classified erase-or-retain; fail closed (and/or
   a CI test) so a *future* tenant-keyed table can't silently escape erasure again. Promote `RETAIN_SET`
   out of `#[cfg(test)]` so the assertion runs in the real build.

**Verification after fix:** a team-tenant erasure leaves 0 rows in `team_member`/the new tables; the
drift gate REDs if any migrated tenant-keyed table is unclassified.

---

## CF-2 [LOW · hygiene] — OCI quota-gate source comment is stale (says "write methods")

**Where:** `crates/corelink-container/src/routes/oci.rs` — comment ~`:807-810` (and the older
`:754-759` region) says the flat per-op cost is charged "on **write** methods."

**Reality:** the live `oci_quota_gate` (~`:835-872`, the rt-nuclear cycle-2 #3 change) now charges the
**$-ceiling on every method including reads** (fail-CLOSED 402); only the request-**count** axis stays
fail-open (429). The code is correct; the **comment lies** and will mislead the next reader. Fix the
comment to match the read-charging behavior. (The wiki gotcha in `surfaces/public-packages` has already
been corrected to distinguish the two axes.)

---

## CF-3 [LOW · correctness footgun] — eviction `Tier` enum can't parse the sold tier slugs

**Where:** `crates/corelink-eviction/src/tier.rs` (the eviction `Tier` enum).

**What's wrong:** the eviction enum is a **legacy 5-arm** `{Free, Solo, Team, Business, Enterprise}`
with **no `FromStr`/`TryFrom`** — but the sold taxonomy (ADR-S19-001) is 6-tier
`{Free, Solo, Starter, Pro, Max, Enterprise}`. So the live `tier_selections.tier` strings
`"starter"/"pro"/"max"` **map to no eviction TTL** — they fall to whatever the unknown-slug branch does
(likely the `free`/shortest-TTL floor). A paying Pro/Max tenant may therefore get **free-tier eviction
TTLs**. (`request_count.rs` does cover the 6-tier slugs, so this is **eviction-side only**.)

**Recommended:** either (a) map the sold `starter/pro/max` slugs to their intended eviction TTLs, or
(b) if the legacy 5-arm domain is deliberate, add an explicit `FromStr` with a documented, *non-free*
default for unknown paid slugs — and a test pinning each sold slug → an intended TTL. (The wiki now
discloses the split as a real coverage gap in `ops/gc-eviction`.)

---

## CF-4 [LOW · latent footgun] — `check_and_accrue` default trait impl is a non-atomic TOCTOU two-step

**Where:** `crates/corelink-container/src/tenant_quota.rs` — the `QuotaStore::check_and_accrue`
**default trait method** (~`:250-271`).

**What's wrong (today: not exploitable):** production wires the **D1 store override** (single atomic
`accrued + delta <= budget` statement) via `quota_guard_from_env` (`:112-130`), fronted by
`LeasedQuotaStore`. The **default** impl, however, is a read-then-write two-step with a TOCTOU window —
safe only because nothing in prod uses it. If a **future** non-D1 quota backend is added and forgets to
override `check_and_accrue`, it would **silently over-admit spend past the $-ceiling**.

**Recommended:** make the default impl fail-closed (or `unimplemented!()`/compile-error) so a new
backend must consciously provide an atomic accrue, rather than inheriting a silent over-admit. (Note:
the leased layer over-**charges** but never over-**serves**, and the audit verified the live D1 path is
sound — this is purely about protecting future backends.)

---

## CF-5 [MED · latent residency / Schrems II] — `sam` macro is request-time routable into the US bucket

**Where:** `crates/corelink-container/src/storage/region_map.rs:60` (`colo_for_macro("sam") => Some("sam")`)
+ `wrangler.toml:638/646` (`env.PROD_SAM` CAS bucket = physically `corelink-cas-prod`, the US bucket)
+ `r2_s3.rs:453-456` (a `sam` tenant's blobs are only key-prefixed `sam/...` *inside* the US bucket).

**What's wrong (today: not reachable via product):** the round-3 Schrems II fix closed the **provisioning**
path — signup rejects `sam`/`apac`/`afr` (`apps/signup-worker/.../clerk.ts:743-751`) and `primary_region` is
immutable post-INSERT — so a `sam` tenant cannot be created through product ingress. But the **request-time**
path is NOT fail-closed: `colo_for_macro("sam")` still returns `Some("sam")`, routing a `sam` tenant to
`env.PROD_SAM`, whose CAS bucket is physically the **US** `corelink-cas-prod`. So a `sam` tenant that exists
**only via an out-of-band D1 row** (not via signup) would have its blobs stored in a US bucket while presenting
as South-America-resident — the exact cross-border placement the residency model forbids. `region_map.rs:30-34`
own doc-comment already admits this. `afr`/unknown fail-closed correctly (`colo_for_macro` → `None` → 503).

**Severity MED / latent / defense-in-depth:** unreachable via product ingress (provisioning rejects `sam`),
and consistent with the **ADR-S14-009 single-bucket launch posture** (at launch, all CAS physically lives in
one US bucket regardless of macro — physical per-region is deferred to S-14). So this is not a live leak today;
it's a missing request-time guard that would matter the moment `sam` becomes provisionable or a `sam` row is
injected by any non-signup path.

**Recommended (one line):** make `colo_for_macro("sam") => None` (like `afr`) so the request path fail-closes a
non-provisioned macro too — matching the provisioning-side rejection. (Then re-confirm the worker mirror
`worker/src/region-map.ts` stays byte-for-byte consistent.) The OKF `compliance/data-residency` concept already
discloses the single-bucket posture; no wiki change needed.

---

> **Status (2026-06-28):** CF-1 (GDPR erase-set) + CF-2/3/4 were implemented in code by #541
> (`0150de7e`). CF-5 (sam) remains open. The findings below (CF-6…CF-9) are NEW, from the
> post-merge **severe brutal audit** (12-finder + adversarial-verify pass on current `main`).

## CF-6 [HIGH · compliance/integrity] — the audit chain is forgeable by a D1 writer (un-keyed BLAKE3, drain-time seal, unwired Object-Lock)

**Where:** `crates/corelink-audit-chain/src/chain.rs:156,168` (`Hasher::new()` — plain **un-keyed** BLAKE3,
no `keyed_hash`/HMAC) + `crates/corelink-container/src/routes/audit_drain.rs:208-235` (the seal runs at the
**hourly** drain, reading rows back from D1) + `crates/corelink-audit-chain/src/lib.rs:89-92` (the R2
Object-Lock binding the chain's append-only durability depends on).

**What's wrong:** the OKF `compliance/audit-chain` concept sells SOC2-grade tamper-evidence ("an audit log a
customer/auditor can trust **even if CoreLink itself is compromised** … append-only … CC7.2"), but the code
does not deliver that against an **insider with D1 write** (per CLAUDE.md, `.env.local`'s
`CLOUDFLARE_API_TOKEN` has D1 read/write):
1. Audit rows are written **plain/unchained** to `audit_outbox`; the chain link is computed **later** at the
   hourly drain — so a D1 writer can alter/delete a row in the **≤1h pre-seal window** (unbounded while the
   drain cron is inert without the key) and the next drain seals a **valid** chain over the forged content.
2. The chain hash is **un-keyed** BLAKE3 — so even **post-seal**, a D1 writer can recompute every downstream
   `chain_hash`/`prev_hash` **and the head** to forge a fully self-consistent chain; the plain-BLAKE3 verifier
   passes. There is no secret the forger lacks.
3. The storage-level immutability that would stop this (**R2 Object-Lock**) and the external witness (**Rekor**)
   are both **deferred / in-memory fakes**.

So the tamper-evidence is real only against an **external** reader who already has an independent trusted copy
of the head — NOT "even if CoreLink is compromised." (The OKF wiki is being corrected to say
tamper-EVIDENCE-at-verify, not tamper-proof-against-insider; this CF is the **code** half.)

**Recommended:** (a) **key the chain** — HMAC-BLAKE3 (`keyed_hash`) or **Ed25519-sign the sealed head** with a
secret the D1 writer lacks (a Worker-only signing key), so a forged chain can't be made self-consistent;
(b) **chain at write-time** (or shrink the pre-seal window) so rows are linked before they can be mutated;
(c) **wire the R2 Object-Lock** immutability the INV-AUDIT-APPEND-ONLY invariant assumes. Until then, do not
represent the audit trail as insider-tamper-proof to an auditor.

## CF-7 [LOW · GDPR transparency] — the legal sub-processor disclosure omits Clerk

**Where:** `legal/sub-processors.md` lists **7** vendors and omits **Clerk**, while
`docs/compliance/vendor-reviews/clerk-dpa-review-2026-04.md` exists and the OKF `compliance/sub-processors`
concept correctly lists **8**. A published sub-processor list that omits an active processor is a GDPR Art.28/
transparency gap. **Recommended:** add Clerk to `legal/sub-processors.md` (it's already DPA-reviewed).

## CF-8 [LOW · observability, deferred-façade] — the in-flight span ledger key conflates tier with tenant

**Where:** `crates/corelink-tracing/src/service.rs:44` — `InFlightKey { tenant_tier, span_id_hex }` keys the
ledger by the **billing plan tier** (Free/Solo/Team), not a tenant id, yet the doc-comment + the
`INV-TENANT-ISOLATION` invariant call it per-tenant isolation; the proof test
(`crates/corelink-tracing/tests/prop_tracing.rs:300-304`) forces two **distinct tiers** so it never exercises
the same-tier-two-tenant collision. Two tenants on the same tier share one key namespace → per-**tier**
bucketing. Practically near-zero risk (in-memory deferred-façade + random `span_id`), but the doc-comment +
test are misleading. **Recommended:** rename the field / fix the doc-comment to "tier bucket", and make the
prop-test use two distinct **tenants on the same tier**. (The OKF wiki claim is being corrected too.)

## CF-9 [LOW · erasure robustness] — the CF-1 erase-set drift gate's `KEY_COLS` is a fixed name allowlist

**Where:** the CF-1 migration-vs-eraseset completeness gate keys on a 6-name `KEY_COLS` allowlist
(`crates/corelink-container/src/routes/dsr/adapter_d1.rs`). A **future** migration whose tenant-subject column
is named differently (`subject_hash`, `principal_id`, …) would be **silently unclassified** by both the parser
and the completeness gate while the gate stays green — re-opening the exact CF-1 class. **Recommended:** make
the classifier detect tenant-key columns structurally (FK to `tenant`, or a registry the migration must
update) rather than by a fixed name list.

*(Doc note, not code: `CLAUDE.md` is stale — it says "146 OKF concepts / ~71 crates / 463 specs"; the repo is
now 157 concepts / ~105 crate Cargo.tomls / `validate_specs.py` 466. Worth a one-line refresh.)*

---

## Not in scope here (handled in the wiki, no code change)

The other final-audit findings were **documentation** overstatements and are already fixed on
`feat/okf-sota-on-main` (`0c44977b`): the container-as-2nd-live-Stripe-webhook framing, the
$-ceiling described as exact-atomic (it's leased/approximate but cannot over-serve), the consent-purpose
enum reconciliation, and the edge D1-fault→401 documentation. No code action needed for those.

---

*Generated by the OKF audit. The audit makes no code changes; CF-1 is the only item with real
near-term risk (a certified-complete GDPR erasure that isn't complete) and is the recommended first fix.*
