# CAA-360 remediation — tracked follow-ups (out of the disjoint-WP wave)

The 2026-06-13 remediation wave fixed all 35 actionable confirmed findings in code
(fail-closed + loud), gate-verified (cargo check + clippy -D warnings + worker/signup tsc).
These items were flagged by the WP agents as **out of their file-scope** (architectural,
infra, or cross-file) — captured here so nothing is lost. None is a new vulnerability; they
complete the defense-in-depth / operability around the wave.

## Infra / ops (lead executes on the deploy)
- **Set `R2_TDK_HEX`** (`openssl rand -hex 32`) as a container secret on all 5 prod envs —
  REQUIRED so the secure HMAC tenant-prefix (F1) is active. Enabling it re-keys all CAS/AC
  objects (prefix changes) → accept a cold cache (CoreLink just launched, minimal real data).
- **Per-region CAS buckets** (F7 physical residency): create `corelink-cas-{lhr,sam,nrt,syd}`
  + bind `R2_CAS_BUCKET`/`R2_CAS_REGION` per `[env.prod-<r>]`. The code is now region-correct;
  until the buckets exist, bytes land in `corelink-cas-prod` under a region-correct key.
- **`ENVIRONMENT=prod`** confirmed set on the signup-worker prod env (F9 fail-closed depends on it).

## CI mechanization (close the drift class — F8)
- Add a CI check that greps every container `env::var`/`env_or` name and asserts it appears in
  the `durable_object.ts` `container.start({env})` forward-list → the set-but-not-forwarded
  class fails the build (the gap that hid ERASURE_SALT_KEY / FABRIC / and the audit's F8).
- Add `R2_TDK_HEX`, `ERASURE_SALT_KEY`, `PAT_SIGNING_KEY` as REQUIRED prod secrets in
  `scripts/secrets-checklist-verify.sh` / `validate_secrets_matrix.py` so launch-readiness
  fails loudly if any is missing (the audit's root-cause class: secrets that fail open silent).

## Defense-in-depth polish (code, next pass)
- **F1 route-loudness:** `routes/cas.rs` + `routes/ac.rs` currently fall back to `InMemory` on
  the builder's `Some(Err)` (TDK required). Prefer refuse-to-mount / 503 so a forgotten TDK is
  LOUD, not a silent non-durable cache. (Safe today; with R2_TDK_HEX set there is no Err.)
- **F3:** wire `adapter_pat::PatVerifier` onto the native CAS/AC/Bazel/Turbo plane for an
  in-container PAT re-verify (today the container trusts the Worker-injected, header-stripped
  tenant; the audit confirmed that path is safe, this is belt-and-suspenders).
- **F4:** split per-consumer internal secrets (`CORELINK_INTERNAL_MINT_KEY` distinct from the
  signup key), add CF Access/WAF on `/_internal/*`, per-tenant authz on the mint endpoint.
- **F27:** thread the PAT `can_write` bit through the cargo/brew/npm/pip resolver ports
  (`adapter-host/src/{cargo,brew}/ports.rs`) so writes require scope-header AND can_write, not
  single-layer header trust.
- **F25:** add `OciAdapterError::TooManyOpenSessions` → HTTP 429 (currently surfaces as 500).
- **F26:** document the OCI two-leg pass-through exception in the auth-model spec; consider
  injecting `x-corelink-scope` on `/token`.

## Product seam (when Runners ships)
- **M2 entitlement source:** `auth_introspect.rs::tenant_has_runners_entitlement()` is a stub
  (returns false → `max_concurrency` absent — correct now). Wire the real D1 entitlements
  lookup when the Runners entitlement store lands; the ladder + Option shape are final/ratified.
- Reconcile the Runners slot-tier ladder (starter/pro/team/scale/max) vs the cache `TierKind`
  set — they are separate axes (per the runners handoff); keep `max_concurrency` derived from
  the Runners entitlement, never from the cache plan.
