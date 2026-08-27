---
id: "snapshots"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "snapshots", "cas", "persistence", "wave-devenv"]
---

# CoreLink DevEnv — Workspace Snapshots & Deduplication

How snapshots preserve workspace state across container restarts and migrations.

---

## 1. Content-Addressed Storage (CAS)
- Fast incremental uploads using Blake3 and chunk hashing.
- Only delta bytes that have changed since the last snapshot are transmitted.
- Bit-accurate restoration of file permissions, timestamps, and symlinks.

## 2. Automated Snapshots
- Triggered automatically before container graceful shutdown or auto-hibernation.
- In-container OOM or crash triggers an emergency sync pass before container recycling.

## 3. Manual Snapshots via CLI
```bash
clw snapshot --name my-feature --force
```
