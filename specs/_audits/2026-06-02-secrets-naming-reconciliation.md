---
id: "AUDIT-2026-06-02-SECRETS-NAMING-RECONCILIATION"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-02"
updated: "2026-06-02"
owner: "Gustavo Schneiter"
tags: ["audit", "secrets", "prod", "incident", "CTRL-CRED-001", "launch"]
references:
  - "docs/internal/secrets-checklist.md"
  - "scripts/secrets-mvp-allowlist.txt"
  - "scripts/put-secrets-prod.sh"
---

# Prod Secrets — PAT_SIGNING_KEY Naming Reconciliation

## Finding (root cause)

Prod ran the PAT signing key under **two different secret names with two
different values**:

- `PAT_SIGNING_KEY` — the name the **runtime reads** (`crates/corelink-container/src/main.rs`,
  `.../routes/internal_pat.rs`, `worker/src/index.ts`).
- `HUGR_PAT_SIGNING_KEY` — the name the **deploy tooling manages**
  (`scripts/secrets-mvp-allowlist.txt`, sourced from `.env.local`).

So `put-secrets-prod.sh` kept syncing `HUGR_PAT_SIGNING_KEY`, while the
runtime's `PAT_SIGNING_KEY` drifted to a separate, unsaved value. Net effect:
PATs minted with the `.env.local` key failed the edge HMAC check (401) — the
root cause of the `sccache → CoreLink` dogfood 401 and any PAT-verify drift.

## Remediation applied (2026-06-02)

| # | Action | Scope | Verified |
|---|---|---|---|
| 1 | Set `PAT_SIGNING_KEY` = `.env.local`'s value | all 5 prod workers (corelink-prod + -lhr/-nrt/-sam/-syd) | cache auth 401 → **passed** (404 = separate cache-path detail) |
| 2 | Delete orphan `HUGR_PAT_SIGNING_KEY` | all 5 prod workers | HTTP 200 each |
| 3 | Fix deploy tooling: allowlist `HUGR_PAT_SIGNING_KEY` → `PAT_SIGNING_KEY` | `scripts/secrets-mvp-allowlist.txt` | `--mvp-only` dry-run now plans `PAT_SIGNING_KEY`; `HUGR_PAT_SIGNING_KEY` gone |
| 4 | Add canonical `PAT_SIGNING_KEY` to source of truth | `.env.local` (gitignored) | present |

## Code-trace (which deployed secrets the runtime actually reads)

| Deployed secret | Read by code? | Verdict |
|---|---|---|
| `PAT_SIGNING_KEY` | ✅ container + worker | canonical — fixed |
| `HUGR_OCI_TOKEN_KEY` | ✅ `corelink-adapter-host/src/oci/config.rs` | consistent (code reads the HUGR_ name) — leave |
| `CORELINK_INTERNAL_AUTH_KEY` | ✅ container + worker | consistent — leave |
| `HUGR_PAT_SIGNING_KEY` | ❌ orphan | **deleted** |
| `HUGR_SESSION_HMAC_KEY` | ❌ no reader found | left (conservative — confirm before delete) |
| `HUGR_AUDIT_CHAIN_HMAC_KEY` | ❌ no reader found | left (audit chain is core + verified working — do NOT blind-delete) |
| `SESSION_HMAC_KEY` / `AUDIT_CHAIN_HMAC_KEY` / `OCI_TOKEN_KEY` (non-prefixed) | ❌ not deployed / not read | n/a |

## Open follow-ups (deliberate — not auto-actioned)

1. **`HUGR_SESSION_HMAC_KEY` / `HUGR_AUDIT_CHAIN_HMAC_KEY`** — trace how the
   audit-chain + session features obtain their HMAC key (no env reader found;
   likely orphans or read via a path not yet traced). Confirm unused, then
   delete. Deleting a used key breaks prod — do NOT blind-delete.
2. **`secrets-checklist.md`** has zero `PAT_SIGNING_KEY` rows — add one so the
   `secrets-checklist-verify.sh` deploy gate covers it (it currently passes via
   the MVP allowlist path).
3. **43 deferred integration secrets** (Twilio/Azure/GCP/Vault/Neon/Drata/etc.)
   — add per-feature when each ships (`--mvp-only` WARN-skips them).

## Note

Real secret VALUES live only in Cloudflare (write-only) + `.env.local` (the
single recoverable copy — **must be backed up**). `CLOUDFLARE_API_TOKEN` has
D1 + Workers read/write; the other CF tokens do not.
