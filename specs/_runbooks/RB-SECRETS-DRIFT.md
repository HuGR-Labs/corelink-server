---
id: "RB-SECRETS-DRIFT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "SRE Lead"
final_approver: "Security Lead"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-R2-14"
tags: ["runbook", "r-prep", "secrets", "drift", "soc2", "cc6.1", "supply-chain"]
---

# RB-SECRETS-DRIFT — Secrets-matrix drift triage

> **Owned by:** SRE Lead (co-owned with Security Lead).
> **Triggered by:** failure of `.github/workflows/secrets-drift.yml` (PR gate
> or daily 04:00 UTC cron), failure of `scripts/secrets-checklist-verify.sh`
> in `cf-deploy-prod.yml`, or auditor request during a SOC 2 CC6.1
> walkthrough.
>
> **SLA:** code-only drift on `main` MUST be closed within **24h** (matrix
> row added OR offending env var reverted). Matrix-only drift (stale rows)
> is soft-warn — must be reconciled before each quarterly access review
> but does NOT block deploys.

---

## 1. Trigger detection

A drift event is recognised when any of the following fires:

1. PR check `secrets-drift / secrets-matrix drift gate` is **red**.
2. Scheduled run of `secrets-drift` (daily 04:00 UTC) reports
   `summary.code_only > 0` in the uploaded `secrets-drift-report.json`
   artifact.
3. `cf-deploy-prod.yml` aborts at the
   `scripts/secrets-checklist-verify.sh` step.
4. Auditor (Schellman / A-LIGN / Drata) flags missing evidence for an
   in-scope secret during CC6.1 sample review.

If ANY of (1..4) occurs, this runbook applies.

---

## 2. Roles

| Role | Responsibility |
|---|---|
| SRE Lead | Triage owner; decides whether the new env var is a real production secret. |
| Security Lead | Co-signs on net-new secrets in `cf-wrangler` / `gha-secret` tier; approves rotation cadence. |
| PR author | Provides context on the call site (new feature? test scaffold? wrong import?). |
| Release Manager | Notified if drift blocks an in-flight cosign/notarize signing pipeline. |

---

## 3. Triage tree

```
secrets-drift workflow red
       │
       ▼
Pull artifact: secrets-drift-report.json
       │
       ▼
For each entry in report.code_only:
       │
       ├── Q1: Is this a *real* production secret (vendor key, signing key, customer credential)?
       │      ├── YES → §4.A: Add row to matrix
       │      └── NO  → §4.B: Add to allowlist OR revert call site
       │
       └── Q2: Was it added intentionally in the current PR?
              ├── YES → Author owns §4
              └── NO  → On-call SRE owns §4 (treat as drift incident)
```

For each entry in `report.matrix_only`:

```
       │
       ▼
Q3: Is the matrix row forward-looking (planned consumer not yet merged)?
       ├── YES → Leave in place; document expected consumer crate in the row's "Notes" cell.
       ├── NO  → Q4: Was the consumer code removed in a recent PR?
       │             ├── YES → §4.C: Remove the matrix row
       │             └── NO  → §4.C: Investigate; row may be a misfile
```

---

## 4. Resolution paths

### 4.A — Add a row to the matrix (code-only drift, real secret)

Required when the new env var is a vendor credential, customer-side key,
or any value with non-zero blast radius on compromise.

1. Open `docs/internal/secrets-checklist.md`.
2. Append a row to the matrix table with **every column filled**:
   `# | Secret | Env var name | Consumer crate(s) / workflow | Vendor |
   Vendor URL | Acquisition steps | Rotation cadence | Rotation owner |
   Compromise procedure | Stored at`.
3. Bump the row count in the `**Total rows:** N` footer at the bottom of
   the matrix.
4. Update `**Last sealed:** YYYY-MM-DD` header to today.
5. If the secret is in `gha-secret` or `cf-wrangler` tier, add it to the
   appropriate `wrangler.toml` placeholder section / GHA environment
   secret list **in the same PR**.
