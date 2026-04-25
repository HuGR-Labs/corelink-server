---
id: "RB-DSR-ERASURE-INCOMPLETE"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "privacy", "dsr", "erasure", "lgpd", "gdpr", "compliance"]
---

# RB-DSR-ERASURE-INCOMPLETE — Erasure Incomplete (DSR Backend Coverage Gap + Pseudonymization Conflict)

> **INV:** INV-DATA-ERASURE-COMPLETE HIGH | **CTRL:** CTRL-PRIV-030 | **SLA:** detect ≤ 24h, remediate ≤ 30 days (regulatory absoluto)

## Pré-condições

- S-11 DSR pipeline live com 10-backend coverage (6 erasure-effective + 4 pseudonymized Object Lock).
- Verification job 24h post-erasure cron ativo.
- Audit chain integrity preservado.
- Erasure attestation Ed25519 disponível (S-14 BYOK customers).

## Detecção

### Sinais primários

- Verification job 24h post-erasure encontra records remaining em algum dos backends.
- Customer complaint: "you still have my data after I requested erasure".
- Quarterly internal compliance audit identifies DSR backend gap.
- Métrica `corelink.privacy.erasure_verification_fail_total` > 0.

### Detection per backend

```
Erasure-effective backends (FULL DELETE expected):
  D1: SELECT count(*) FROM <table> WHERE subject_id = ? — should be 0
  Neon: SELECT count(*) FROM <table> WHERE subject_id = ? — should be 0
  R2 (mutable): aws s3 ls s3://corelink-cas/<tenant_prefix>/ — should be empty
  KV: cf kv:bulk get --prefix=<subject_prefix> — should return 0
  DO: do.list_keys(<prefix>) — should return 0
  Stripe: customer.id deleted — verify via API
  Loki (warm 90d): query API delete confirmed

Pseudonymized backends (PII REPLACED, audit chain preserved):
  R2 audit (Object Lock 7y): records remain BUT subject_id replaced com sha256(tenant_id || erasure_salt)
  R2 billing-events (Object Lock 7y): same
  Loki cold (R2 archive 400d): same
  Cloudflare Analytics Engine (rolling 30d): same; auto-expires
```

## Comunicação

- **Severidade**: **SEV-1** (regulatory exposure LGPD Art. 18 + GDPR Art. 17 + CCPA §1798.105).
- **Page**: Privacy Officer + Legal + SRE + Engineer (S-11 owner).
- **Internal channel**: `#incidents-corelink-privacy` (separate from general SEVs).
- **Customer notification**: REQUIRED within 72h (GDPR Art. 33 if data integrity gap).
- **Regulatory notification**: consideration based on jurisdiction + scope:
  - **LGPD**: ANPD notification se "risco ou dano relevante" (Art. 48); evaluation by Legal + DPO interim.
  - **GDPR**: Lead supervisory authority (Irish DPC default) within 72h.
  - **CCPA**: state AG (California) "in the most expedient time possible".
- **Status page**: usually internal; customer-facing notification per affected subject.

## Mitigação imediata (≤ 30 dias regulatory absoluto)

### Step 1: Identify affected backend(s) + records (≤ 24h)
1. Run verification query per backend.
2. Identify records remaining + cause:
   - **Erasure-effective backend missed**: complete erasure manually + investigate.
   - **Pseudonymization malformed**: pseudo records have PII visible? Re-pseudonymize.
3. Document scope: how many subjects, how many records, time since DSR request.

### Step 2: Per-record remediation (≤ 7d)

**Case A: Erasure-effective backend gap**
- Manual erasure execute imediato (SQL DELETE / R2 delete / KV purge / DO delete / Stripe delete).
- Audit emit `corelink.privacy.erasure_completed_retroactive` per record.
- Re-run verification job 24h post-fix.

**Case B: Pseudonymization malformed**
- Re-pseudonymize com correct SHA-256 + tenant_id || erasure_salt.
- Audit emit `corelink.privacy.pseudonymization_corrected`.
- Customer notification: pseudonymization completed.

**Case C: Cross-backend partial fail**
- Complete em remaining backends sequentially.
- Verify integrity per backend.
- Compile final compliance report per subject.

