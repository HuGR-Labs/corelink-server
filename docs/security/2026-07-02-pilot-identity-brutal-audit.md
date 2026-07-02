# Brutal-audit — pilot per-tenant identity + provisioning + resolve-tenant + dual-read (2026-07-02)

Attacker-grade audit of the just-shipped pilot-critical surface, BEFORE the pilot depends on it.
Two independent adversarial passes (identity/isolation; resolve-tenant/dual-read). Verdict + tracked follow-ups.

## Verdict: core isolation is SOLID — pilot may go live with real security confidence
No exploitable bypass on any tested vector:
- **No cross-tenant landing** — the resolved tenant is derived SOLELY from the cryptographically-verified Clerk
  `sub` (`clerk_auth.ts` `verifyGithugrSession` → `deriveGithugrTenantId(sub)`); `audience` is only an
  equality-check (mismatch → 403, never a redirect); no header/JWT-`org`/audience feeds the landing.
- **JWT-verify robust** — RS256-only (no alg-confusion / `none` / HS256), `exp`/`nbf`/`iss` exact-pinned, `azp`
  re-asserted against the allowlist. The dead fixed-`GITHUGR_TENANT_ID` fallback is fully removed.
- **No forgeable-sub squatting** — only the attacker's OWN verified sub is ever provisioned; no request field
  passes a chosen sub; pre-seeding a victim's deterministic tenant_id is inert (can't forge the sub to land).
- **No fail-open** — every verify/D1 error → 401/500 fail-closed; concurrent double-first-login converges
  (deterministic id + `INSERT OR IGNORE` first-writer-wins; read-back returns the same tenant).
- **resolve-tenant** — auth gate is constant-time + length-safe + dedicated-key (≥32, route-not-mounted below);
  parameterized SQL (no SQLi); `deny_unknown_fields`.
- **dual-read** — no cross-email collision (256-bit; the `IN (salted,legacy)` set doesn't widen victim matching;
  unit-tested); salt drift can only fail-NEGATIVE (invite doesn't bind), never false-positive/takeover.

## FIXED in this change (both LOW)
- **H3 — un-throttled provisioning write-amplification** → `provisionOrLookupGithugrTenant` is now LOOKUP-FIRST
  (existing sub → single SELECT, zero writes; provisioning only on first-ever login).
- **H5 — non-atomic 5-row provision** → wrapped in a transactional D1 `batch()` (all-or-nothing; no partial
  row-set lingering). Fail-closed contract unchanged.

## TRACKED FOLLOW-UPS (not pilot-blockers)
- **[MEDIUM] resolve-tenant is a POST-auth enumeration/topology oracle.** Any configured fabric key
  (`FABRIC_INTROSPECT_AUTH_KEY` / `_HUGR`) resolves ANY org → returns the raw `tenant_id`; the 200-vs-404 split
  leaks provisioning state. It is INFO-LEAK only (post-auth, key-gated) — knowing a victim's tenant_id does NOT
  grant write-access (the token derives the tenant from the verified sub, not a passed id). Acceptable for the
  pilot (small trusted consumer set, key held OOB). **Fast-follow hardening:** per-caller key scoping (limit a
  key to its own orgs), and/or collapse 404-vs-503 to avoid the transient-vs-unmapped distinction. The 200-vs-404
  "is-this-org-provisioned" oracle is intrinsic to a lookup endpoint and can't be hidden while remaining useful.
- **[LOW] salt-drift across the 6 EMAIL_HASH_SALT targets** → a mismatched value makes legit invites
  fail-NEGATIVE (never a wrong-row match). Mitigated by the "same value on all 6" instruction; self-heals on
  invite re-fire. Operational: set-once in a quiet window, never rotate (no retro-salt).
- **[INFO] `tenant_org_map` dual-writer** — the signup-worker (CoreLink Clerk, `user.id` fallback) and the
  githugr exchange (`sub`) both write the shared PK `clerk_org_id`. Two independent Clerk instances → a real
  collision needs identical opaque `user_...` suffixes across instances (cryptographically negligible). Inert
  today; noted for awareness.

Audit files-in-scope: `worker/src/lib/{clerk_auth,githugr_provision,session_exchange}.ts`,
`crates/corelink-container/src/routes/auth_introspect.rs`, `crates/corelink-container/src/email_hash.rs`,
`apps/signup-worker/src/{lib/d1,webhooks/clerk}.ts`, `worker/src/index.ts` (resolve-tenant route),
`migrations/d1/0083_tenant_org_map.sql`.