6. Open the PR with title `secrets-matrix: add <ENV_VAR_NAME> (<vendor>)`.
   Required reviewers: SRE Lead + Security Lead.
7. Re-run the failed PR gate (`secrets-drift`) — it MUST go green before
   merge.

**Acceptance:** `python3 scripts/validate_secrets_matrix.py` exits 0;
`bash scripts/secrets-checklist-verify.sh` exits 0.

### 4.B — Add to allowlist (not actually a secret)

Use this path for proptest knobs, OS env vars, build metadata, test-only
flags. The bar is HIGH — anything that could leak a credential or change
runtime trust posture MUST go through §4.A.

1. Edit `scripts/validate_secrets_matrix.py` → `ALLOWLIST_REGEX`.
2. Mirror the same anchor in `scripts/secrets-checklist-verify.sh`
   (the bash deploy-gate verifier).
3. Add a one-line comment explaining why the var is not a secret
   (e.g. `# K6_TARGET_HOST: load-test target host, not a credential`).
4. Open the PR with title `secrets-drift: allowlist <ENV_VAR_NAME>`.
   Required reviewer: Security Lead.

### 4.C — Remove a matrix row (matrix-only drift, dead row)

1. Verify the row has no consumer:
   `grep -rE "env::var\\(_os\\)?\\(\"<ENV_VAR_NAME>\"|process\\.env\\.<ENV_VAR_NAME>|secrets\\.<ENV_VAR_NAME>" .`
2. If confirmed absent, delete the row from `docs/internal/secrets-checklist.md`.
3. Decrement `**Total rows:** N`.
4. If the secret was rotated in the past 90 days, schedule a final
   revocation at the vendor (via `secrets-runbook.md` §3 rotation flow).
5. Open the PR with title `secrets-matrix: retire <ENV_VAR_NAME>`.

---

## 5. Verification

After resolution, on the PR branch:

```bash
python3 scripts/validate_secrets_matrix.py
# Expected: validate_secrets_matrix: ... code_only=0
bash scripts/secrets-checklist-verify.sh
# Expected: secrets-checklist-verify: OK (no drift).
```

If either command still reports drift, **DO NOT** override or skip — open
an incident and page SRE on-call. The CF deploy gate will fail anyway,
so silencing the PR check would not unblock anything downstream.

---

## 6. Post-incident actions

- Update `specs/_audits/2026-05-15-secrets-coverage-baseline.md` (or its
  successor) with the new total counts if rows were added/removed.
- If drift was a result of a new vendor onboarding, capture the
  acquisition steps in `docs/internal/secrets-runbook.md` so the next
  rotation can follow the same flow.
- If drift was caused by a copy/paste error in a test fixture, add a
  unit test that asserts the env var is read only behind a `cfg(test)`
  guard.

---

## 7. SOC 2 / ISO 27001 evidence

This runbook closes a portion of the **CC6.1 (Logical Access — Credential
Management)** trust services criterion: the matrix + drift gate provides
auditable proof that every production secret has a documented rotation
owner, cadence, and compromise procedure, and that the codebase cannot
silently introduce new untracked credentials.

Evidence sources:

- `secrets-drift-report.json` artifact (90d retention) — daily snapshot.
- `cf-deploy-prod.yml` deploy gate log — proves drift cannot reach
  production without an explicit override (no override path exists).
- `docs/internal/secrets-checklist.md` git history — proves every row
  was reviewed at PR time.

Mapped to: `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §CC6.1.

---

## 8. Cross-references

- `docs/internal/secrets-checklist.md` — canonical matrix
- `docs/internal/secrets-runbook.md` — rotation procedures
- `scripts/validate_secrets_matrix.py` — Python validator (this runbook's primary trigger)
- `scripts/secrets-checklist-verify.sh` — bash deploy-gate verifier
- `.github/workflows/secrets-drift.yml` — daily cron + PR gate
- `.github/workflows/cf-deploy-prod.yml` — production deploy gate
- `ROADMAP-TO-GA.md` §9 — Human Track credential acquisition
- `specs/_audits/2026-05-15-secrets-coverage-baseline.md` — baseline snapshot