### Step 3: Customer + regulatory notification (≤ 72h)
1. Customer outreach per affected subject:
   - Apology + transparency.
   - Final confirmation pos-fix.
   - Offer: extended privacy controls / SLA credit if commercial customer.
2. Regulatory: Legal/DPO determines based on jurisdiction + impact scope.
3. Audit log retention 7y for compliance.

### Step 4: Post-mortem (≤ 7d)
1. Mandatory post-mortem.
2. 5-Why root cause.
3. Privacy Officer + Legal review.
4. Action items tracked.

## Diagnóstico (≤ 24h)

### Causa raiz típica

1. **Backend integrated tardio** (40-50%):
   - New backend added (e.g., S-14 BYOK adds KMS — DSR sweep não atualizado).
   - Solution: backend registry + CI gate force update DSR sweep when new backend added.

2. **Bug em erasure worker** (20-30%):
   - Specific edge case (unicode subject_id, special chars em path).
   - Solution: property test 10k subject_id variations.

3. **Audit chain conflict** (10-15%):
   - Object Lock 7y prevents delete; pseudonymization rule não applied.
   - Solution: ensure pseudonymization path triggers when Object Lock detected.

4. **Race condition** (5-10%):
   - DSR erasure overlapping com new write (post-erasure record); should be idempotent re-fire.
   - Solution: erasure idempotent + 24h verification catches.

5. **Cross-region replica missed** (5-10%):
   - S-14 hot blob replica not erased; primary erased OK.
   - Solution: erasure cascades to all regions; verification covers.

### Investigação

- Verification job logs detail.
- Audit chain trace per affected record.
- Backend admin API queries.
- Erasure attestation Ed25519 verify (S-14).
- Code path review (recent S-11 changes).

## Resolução

### Hot fix (≤ 30d regulatory absoluto)

- Complete erasure manually em all affected backends.
- Verification re-run 24h post-fix.
- Customer notification.
- Compliance audit log.

### Cold fix (1-3 months)

- **Backend coverage CI gate**: PR adicionando new backend MUST add to DSR sweep list (compile-time check).
- **Property test expansion**: 10k subject_id variations + chaos backend unavailability.
- **Pseudonymization E2E test**: monthly synthetic DSR + verify all 10 backends.
- **Customer-facing erasure dashboard** (S-16): customer pode verify erasure status real-time.
- **Erasure attestation Ed25519 verify endpoint** (S-14 R-S14-10): customer pode verify post-erasure proof.
- **TLA+ `dsr_erasure_atomicity.tla` (planned)**: formal model erasure atomic OR compensating-rollback.

## Post-incident

- Post-mortem mandatório CRITICAL.
- 5-Why focused on backend coverage gap.
- INV-DATA-ERASURE-COMPLETE review.
- Customer trust review + outreach.
- Regulatory engagement se LGPD/GDPR/CCPA notification triggered.

## Evidence

- Verification job report (pre/post fix).
- Backend-by-backend remediation log.
- Audit chain trace per affected subject.
- Customer notification log.
- Regulatory notification log (se applicable).
- Erasure attestation Ed25519 (S-14 customers).

## Escalation

- Sustained > 30d sem complete erasure → SEV-1 + CEO + Legal + Compliance officer + regulatory notification likely required.
- Pattern recurrent (> 2 incidents em 90d) → architectural review + Privacy team retrospective.

## References

- `invariant_registry.md` INV-DATA-ERASURE-COMPLETE (§3.5).
- `specs/04_sprints/S11/_spec_contract.md` (10-backend erasure + verification 24h R-S11-6).
- `specs/04_sprints/S14/_spec_contract.md` (erasure attestation Ed25519 R-S14-10).
- `specs/03_architecture/privacy_model.md`.
- `specs/03_architecture/error_taxonomy.md` `COR_DSR_*`.
- LGPD Art. 18 (right to erasure) + Art. 48 (incident notification).
- GDPR Art. 17 (right to erasure) + Art. 33 (breach notification).
- CCPA §1798.105 (right to delete).
- EDPB Guidelines 5/2020 (pseudonymization).
- NIST SP 800-88 Rev.1 (crypto-erase via key destroy).
