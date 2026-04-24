---
id: "RB-FM-007"
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
tags: ["runbook", "p1", "security", "rce", "supply-chain"]
---

# RB-FM-007 — Deserialization RCE (dep corrupted or bad serde config)

> **FM:** FM-007 (S=5, RPN=25, P1 S=5→upgrade) | **CTRLs:** CTRL-INPUT-003 + PAT-INPUT-HARDEN-001 | **SLA:** mitigate < 1h

## Detecção

- Alert `corelink_panic_total > 0` com stack trace contendo `serde::de::Error` suspeito.
- SAST (semgrep) detecta novo uso de `bincode::deserialize` ou `serde_json::from_slice` sem `deny_unknown_fields`.
- Fuzz run encontra crash em parser de input.
- Customer/security researcher reporta payload malicioso via bug bounty.

## Comunicação

- **SEV-1** (RCE = possibilidade de comprometer todo Worker isolate).
- Page Security Lead + Architect + CEO.
- Se confirmado exploit ativo: status page + customer notification plan.

## Mitigação imediata (≤ 30 min)

1. **Disable endpoint afetado** via kill-switch (config flag).
2. Se unclear qual endpoint: `degrade_mode=read-only` enquanto triagem.
3. Analise stack trace: qual dep? qual struct? qual input trigger?
4. Snapshot do Worker state antes de restart (evidence).

## Mitigação completa (≤ 1h-4h)

1. Patch hot-fix:
   - Adicionar `#[serde(deny_unknown_fields)]` a struct afetada.
   - Substituir `from_slice` por `from_slice::<ExactType>` com type explícito.
   - Bound explícito para size de input.
2. Deploy progressive rollout.
3. Se dep root cause: bump dep (ver advisory) + `cargo audit`.
4. Se supply chain: checar SBOM + cosign verify.

## Forensics

1. Audit log: quais inputs foram processados antes do crash?
2. Fuzz corpus: adicionar input trigger ao corpus pra prevenir regressão.
3. SAST rule: adicionar regra específica pra pattern.
4. Se bug bounty: pay out + disclosure policy 90d.

## Post-mortem

- Obrigatório se exploit foi ativo ou se foi publicly disclosed.
- Revisar CTRL-INPUT-003 policy.
- Considerar sandboxing adicional (gVisor / Firecracker tighter).

## Prevenção

- CTRL-INPUT-003 rigorously enforced em CI.
- Fuzz nightly em todos parsers (EVT-008).
- Dep audit diário (CTRL-SUPPLY-004).
- `cargo-deny` com deny raw `unsafe`; whitelist explícita.
- Bug bounty program ativo (CTRL-SUPPLY-* + CTRL-AUDIT-004).
