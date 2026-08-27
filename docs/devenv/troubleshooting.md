---
id: "troubleshooting"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "troubleshooting", "debug", "wave-devenv"]
---

# CoreLink DevEnv — Troubleshooting Guide

Common issues, diagnostic procedures, and recovery steps.

---

## 1. Connection Timeout or WebSocket 1006
- **Cause**: The container is waking from deep hibernation or network was temporarily partitioned.
- **Solution**: Refresh the browser page or re-run `clw devenv status`. Reconnection completes automatically in < 5 seconds.

## 2. In-Container Process OOM
- **Cause**: Workload exceeded assigned container memory limit.
- **Solution**: Use the dashboard to upgrade to the `power-8` hardware tier or check `dmesg` inside the terminal.

## 3. Snapshot Lock Conflict
- **Cause**: An automated snapshot pass is currently writing deltas to CAS.
- **Solution**: Wait for the active sync pass to complete or supply `--force` in emergency recovery scenarios.
