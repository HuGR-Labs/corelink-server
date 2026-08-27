---
id: "quickstart"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "quickstart", "guide", "wave-devenv"]
---

# CoreLink DevEnv — 5-Minute Quickstart

CoreLink DevEnv gives developers full desktop, terminal, and VS Code environments running inside ephemeral, hardware-isolated Cloudflare microVM containers with instant CAS-backed workspace snapshotting.

---

## 1. Prerequisites

- CoreLink Personal Access Token (`cl_pat_...`) or active Web UI session.
- `clw` CLI installed locally:
  ```bash
  curl -fsSL https://releases.corelink.humangr.com/clw/install.sh | sh
  ```

---

## 2. Launching from the Web Dashboard

1. Navigate to `https://corelink.humangr.com/customer/devenv`.
2. Click **Start DevEnv**.
3. Select your hardware tier (`standard-2`, `standard-4`, `power-8`, or `ultra-16`).
4. Once the state transitions to `RUNNING`, choose your connection mode:
   - **VS Code Editor**: Embedded browser-based code-server.
   - **Terminal**: Low-latency `ttyd` interactive shell.
   - **Desktop (noVNC)**: Full graphical desktop with browser and audio.

---

## 3. Launching from the CLI

```bash
# Export your authentication token
export CORELINK_PAT="cl_pat_your_token_here"

# Start the remote DevEnv instance
clw devenv start --tier standard-4 --name my-feature-branch

# Open an interactive SSH / terminal session
clw devenv attach --tty

# Snapshot your local workspace into the running remote environment
clw snapshot --name my-feature-branch
```

---

## 4. Lifecycle & Auto-Hibernation

- DevEnvs automatically hibernate when idle for more than 30 minutes.
- When hibernated, memory and vCPU billing stops, and your workspace is safely preserved in Content-Addressed Storage (CAS).
- Reconnecting to any endpoint instantly wakes the container in < 5 seconds.
