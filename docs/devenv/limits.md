---
id: "limits"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "limits", "quotas", "pricing", "wave-devenv"]
---

# CoreLink DevEnv — Limits, Quotas & Pricing

---

## 1. Hardware Tiers

| Tier | vCPUs | Memory | Disk (CAS) | Rate / Hour |
| :--- | :---: | :---: | :---: | :---: |
| **Standard** | 2 | 4 GB | 50 GB | Included in Base Plan |
| **Performance** | 8 | 16 GB | 200 GB | $0.30 / vCPU-h |
| **GPU (Preview)** | 8 + GPU | 32 GB | 500 GB | Custom Quote |

## 2. Resource Limits & Throttling
- **Max Inactive Idle Time**: 15 minutes before automatic hibernation.
- **Max Continuous Run Duration**: 24 hours per session.
- **Max Snapshot Size**: 50 GB per snapshot manifest.
- **WebSocket Concurrency**: Up to 100 concurrent WS sessions per tenant DO.
