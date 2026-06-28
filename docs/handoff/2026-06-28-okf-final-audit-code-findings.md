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

## Not in scope here (handled in the wiki, no code change)

The other final-audit findings were **documentation** overstatements and are already fixed on
`feat/okf-sota-on-main` (`0c44977b`): the container-as-2nd-live-Stripe-webhook framing, the
$-ceiling described as exact-atomic (it's leased/approximate but cannot over-serve), the consent-purpose
enum reconciliation, and the edge D1-fault→401 documentation. No code action needed for those.

---

*Generated by the OKF audit. The audit makes no code changes; CF-1 is the only item with real
near-term risk (a certified-complete GDPR erasure that isn't complete) and is the recommended first fix.*
