---
id: "faq"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "faq", "wave-devenv"]
---

# CoreLink DevEnv — Frequently Asked Questions

---

### Q1: Is my data lost when the container stops?
No. All files under `/workspace` are backed by Content-Addressed Storage (CAS) snapshots and automatically re-hydrated whenever a container starts.

### Q2: Can multiple team members connect to the same DevEnv?
Yes. WebSocket proxies support multiple concurrent connections to the same desktop (noVNC) or terminal sessions for real-time pair programming.

### Q3: What happens if I exceed my monthly vCPU quota?
API requests to create new DevEnvs will return HTTP 402 with quota upgrade options. Active environments are permitted to finish their current session before stopping.
