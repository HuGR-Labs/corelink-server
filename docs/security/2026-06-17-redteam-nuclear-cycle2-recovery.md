# Nuclear red-team — cycle-2 RECOVERY run (2026-06-17, ~4am quiet-Mac)

> Re-run to recover the round-3/4 + kill-chain coverage the prior run's rate-limit cut.
> **Again rate-limited mid-flight** (rounds 3/4 hunters + all 5 kill-chains + completeness critic
> killed by the session limit, resets 9am America/Bahia) — so this is **again a LOWER BOUND**.
> Full structured result (all 25 confirmed + evidence): the workflow task output at
> `…/tasks/wzup4ymbl.output` (114k chars). Run id `wf_2a4812d8-15b`.

## Summary
- Boundaries mapped: **17** · rounds run: **4** (3/4 partial) · candidates: **80** · **confirmed exploitable: 25**
- Severity: **1 CRITICAL · 20 HIGH · 2 MED · 2 LOW**
- Reachability: leaked-secret-only **6** · any-free-tenant **15** · authed-tenant **4**

## Top confirmed (full detail in the output file)

### 🔴 CRITICAL — leaked shared key = mint-admin + mass-erase + tier-downgrade ransom chain (4/5)
One leaked **`CORELINK_INTERNAL_AUTH_KEY`** unlocks the whole privileged `/_internal/*` family because
mint (`internal_pat.rs:698`), admin (`admin.rs:399-426`), CAS-erase (`main.rs:357`) all **fall back to
the shared key**, and **DSR (`dsr.rs:240`) + CAS-erase don't honor their dedicated keys at all**.
Chain: mint any-tenant admin PAT → `/_internal/dsr/erase` or repeated `/_internal/cas/<v>/<h>/erase`
(irreversible 410 across regions) → `/_internal/admin/mutate` tier-downgrade → ransom.
**This is the exact blast radius #297 claimed to close — intact** because the per-consumer secrets
(#156/#157/#158) are not all provisioned (live state = shared key only) AND erase/admin don't enforce
their dedicated keys. **Fix (W-F4a/b/c, all OPEN):** (1) take `/_internal/*` off the public internet
(CF Service Binding from signup-worker); (2) provision the per-consumer secrets + **remove the shared
fallback for the destructive consumers** (erase/admin) — fail-closed if dedicated key absent;
(3) per-tenant authz on erase/DSR (require a D1 `dsr_requested` row written only by the Clerk
`user.deleted` path — never erase on a body-asserted `tenant_id`) + an allow-scope on mint; (4) rate-limit
+ anomaly-alert the erase surfaces. Immediate compensating control: rotate the shared key.

### 🟠 HIGH — OCI push bypasses the per-tenant monthly $-ceiling entirely (5/5)
The Worker OCI pass-through (`index.ts:1716-1752`) `stripClientTrustHeaders` + **`h.delete('x-corelink-tenant-id')`**
and returns early **before** the quota block (`index.ts:2024-2102`), so the Worker $-gate never runs for
OCI. In the container, `oci_quota_gate` (`oci.rs:614-635`) only calls `gate.check(tenant)` when
`!tenant.is_empty()` — but the header was stripped and the OCI route never re-inserts it → **the
$-ceiling check is always skipped**. Any Free-tier PAT with OCI push → unbounded billable OCI ops
(manifest/upload-leg/finalize), zero-byte PATCH legs uncapped on BOTH $ and storage. Directly attacks
the ~80% margin (ADR-0068 cost tripwire inert on OCI). Monthly request-count cap is Worker-edge-only →
also never runs for OCI. **Fix:** resolve the real tenant for the OCI $-gate — either the Worker sets a
**server-trusted** `x-corelink-tenant-id` on the OCI branch (from the PAT it already resolved), or the
container's OCI route re-derives tenant from the bearer/PAT before `oci_quota_gate` (it already resolves
tenant at the `/v2/token` leg — thread it). Make the gate **fail-closed** when tenant is empty.

## The other 23 confirmed (HIGH×18 + MED×2 + LOW×2)
In the output file — clusters observed: OCI/registry quota+request bypass family, Turbo PUT
char/charge gaps, brew/npm/pip manifest, multi-region byte under-count, CAS idempotent re-PUT charging,
rate-limiter init, read-only-PAT capability, storage under-count. **Dedupe pending** vs the 39 in
`2026-06-16-redteam-nuclear-cycle2.md` (several look like the same B/C/F byte-accounting + REQUEST_QUOTA
fail-open families already partially fixed in #300/#301/#302 — must confirm net-new vs re-stated before
fixing).

## Next (resume after the 9am Bahia session reset)
1. Read the full output file; **dedupe** all 25 vs the prior 39 → net-new set.
2. Fix ALL net-new (no severity floor), SOTA, root-cause. **CRITICAL ransom chain** = the W-F4 family
   (code parts: remove shared fallback for erase/admin + per-tenant authz; owner parts: provision
   #156/#157/#158 + the Service Binding). **OCI $-bypass** = a clean code fix (tenant resolution).
3. Verify LOCAL (clippy -D + tsc + tests, `CARGO_BUILD_JOBS=2`, toolchain on PATH, no `--all-features`)
   — gated on a quiet Mac + the ~98% disk being relieved (the os-error-2 amplifier).
4. Minimal PRs, cold-review, merge on local-green, re-run nuclear (rounds 3/4/chain still uncovered).
