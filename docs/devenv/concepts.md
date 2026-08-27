---
id: "concepts"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "concepts", "architecture", "wave-devenv"]
---

# CoreLink DevEnv — Core Concepts

Understanding the architecture, lifecycle state machine, and data persistence guarantees of CoreLink DevEnv.

---

## 1. Container & Durable Object Topology

Each tenant's DevEnv is anchored by a singleton **Cloudflare Container Durable Object** (`RunnerDevEnvDO`):
- **MicroVM Isolation**: Secure gVisor/firecracker virtualization boundary.
- **WebSocket Hibernation**: Keeps zero idle connections active on edge servers while preserving client session state.
- **Supervisor (`supervisord`)**: Manages in-container services:
  - `ttyd` on port 7681
  - `noVNC / websockify` on port 6080
  - `code-server` on port 8080
  - `corelink-check-exec-server` on port 9090

---

## 2. Strict State Machine (INV-I2)

The DevEnv state transitions adhere strictly to the validated invariant matrix:

```mermaid
stateDiagram-v2
    [*] --> stopped
    stopped --> starting: start()
    starting --> running: health check 200
    starting --> errored: timeout / failure
    running --> stopping: stop()
    running --> errored: crash / OOM
    stopping --> stopped: clean teardown
    stopping --> errored: forced kill failure
    errored --> starting: restart()
    errored --> stopped: reset()
```

---

## 3. Workspace Persistence & CAS

Workspaces are decoupled from container ephemeral lifecycles:
- **`clw snapshot`**: Content-Addressed chunking (CDC) deduplicates modified files and pushes chunks to R2 CAS.
- **`clw hydrate`**: Reconstructs exact workspace filesystem trees into `/workspace` upon container cold boot.
- **D1 Metadata**: Tracks tenant monthly vCPU usage, snapshot references, and hardware tiers.
