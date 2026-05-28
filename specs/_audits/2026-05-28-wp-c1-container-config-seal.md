---
id: "WP-C1-CONTAINER-CONFIG-SEAL-2026-05-28"
type: "audit"
doc_status: "SEALED"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-28"
updated: "2026-05-28"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["wp-c1", "p0-wave", "phase-1", "containers", "r2-s3", "egress", "cpu-ms", "wave-33"]
references:
  - "wrangler.toml"
  - "docs/internal/secrets-checklist.md"
  - "scripts/d-day-r2-s3-creds-runbook.md"
  - "specs/_audits/2026-05-28-wave33-p0-remediation-dispatch-matrix.md"
---

# WP-C1 — Container R2-S3 Egress + Creds Config + cpu_ms Raise — SEAL

**Date:** 2026-05-28  
**Author:** Claude Sonnet 4.6  
**Wave:** P0 remediation wave, Phase 1  
**Fixes:** P0-5 (enableInternet false) + P1-3 (cpu_ms too low for HMAC auth)  
**Decision gate:** DECISION-GATE-1 Option A (Owner-approved)

---

## Summary

This WP configures the Cloudflare native container to reach R2 via the S3 API
over egress (DECISION-GATE-1 Option A — no VPC-style internal route exists on
the CF Workers platform). It also raises the `[limits] cpu_ms` cap from 30 to
100 ms to provide headroom for the HMAC auth path (5–15 ms overhead was causing
CPU-Exceeded 1102 errors under load at 30 ms).

No secret values are committed. The runbook at
`scripts/d-day-r2-s3-creds-runbook.md` documents how the Owner mints the R2 S3
token and puts the secrets on D-day.

---

## Changes delivered

### 1. `wrangler.toml` — `[limits] cpu_ms` raised 30 → 100

```toml
[limits]
cpu_ms = 100
```

Rationale: Sonnet P1-3 found that the HMAC auth path adds 5–15 ms overhead.
The previous 30 ms cap caused CPU-Exceeded (error 1102) under sustained load.
100 ms provides 3–6× headroom without removing the runaway-request guardrail.

### 2. `wrangler.toml` — `[[env.prod.containers]]` egress + R2 S3 env refs

```toml
[[env.prod.containers]]
class_name = "CoreLinkServer"
image = "./Dockerfile"
max_instances = 20
instance_type = "standard-1"
enableInternet = true

[env.prod.containers.vars]
R2_S3_ENDPOINT = "https://CLOUDFLARE_ACCOUNT_ID_PLACEHOLDER.r2.cloudflarestorage.com"

[env.prod.containers.secrets]
CLOUDFLARE_ACCOUNT_ID = "CLOUDFLARE_ACCOUNT_ID"
R2_S3_ACCESS_KEY_ID = "R2_S3_ACCESS_KEY_ID"
R2_S3_SECRET_ACCESS_KEY = "R2_S3_SECRET_ACCESS_KEY"
```

Key decisions:
- `enableInternet = true` is scoped exclusively to `[[env.prod.containers]]`
  (the prod block). The default-env `[[containers]]` block is untouched.
- `R2_S3_ENDPOINT` is a non-secret URL template in `[vars]` (plaintext config);
  it contains a placeholder `CLOUDFLARE_ACCOUNT_ID_PLACEHOLDER` that the Owner
  replaces with the real account ID on D-day (see runbook Step 3).
- `CLOUDFLARE_ACCOUNT_ID`, `R2_S3_ACCESS_KEY_ID`, `R2_S3_SECRET_ACCESS_KEY` are
  wrangler secret references — no values here; injected at runtime via
  `wrangler secret put`.

### 3. `docs/internal/secrets-checklist.md` — rows 139 + 140 added

| # | Secret | Env var | Rotation | Owner | Stored at |
|---|---|---|---|---|---|
| 139 | R2 S3 access key ID (container egress) | `R2_S3_ACCESS_KEY_ID` | 90d | SRE Lead | cf-wrangler |
| 140 | R2 S3 secret access key (container egress) | `R2_S3_SECRET_ACCESS_KEY` | 90d | SRE Lead | cf-wrangler |

Total rows updated: 138 → 140.

### 4. `scripts/d-day-r2-s3-creds-runbook.md` — NEW

Full step-by-step runbook for:
- Minting an R2 S3 API token in the CF dashboard (scoped to CoreLink buckets,
  NOT the audit-archive bucket per CTRL-AUDIT-001 / lease-privilege)
- Replacing the `R2_S3_ENDPOINT` placeholder with the real account ID
- `wrangler secret put` for both secrets
- Smoke verification of R2 connectivity post-deploy
- 90-day rotation procedure (create-new → put → deploy → smoke → revoke-old)

### 5. `scripts/validate_secrets_matrix.py` — allowlist entry for `R2_S3_ENDPOINT`

