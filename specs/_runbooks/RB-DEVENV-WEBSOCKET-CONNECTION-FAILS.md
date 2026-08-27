---
id: "RB-DEVENV-WEBSOCKET-CONNECTION-FAILS"
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

# RB-DEVENV-WEBSOCKET-CONNECTION-FAILS — WebSocket Edge Proxy Degradation

> **Severity floor:** P1
> **Detect → Acknowledge → Engage signers:** SRE on-call.
> **Companion docs:** `RB-INCIDENT-RESPONSE.md`.

---

## 1. Symptoms
- Web UI receives WebSocket 1006 / 1011 disconnects.
- `noVNC` binary frame negotiations failing on port 6080.

## 2. Diagnosis
1. Verify subprotocol negotiation headers in Cloudflare edge logs (`Sec-WebSocket-Protocol: binary`).
2. Verify in-container `websockify` process is healthy on port 6080.

## 3. Resolution
- Restart supervisord web proxy workers inside the microVM via `corelink-check-exec-server` or trigger DO warm resume.
