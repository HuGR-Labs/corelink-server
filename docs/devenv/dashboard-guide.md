---
id: "dashboard-guide"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "dashboard", "ui", "wave-devenv"]
---

# CoreLink DevEnv — Dashboard User Guide

The CoreLink DevEnv Web Dashboard (`/customer/devenv`) provides real-time control, live status telemetry, and multi-protocol connection cards.

---

## 1. Navigating to DevEnv

1. Log in to your CoreLink console via Clerk authentication.
2. Select **Dev Environments** from the main customer navigation bar.

---

## 2. Interface Elements

- **Status Banner**: Displays current lifecycle state (`stopped`, `starting`, `running`, `stopping`, `errored`) with live color badges.
- **Hardware Tier Selector**: Allows switching between `standard-2` (2 vCPU / 4 GB), `standard-4` (4 vCPU / 8 GB), `power-8` (8 vCPU / 16 GB), and `ultra-16` (16 vCPU / 32 GB) tiers when starting an environment.
- **Connection Modalities**:
  - **Open Code Server**: Opens VS Code Web in a dedicated tab.
  - **Open Terminal**: Opens an in-browser `ttyd` interactive terminal.
  - **Open Desktop (VNC)**: Launches full graphical environment with keyboard and mouse capture.
- **Control Actions**:
  - **Create Snapshot**: Triggers manual snapshot synchronization.
  - **Stop Environment**: Initiates graceful shutdown and persistence flush.
  - **Resize Display**: Adjusts remote X11 viewport to 1080p, 1440p, or custom dimensions.