`R2_S3_ENDPOINT` is a non-secret URL template scanned from the
`[env.prod.containers.vars]` section by `WRANGLER_VAR_RE`. It was added to the
allowlist (analogous to `TF_BACKEND_ENDPOINT`, `VAULT_ADDR`, `DT_API_URL`, etc.)
because the URL itself carries no credential material; the account ID component
is supplied at runtime via the `CLOUDFLARE_ACCOUNT_ID` secret (row #49-equiv).

---

## DoD verification

### DoD 1 — TOML parse
```
python3 -c "import tomllib; tomllib.load(open('wrangler.toml','rb')); print('OK')"
→ OK ✅
```

### DoD 2 — `[[env.prod.containers]]` has `enableInternet = true` + R2 S3 env refs
```
python3 -c "
import tomllib, json
d = tomllib.load(open('wrangler.toml','rb'))
c = d['env']['prod']['containers'][0]
assert c['enableInternet'] is True
assert c['secrets']['R2_S3_ACCESS_KEY_ID'] == 'R2_S3_ACCESS_KEY_ID'
assert c['secrets']['R2_S3_SECRET_ACCESS_KEY'] == 'R2_S3_SECRET_ACCESS_KEY'
assert 'R2_S3_ENDPOINT' in c['vars']
print('DoD-2 OK')
"
→ DoD-2 OK ✅
```

### DoD 3 — `[limits] cpu_ms = 100`
```
python3 -c "import tomllib; d=tomllib.load(open('wrangler.toml','rb')); assert d['limits']['cpu_ms']==100; print('DoD-3 OK')"
→ DoD-3 OK ✅
```

### DoD 4 — secrets-checklist rows for `R2_S3_ACCESS_KEY_ID` + `R2_S3_SECRET_ACCESS_KEY`
```
grep -c "R2_S3_ACCESS_KEY_ID\|R2_S3_SECRET_ACCESS_KEY" docs/internal/secrets-checklist.md
→ ≥ 2 (rows 139, 140 present) ✅
```

### DoD 5 — `validate_secrets_matrix.py` — no new code_only drift
```
python3 scripts/validate_secrets_matrix.py 2>&1 | grep "code_only"
→ validate_secrets_matrix: ... code_only=0 ✅
```
`R2_S3_ACCESS_KEY_ID` and `R2_S3_SECRET_ACCESS_KEY` appear as `matrix_only`
(expected — forward-looking until Owner puts the secrets on D-day).

### DoD 6 — Runbook documents R2 S3 key minting + `wrangler secret put`
```
ls scripts/d-day-r2-s3-creds-runbook.md
→ present ✅
```
Runbook covers: CF dashboard token minting, account ID placeholder replacement,
`wrangler secret put`, smoke verification, 90-day rotation procedure.

### DoD 7 — `validate_specs.py` green
```
python3 scripts/validate_specs.py 2>&1 | tail -1
→ ✅ Todos validados: 450 com schema completo, 11 com YAML only (461 total). ✅
```

### DoD 8 — Only containers block + cpu_ms touched in wrangler.toml
Verified via `git diff wrangler.toml`: only the `[limits]` section (cpu_ms
30→100) and the `[[env.prod.containers]]` block (enableInternet, vars,
secrets) were modified. No routes, bindings, R2 buckets, KV namespaces, D1
databases, or DO bindings were touched.

---

## Acceptance gates (all green)

```
python3 -c "import tomllib; tomllib.load(open('wrangler.toml','rb')); print('OK')"
# → OK

grep -n "enableInternet\|cpu_ms\|R2_S3" wrangler.toml | head
# → shows enableInternet = true, cpu_ms = 100, R2_S3_* vars/secrets

python3 scripts/validate_secrets_matrix.py 2>&1 | grep -E "code_only|matrix_only" | head
# → matrix_only: [R2_S3_ACCESS_KEY_ID, R2_S3_SECRET_ACCESS_KEY, ...], code_only=0

python3 scripts/validate_specs.py 2>&1 | tail -2
# → ✅ Todos validados: ...
```

---

## Blockers / Owner actions required before container can reach R2

1. **Replace `R2_S3_ENDPOINT` placeholder** — update
   `[env.prod.containers.vars]` in `wrangler.toml` with the real 32-hex
   Cloudflare account ID (non-secret; can be committed).
2. **Mint R2 S3 API token** — per runbook Step 2 in
   `scripts/d-day-r2-s3-creds-runbook.md`.
3. **`wrangler secret put R2_S3_ACCESS_KEY_ID --env prod`** — per runbook Step 4.
4. **`wrangler secret put R2_S3_SECRET_ACCESS_KEY --env prod`** — per runbook Step 4.
5. **`wrangler deploy --env prod`** — picks up new secrets + `enableInternet`.
6. **Smoke test** — per runbook Step 7.

These are all Owner/SRE-Lead D-day actions; no code changes required.

---

## Non-scope (not touched by this WP)

- Container crates (`crates/corelink-container/`)
- Worker source (`worker/`)
- All other `wrangler.toml` sections (routes, other env blocks, staging)
- Actually minting or storing real secret values (Owner D-day)
