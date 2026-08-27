---
id: "RB-DEVENV-SNAPSHOT-CORRUPTION"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "devenv", "p1", "sre", "wave-devenv"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-DEVENV-SNAPSHOT-CORRUPTION — CAS Workspace Manifest Hash Mismatch

> **Severity floor:** P1
> **Detect → Acknowledge → Engage signers:** SRE on-call + CAS storage lead.
> **Companion docs:** `RB-INCIDENT-RESPONSE.md`.

---

## 1. Symptoms
- `clw hydrate` fails with `CAS_CHUNK_HASH_MISMATCH` or `MANIFEST_INVALID`.
- Workspace boots into empty directory fallback.

## 2. Diagnosis
- Inspect R2 CAS blob digests for the corresponding manifest reference.
- Run `clw ls --name <WORKSPACE> --json` to inspect previous snapshot generations.

## 3. Resolution
- Re-hydrate to the preceding known valid snapshot generation:
  ```bash
  clw hydrate --name <WORKSPACE> --generation <PREVIOUS_GEN> --force
  ```
