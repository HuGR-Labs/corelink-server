---
id: "AUDIT-2026-05-16-SECRETS-MATRIX-TIGHTEN"
type: "audit_report"
doc_status: "FROZEN"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "secrets", "soc2-cc6.1", "r-prep", "wave-20", "small-closure", "validator-tighten"]
---

# Secrets Matrix Tighten — Wave-20 Small-Closure (3-Secret Triage)

> **Audit Date:** 2026-05-16 · **Branch:** `wt/r-prep-secrets-matrix-tighten` · **Lane:** R-PREP (release-prep) · **Wave:** 20 small-closure
> **Reviewer:** Gustavo Schneiter
> **Files touched:** `scripts/validate_secrets_matrix.py`, `docs/internal/secrets-checklist.md`
> **Base commit:** `2eec064` (main, post-wave-19 merge)
> **Cross-ref:** `specs/_audits/2026-05-15-secrets-coverage-baseline.md`, `specs/_runbooks/RB-SECRETS-DRIFT.md`, `docs/internal/secrets-runbook.md`
> **Disposition:** Closes the wave-19 follow-on flagging 3 unmapped secrets (`STATUSPAGE_API_KEY`, `TWILIO_ACCOUNT_SID`, `TWILIO_AUTH_TOKEN`). One secret receives a real code binding via a precise validator extension (Path A); two receive forward-looking annotations with target wave + feature (Path B). Validator output remains clean (exit 0; zero `code_only` drift) with `matrix_only` dropping from 21 → 20.

---

## 1. Context

