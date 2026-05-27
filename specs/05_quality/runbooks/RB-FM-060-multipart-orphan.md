---
id: "RB-FM-060"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p2", "multipart", "r2", "stub"]
---

# RB-FM-060 — Multipart Upload Orphan Accumulation

> **FM:** FM-060 (S=3, O=3, D=2, RPN=18, P2) | **CTRL:** PAT-SWEEPER-001 | **SLA:** detect ≤ 24h, mitigate ≤ 7d (auto-abort)

## Detecção

- Métrica `corelink.multipart.ongoing_sessions` cresce sustained sem completion proportional.
- R2 cost spike em `MultipartUpload` ongoing parts (cobrado mesmo sem complete).
- Sweeper cron DO logs: many sessions > 7d.

## Comunicação

- **SEV-3** (cost concern; não impacta production functionality).
- Page SRE on-call.
- Internal channel.

## Mitigação imediata

1. Confirmar via R2 admin API: `ListMultipartUploads` retorna count.
2. Sweeper ativa: verificar logs cron DO último run.
3. Se sweeper inactive: trigger manual abort batch.
4. Verificar root cause: client disconnect padrão? Network issues? Specific tenant?

## Resolução

- Hot fix: manual abort orphan parts > 7d.
- Cold fix:
  - Tighten sweeper cadence se needed (default daily).
  - Investigate root cause (network instability, client SDK bug, specific tenant misuse).
  - Customer outreach se tenant-specific.

## References

- `failure_modes.md` FM-060.
- `specs/04_sprints/_sealed/S05/_spec_contract.md`.
- R2 multipart limits: <https://developers.cloudflare.com/r2/api/s3/multipart-uploads/>.
