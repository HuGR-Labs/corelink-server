---
id: "resizing"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "resizing", "display", "wave-devenv"]
---

# CoreLink DevEnv — Viewport Resizing & Presets

Dynamically adjust the remote X11 desktop display resolution without interrupting running processes.

---

## 1. Supported Resolution Presets
- **1080p Full HD**: `1920x1080` (Default)
- **1440p Quad HD**: `2560x1440`
- **4K Ultra HD**: `3840x2160`
- **Custom Viewport**: Configurable up to `4096x2160`

## 2. Triggering Display Resize via API
```bash
curl -X POST https://api.corelink.humangr.com/v1/customer/devenv/resize \
  -H "Authorization: Bearer cl_pat_..." \
  -H "Content-Type: application/json" \
  -d '{"width": 2560, "height": 1440}'
```