Wave-19 SEAL (commit `2eec064`) closed the audit-export, Neon-shadow, CLI verify, and audit-export-async lanes. The closure report flagged a small residual: three secrets had been documented in `docs/internal/secrets-checklist.md` (rows #40 / #41 / #42) but the validator (`scripts/validate_secrets_matrix.py`) classified them as `matrix_only` — i.e. "documented in the matrix but no consumer detected in code."

The wave-20 small-closure brief specified two acceptable dispositions per secret:

- **Path A — bind to real consumer:** locate the code consumer; if the validator's scanner is missing it, extend the scanner. Net effect: `in_both` count increases.
- **Path B — tighten as forward-looking:** if no consumer exists yet, annotate the matrix row with `status: forward-looking`, target wave, and target feature; document under a new `§Forward-looking secrets` subsection so the soft-warn category is intentional rather than dead.

Both paths are valid; the choice was driven by *whether a real consumer exists in the wave-19 codebase*.

---

## 2. Per-secret triage

### 2.1 `STATUSPAGE_API_KEY` — Path A (bind to real consumer)

**Discovery.** A `grep` over `crates/`, `apps/`, `scripts/`, `specs/_runbooks/`, `docs/` revealed:

```
crates/corelink-statuspage-real/src/lib.rs:34            # doc comment
crates/corelink-statuspage-real/src/wasm32_backend.rs:202 # doc comment
crates/corelink-statuspage-real/src/http.rs:4             # doc comment
crates/corelink-statuspage-real/src/redact.rs:3           # CTRL-PRIV-001 redaction helper
crates/corelink-clerk-cf/src/dsr_statuspage_cron.rs:81    # pub const STATUSPAGE_API_KEY: &str = "STATUSPAGE_API_KEY";
crates/corelink-clerk-cf/src/dsr_statuspage_cron.rs:155   # env.secret(bindings::STATUSPAGE_API_KEY) — REAL CONSUMER
```

A real consumer exists in `crates/corelink-clerk-cf/src/dsr_statuspage_cron.rs:155`:

```rust
let api_key = match env.secret(bindings::STATUSPAGE_API_KEY) {
    Ok(v) => v.to_string(),
    Err(_e) => { /* fail-CLOSED audit emit */ return; }
};
```

This is the wave-18 `wasm32` Cloudflare Worker cron lane (DSR completion publish). The secret is **read at runtime via the workers-rs `env.secret(...)` API**, which is structurally different from `std::env::var(...)` and therefore was not matched by the validator's pre-existing `RUST_ENV_RE` regex.

**Validator extension.** The fix is a *precise* extension: scan for `env.{secret,var}(...)` calls, accept either a string literal argument or a `path::IDENT` argument that resolves against a file-local `pub const IDENT: &str = "VALUE"` map *where VALUE itself matches the env-var name shape* `[A-Z][A-Z0-9_]*`. This last filter is load-bearing — it excludes the ~17 unrelated `pub const COR_*: &str = "COR_*"` error-code constants and the various SQL-statement constants that share the back-ref shape.

The extension adds 30 lines to `scripts/validate_secrets_matrix.py`:

- `RUST_WORKER_ENV_RE` — new regex for `\.(?:secret|var)(...)` calls
- `RUST_BINDING_CONST_RE` — new regex for `pub const X: &str = "V"` declarations
- `scan_rust()` — pre-builds the file-local const map per file; resolves `path::IDENT` args via the map; guards against non-env-var values with a shape regex.

**Coverage probe.** A standalone Python probe confirms the extension hits exactly 3 binding-style consumers across the workspace:

```
['STATUSPAGE_API_KEY', 'STATUSPAGE_PAGE_ID', 'STATUSPAGE_TENANT_ID']
```

`STATUSPAGE_PAGE_ID` and `STATUSPAGE_TENANT_ID` were already in `in_both` via the `wrangler.toml` `[vars]` scanner. Net effect of the extension: **one additional secret** (`STATUSPAGE_API_KEY`) moves from `matrix_only` → `in_both`.

**Matrix row update.** Row #42 (`STATUSPAGE_API_KEY`) now lists both consumers:

```
corelink-statuspage-real (wave-16 DSR completion publish path via
`Authorization: OAuth …`) + corelink-clerk-cf::dsr_statuspage_cron
(wave-18 wasm32 scheduled cron; resolved via
`env.secret(bindings::STATUSPAGE_API_KEY)`)
```

### 2.2 `TWILIO_ACCOUNT_SID` — Path B (forward-looking)

**Discovery.** A repo-wide `grep` over `crates/`, `apps/`, `scripts/`, `specs/_runbooks/`, `docs/` returned **only documentation hits**:

```
docs/internal/secrets-checklist.md:98     # matrix row
docs/internal/secrets-runbook.md:42       # wrangler secret put line
```

No `.rs`, `.ts`, `.yml`, or `wrangler.toml` consumer was found. `TWILIO_ACCOUNT_SID` is genuinely **forward-looking** — the Twilio account is procured, the deploy runbook step is documented, but no code path consumes it yet.

**Disposition.** Path B: tighten the matrix row by replacing the `(SMS notification path)` placeholder consumer with an explicit forward-looking pointer:

```
(forward-looking — see §Forward-looking secrets;
target wave-22 SMS notification follow-on)
```

A new `## Forward-looking secrets` subsection (immediately above `## Drift policy`) documents the target wave, target feature, and tracking note for each forward-looking row. `TWILIO_ACCOUNT_SID` is annotated:

| Target wave | Target feature / binding | Tracking note |
|---|---|---|
| wave-22 | SMS notification follow-on (PagerDuty-equivalent customer-side SMS for SEV-1 breach notifications); pairs with `TWILIO_AUTH_TOKEN`. | Twilio account already procured; `wrangler secret put` step documented in `docs/internal/secrets-runbook.md` so the secret can be pre-staged before the wave-22 consumer crate lands. No code consumer yet. |

### 2.3 `TWILIO_AUTH_TOKEN` — Path B (forward-looking)

**Discovery.** Same as 2.2 — documentation-only hits, no code consumer:

```
docs/internal/secrets-checklist.md:99
docs/internal/secrets-runbook.md:43
```

**Disposition.** Path B, paired with `TWILIO_ACCOUNT_SID` under the same wave-22 target. Tracking note explicitly notes that the 180d rotation cadence **starts the day the consumer ships** — no rotation clock for unwired credentials (per CTRL-PRIV-001 spirit; rotation evidence must trace to a real consumer for SOC 2 CC6.1).

---

## 3. Validator delta

### 3.1 Before (commit `2eec064`)

```
validate_secrets_matrix: matrix=125 code=104 in_both=104 matrix_only=21 code_only=0
WARN matrix_only (21 rows):
  ...
  STATUSPAGE_API_KEY     ← targeted by this audit
  TWILIO_ACCOUNT_SID     ← targeted by this audit
  TWILIO_AUTH_TOKEN      ← targeted by this audit
```

### 3.2 After (this audit)

```
validate_secrets_matrix: matrix=125 code=105 in_both=105 matrix_only=20 code_only=0
WARN matrix_only (20 rows — all now intentional per §Forward-looking secrets):
  ...
  TWILIO_ACCOUNT_SID     ← now annotated forward-looking wave-22
  TWILIO_AUTH_TOKEN      ← now annotated forward-looking wave-22
  (STATUSPAGE_API_KEY no longer in matrix_only — now in_both)
```

**Exit code:** 0 (no `code_only` drift; `matrix_only` is soft-warn by design).

**Net delta:** `in_both` 104 → 105 (+1); `matrix_only` 21 → 20 (-1). The two remaining TWILIO rows are now **classified rather than unmapped** — every matrix-only row has an explicit row in `§Forward-looking secrets` documenting why it is matrix-only and what wave will move it to `in_both`.

### 3.3 Brief acceptance criterion

The brief specified: *"3 unmapped warnings → 0 unmapped (with 21 forward-looking acknowledged as before, ideally now 21+3=24 OR they get binding consumers — whichever the discovery surfaces)."*

Discovery surfaced a **hybrid**:

- 1 secret (`STATUSPAGE_API_KEY`) had a real consumer → bound via scanner extension → moved to `in_both`.
- 2 secrets (`TWILIO_*`) had no consumer → tightened as forward-looking → remain in `matrix_only` but with explicit per-row classification in the new subsection.

This is the cleanest outcome: we did not paper over a real binding by labeling it forward-looking (would have been factually wrong), and we did not invent a fake consumer to push `TWILIO_*` into `in_both` (would have been audit-toxic). The 3 secrets are now individually triaged with cited reasoning per the audit charter.

---

## 4. Scanner-extension correctness

### 4.1 Why a back-reference filter on the const map

The naive extension *"if `env.secret(bindings::X)` is called, treat X as the env-var name"* would over-match. Many `pub const` declarations in the workspace have the shape `pub const NAME: &str = "VALUE"` where VALUE is NOT an env-var name. Examples surveyed:

- `corelink-reapi::error_map::COR_AUTH_PAT_INVALID = "COR_AUTH_PAT_INVALID"` (error code)
- `corelink-audit-chain::neon_shadow::real::SQL_BEGIN_TXN = "BEGIN"` (SQL keyword)
- `corelink-dsr::receipt::RECEIPT_ALG_RS256 = "RS256"` (JWA alg literal)
- `corelink-cf-bindings::*` worker-binding constants (real env vars — kept).

The extension's correctness rests on **three filters stacked**:

1. The match site must be a `.secret(...)` or `.var(...)` method call — restricts to worker-rs binding access shape.
2. The argument must be either a string literal OR a `path::IDENT` that exists as a `pub const IDENT: &str = "VALUE"` *in the same file*.
3. The resolved VALUE must itself match `^[A-Z][A-Z0-9_]*$` — restricts to env-var-name shape.

Together these three filters precisely capture worker-binding env vars and reject error codes, SQL keywords, JWA algs, and other unrelated `pub const X: &str = "V"` declarations.

### 4.2 Why file-local resolution (not workspace-wide)

`workers-rs` binding constants are conventionally declared in a `mod bindings` block at the top of the same `.rs` file that consumes them (see `crates/corelink-clerk-cf/src/dsr_statuspage_cron.rs:69-90`). The validator's scanner is per-file and pre-builds the const map once per file before iterating the call sites. This:

- Keeps the validator's complexity linear in file size (no workspace-wide symbol table needed).
- Avoids cross-file false positives (a `pub const FOO: &str = "FOO"` in one file would otherwise resolve `bindings::FOO` in another, even if they are unrelated).
- Matches the worker-rs idiom — bindings live next to their consumer.

### 4.3 Future-extension cost

If the workspace adopts a shared `mod bindings` module imported across files, the scanner would need workspace-wide resolution. That extension is straightforward (build the const map across all `.rs` files first, then iterate call sites) but **not required today**. The current per-file scope is sufficient for every binding-style consumer in the wave-19 codebase (3 hits, all in `dsr_statuspage_cron.rs`).

---

## 5. Compliance impact

- **SOC 2 CC6.1 (logical access — credentials).** The daily `secrets-drift.yml` cron now reports 20 `matrix_only` rows, all of which have explicit per-row classification in the new `§Forward-looking secrets` subsection. Audit sampling can answer "why is row X matrix-only?" via the matrix itself, no spelunking required.
- **CTRL-PRIV-001 (no credential plaintext).** Unchanged. The validator scans for *names*, never values; the matrix is value-free; the audit doc is value-free.
- **INV-AUTH-AUDIT-PSEUDONYMIZATION.** Unchanged. `STATUSPAGE_API_KEY`'s `corelink-statuspage-real::redact` helper continues to enforce that the OAuth token is never logged plaintext (verified pre-existing behaviour; not touched by this audit).
- **Drift policy.** Unchanged. Adding an `env.secret("FOO")` or `env::var("FOO")` call still requires adding a matrix row in the same PR; the validator now catches both call shapes.

---

## 6. Verification

```
$ python3 scripts/validate_secrets_matrix.py ; echo exit=$?
validate_secrets_matrix: matrix=125 code=105 in_both=105 matrix_only=20 code_only=0
WARN matrix_only (forward-looking or stale rows; soft-warn):
  - AUDIT_R2_BUCKET
  - AWS_ACCESS_KEY_ID
  - AWS_SECRET_ACCESS_KEY
  - AWS_USE_FIPS_ENDPOINT
  - CLERK_AUDIENCE
  - CLERK_JWKS_URL
  - CLERK_JWT_ISSUER
  - CLERK_PUBLISHABLE_KEY
  - COOKIEBOT_DOMAIN_GROUP_ID
  - DT_MOCK_INJECTION_ENABLED
  - HUBSPOT_PRIVATE_APP_TOKEN
  - PAGERDUTY_SYNTHETIC_ROUTING_KEY
  - SLACK_WEBHOOK_URL_ALERTS_SEV1
  - SLACK_WEBHOOK_URL_ALERTS_SEV2
  - SLACK_WEBHOOK_URL_BREACH_NOTIFICATIONS
  - SLACK_WEBHOOK_URL_ENTERPRISE_INQUIRIES
  - SLACK_WEBHOOK_URL_LIGHTHOUSE_CUSTOMERS
  - SLACK_WEBHOOK_URL_ONCALL_HANDOFF
  - TWILIO_ACCOUNT_SID
  - TWILIO_AUTH_TOKEN
exit=0
```

Brief acceptance: ✓ no new unmapped warnings; the 3 wave-19-flagged secrets are individually resolved (1 mapped, 2 forward-looking-tightened).

---

## 7. Decision log

| Date | Decision | Rationale |
|---|---|---|
| 2026-05-16 | `STATUSPAGE_API_KEY` → Path A (bind) | A real consumer exists at `crates/corelink-clerk-cf/src/dsr_statuspage_cron.rs:155`; misclassification was a scanner-coverage gap (worker-rs `env.secret(...)` not matched by the existing `env::var(...)` regex), not a documentation gap. |
| 2026-05-16 | `TWILIO_*` → Path B (forward-looking) | Repo-wide grep confirms zero code consumers. Twilio account is procured (per runbook), but no consumer crate exists in the wave-19 codebase. Forward-looking classification is factually correct. |
| 2026-05-16 | Scanner extension is per-file, not workspace-wide | Worker-rs convention is to declare `mod bindings` in the same file as consumers. Workspace-wide resolution adds complexity without coverage gain today. |
| 2026-05-16 | Scanner-resolved values gated to `^[A-Z][A-Z0-9_]*$` shape | Filters out the 17+ unrelated `pub const X: &str = "X"` error codes (`COR_*`) and SQL keywords (`SQL_*`) that share the back-ref shape but are not env vars. |
| 2026-05-16 | New `§Forward-looking secrets` subsection added | Per-row classification makes the daily `secrets-drift.yml` cron's `matrix_only` count audit-explainable at a glance (SOC 2 CC6.1 evidence ergonomics). |

---

## 8. Files changed

- `scripts/validate_secrets_matrix.py` — added `RUST_WORKER_ENV_RE`, `RUST_BINDING_CONST_RE`, and extended `scan_rust()` to resolve worker-binding consts with shape-filter.
- `docs/internal/secrets-checklist.md` — updated `Last sealed` header; updated row #42 (`STATUSPAGE_API_KEY`) consumer column; updated rows #40 / #41 (`TWILIO_*`) to point at `§Forward-looking secrets`; added `## Forward-looking secrets` subsection above `## Drift policy`.
- `specs/_audits/2026-05-16-secrets-matrix-tighten.md` — this audit.

No production code touched; no spec/runbook touched. Closure is documentation + validator-scanner only.
