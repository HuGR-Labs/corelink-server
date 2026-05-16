---
id: "RB-AUDIT-EXPORT-INTEGRITY"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WI-R-PREP-AUDIT-EXPORT"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RB-AUDIT-CHAIN-001"
  - "security_model"
tags: ["runbook", "audit", "audit-export", "soc2", "cc7.2", "gdpr", "lgpd", "chain-integrity", "fail-closed"]
---

# RB-AUDIT-EXPORT-INTEGRITY — Customer-reported audit export verify failure

> **Status:** DRAFT. Owner: Gustavo Schneiter.
>
> **Scope:** triage when a customer (or their auditor) reports that
> `corelink audit verify <export>` failed against an export the customer
> produced via `corelink audit export`. Covers both legitimate chain
> breaks (a P0 incident — daily verifier would surface this too) and
> non-incident causes (clock skew, partial export, customer-side file
> corruption).
>
> **Companion docs:**
> - `specs/03_architecture/security_model.md` §Merkle proofs (cryptographic spine)
> - `crates/corelink-audit-chain/src/exporter.rs` (export trait + proof shape)
> - `crates/corelink-cli/src/commands/audit.rs` (CLI surface)
> - `apps/docs/docs/how-to/export-audit-log.mdx` (customer-facing instructions)
> - `RB-AUDIT-CHAIN-001` (sibling runbook for the daily verifier's chain-break alarm)
>
> **Trigger:** any of —
> 1. Customer ticket: "`corelink audit verify` reported a chain break."
> 2. Customer ticket: "the BLAKE3 in the export filename doesn't match the file body."
> 3. Auditor / regulator email: "the audit log you sent us appears altered."
> 4. Drata evidence task fails its automated integrity check.
> 5. Internal Slack: `#sev-1-audit-integrity` automated alert from the daily verifier (deduplicate with `RB-AUDIT-CHAIN-001`).

---

## 1. Severity decision (5 minutes)

Open the customer's export file. Run locally:

```bash
corelink audit verify <path-to-export>.jsonld
```

| Symptom | Severity | Next step |
|---|---|---|
| `verify failed: at least one Merkle proof did not match the chain anchor` | **SEV-0** until proven otherwise. | §2 chain-break triage. |
| `audit verify: export missing inclusion proofs` | SEV-3 (customer pilot error). | Re-export with `--include-merkle-proofs --verify`; close ticket. |
| `audit verify: file not found / parse error` | SEV-4. | Help customer rebuild the export; check disk corruption / filesystem chain. |
| BLAKE3 in filename does NOT match recomputed BLAKE3 of body | SEV-1 — file in transit was corrupted OR tampered. | §3 file-integrity triage. |

**Page the on-call SRE for SEV-0 / SEV-1** via [`ONCALL-ESCALATION-MATRIX.md`](./ONCALL-ESCALATION-MATRIX.md).

---

## 2. Chain-break triage

A chain break in an export means one of:

| Cause | Probability | How to confirm |
|---|---|---|
| **Real tamper on R2** — adversary modified an audit object inside Object Lock retention (extremely unlikely; Object Lock Governance Mode blocks PutObject overwrite). | < 0.01% | Pull the daily verifier's `last_verified_hash` for the same tenant + sequence range. If verifier passed but export breaks → §2.1. |
| **JCS canonicalization drift** between producer (CF Worker) and consumer (CLI verifier). | Low — pinned `serde_jcs = "0.2"` workspace-wide. | Compare `serde_jcs` version in customer's CLI vs the CF Worker. |
| **Partial export window** crossing a chain-rotation boundary (per-region chain in S-09 invariant 4). | Medium — most common false-positive. | Inspect manifest `chain_anchor_prev_hash` and `chain_head_at_export`; cross-check against the daily verifier checkpoint store. |
| **Customer-side file mutation** after `corelink audit export` (text editor opened it, accidentally saved). | High in T-90 customer pilots. | Re-export and have customer attach the **fresh** file unchanged. |

### 2.1 Confirming a real chain break

1. **Match the chain head.** Open the manifest:
   ```bash
   jq '.manifest' <export>.jsonld
   ```
   Cross-reference `chain_head_at_export` against the daily verifier's `last_verified_hash` for the same tenant + the manifest's `until_ms`. Source: `D1 audit_chain_verify_checkpoints` table (admin UI; deferred — query via wrangler today).
   - **MATCH** → the chain head AT EXPORT TIME was clean; the break is in the customer-side file or a partial export. Go to §3.
   - **MISMATCH** → the chain head differs from what the daily verifier recorded. STOP. Escalate to **SEV-0**.

2. **Locate first divergence.** The verify error includes `at_sequence`. Pull the canonical event at that sequence from R2:
   ```bash
   # R2 layout: audit/{tenant_id}/{date}/{seq:08}.cloudevent.ndjson
   wrangler r2 object get corelink-audit-iad "audit/${TENANT_ID}/${DATE}/$(printf '%08d' $SEQ).cloudevent.ndjson"
   ```
   Compare byte-for-byte against the row in the export. If different, the export was mutated (customer-side or in-transit) — recoverable. If identical, the daily verifier's stored hash differs from re-canonicalized hash → **the chain is broken**.

3. **Trigger `RB-AUDIT-CHAIN-001`.** Real chain breaks fold into the existing audit-chain-break runbook (SEV-0 page Gustavo + Privacy Officer + Compliance).

---

## 3. File-integrity triage (BLAKE3 mismatch)

When the BLAKE3 in the filename ≠ recomputed BLAKE3 of body:

1. **Ask the customer to re-download / re-fetch** the file from the original source (Drata, S3, email attachment). Email round-tripping through Microsoft Defender SafeAttachments occasionally re-encodes newlines.
2. **If re-download produces the same mismatch**, the file was mutated after export. Have the customer re-run `corelink audit export` directly into the evidence channel.
3. **If the customer cannot re-export** (chain head has advanced), produce a fresh export server-side from the same window. The chain head will differ, but the proof shape is stable.

---

## 4. Re-export procedure

For SEV-0 / SEV-1 cases where we need to ship the customer a clean replacement:

```bash
# Operator-side (CoreLink team)
corelink audit export \
  --tenant <customer-tenant-id> \
  --since <ms> \
  --until <ms> \
  --format json-ld \
  --include-merkle-proofs \
  --verify \
  --output ./operator-replacement/

# Attach the operator-replacement file + a signed cover letter
# referencing the original ticket number to the customer's Drata
# evidence channel.
```

**Audit-of-audit:** the re-export itself emits an audit event (`corelink.audit_export.window_exported`) so the operator's action is in the chain.

---

## 5. Customer-facing comms

### 5.1 SEV-3 (missing proofs / pilot error)

> Hi `<name>`,
>
> Your export was generated without inclusion proofs, which is why `audit verify` doesn't have anything to check. Run it again with `--include-merkle-proofs --verify`:
>
> ```bash
> corelink audit export --tenant <ID> --since <MS> --until <MS> \
>   --format json-ld --include-merkle-proofs --verify --output ./
> ```
>
> The new file is content-addressed (BLAKE3 in the filename) — attach that to your evidence ticket. No further action needed on our side.
>
> — CoreLink Support

### 5.2 SEV-1 (file mutated in transit)

> Hi `<name>`,
>
> The BLAKE3 in the export filename doesn't match the file body, which means something altered the file after export — most likely the email gateway or a text-editor save. We've generated a fresh export and attached it to this ticket; please re-run `corelink audit verify` on the attachment.
>
> Your audit chain is intact server-side (the daily verifier confirmed `chain_verified_ok` at `<TIMESTAMP>` for the same window).
>
> — CoreLink Support

### 5.3 SEV-0 (real chain break — extremely rare)

**Do not send a templated reply.** Page Gustavo + Compliance Officer. Internal Slack `#sev-0-audit-integrity`. Customer comms drafted jointly with Privacy + Legal per `RB-AUDIT-CHAIN-001`. Regulatory notification clock starts at confirmation (GDPR Art. 33 — 72h to supervisory authority).

---

## 6. Post-incident actions

For every SEV-0 / SEV-1:

1. Post-mortem within 5 business days (`specs/_post_mortems/PM-AUDIT-EXPORT-<YYYY-MM-DD>-<ticket>.md`).
2. If JCS drift was the cause: pin a regression test in `crates/corelink-audit-chain/tests/prop_audit_chain.rs` covering the exact event shape.
3. If file-in-transit corruption was the cause: amend the customer doc with the SafeAttachments warning; consider Drata API direct upload instead of email.
4. Tag the runbook execution in the runbook-drill tracker:
   ```bash
   corelink runbook-drill record \
     --runbook-id RB-AUDIT-EXPORT-INTEGRITY \
     --executor op_<id> \
     --evidence <asciinema-url> \
     --duration-seconds <ACTUAL> \
     --expected-seconds 1800
   ```

---

## 7. Fitness function

This runbook MUST be drilled quarterly. Expected duration **30 minutes** end-to-end on a synthetic export (`corelink audit export --fixture`). Drift > 2x (60 min) triggers FM-202 review per `corelink-runbook-tracker`.

## 8. See also

- `specs/_runbooks/RB-GA-CUTOVER.md` §3.7 + §4 G2 + §6.1.3 — GA cutover audit-chain Logpush enablement + greenlight criterion G2 + T+24h spot verifier; pre-cutover re-read mandatory per §0.2.5.
