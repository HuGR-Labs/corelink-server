---
id: "connections"
type: "guide"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-27"
updated: "2026-08-27"
owner: "Gustavo Schneiter"
tags: ["devenv", "connections", "vnc", "ttyd", "code-server", "wave-devenv"]
---

# CoreLink DevEnv — Connection Protocols

DevEnv routes traffic across three distinct edge WebSocket proxies through Cloudflare Durable Objects.

---

## 1. Desktop GUI (noVNC / Port 6080)
- **Protocol**: WebSocket RFB subprotocol (`binary`).
- **Client Support**: Compatible with modern desktop browsers and iOS 17+ Mobile Safari.
- **Audio & Clipboard**: Integrated browser clipboard sync and PulseAudio stream.

## 2. Interactive Terminal (ttyd / Port 7681)
- **Protocol**: UTF-8 stream with ANSI color rendering.
- **Shell**: Bash / Zsh with pre-configured developer toolchains (Rust, Go, Node, Python, Git).

## 3. Web Editor (code-server / Port 8080)
- **Protocol**: HTTP / WebSocket RPC.
- **Extensions**: Full OpenVSX extension marketplace support and settings sync.
