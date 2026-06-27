---
type: "ADR"
title: "ADR-S12-046 — License Allowlist: OSI-Approved Permissive + Banned Copyleft"
description: "Why CoreLink enforces a default-deny license allowlist of permissive OSI licenses (plus copyleft = deny defence-in-depth) at the cargo-deny CI gate to keep its shipped WASM blob free of GPL/AGPL copyleft exposure."
source_files:
  - "specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md"
  - "deny.toml"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s12", "supply-chain", "license-allowlist", "compliance"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S12-046 — License Allowlist: OSI-Approved Permissive + Banned Copyleft

CoreLink ships closed-source Cloudflare Workers WASM binaries built from a transitive crates.io dependency graph. A single silently-introduced GPL/AGPL transitive dep would subject that shipped blob to copyleft obligations, with post-ship remediation requiring legal removal + re-deploy. This ADR enforces a **default-deny** license allowlist of permissive OSI licenses, plus a `copyleft = "deny"` defence-in-depth layer, at the cargo-deny CI gate — so any new license requires explicit ADR review.

# Context

CoreLink ships closed-source WASM with a transitive Rust dependency graph; without an explicit license allowlist at CI level, a transitive GPL/AGPL dep could expose CoreLink to copyleft (distribute source or open the SaaS under copyleft terms). cargo-deny license enforcement is the industry pattern (Mozilla Servo, Fuchsia, Bevy, Tauri, Cloudflare) (`specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md:23-35`).

# Decision

A default-deny allowlist of permissive SPDX expressions (MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2/3-Clause, ISC, MPL-2.0, Unicode-DFS-2016/3.0, Zlib, CC0-1.0, 0BSD) in `deny.toml [licenses].allow`, with explicit bans on GPL-*/AGPL-*/SSPL-1.0/Commons-Clause/BUSL-1.1 plus `copyleft = "deny"` to catch any copyleft (LGPL/EUPL/CDDL/EPL) not explicitly listed. `[licenses].default = "deny"` and `unlicensed = "deny"` make anything unlisted or undeclared fail; private workspace crates (`publish = false`, `UNLICENSED`) are exempt. A new license addition requires an ADR (OSI status + legal review) + Compliance sign-off + a `deny.toml` bump (`specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md:37-90`). Rationale: an allowlist is default-deny safe (a new copyleft license is caught, unlike a denylist), `copyleft = "deny"` is defence-in-depth, MPL-2.0 is allowed because its file-level weak copyleft does not force opening non-MPL code, and the Unicode licenses are permissive data licenses needed by `icu_data`/`unicode-ident` (`specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md:92-118`).

# Consequences

Zero GPL/AGPL exposure via automated enforcement at every PR; quarterly Legal review limited to a 5% sample since the gate catches most drift; and a clear ADR + sign-off process for new licenses. Trade-offs: some useful crates may use non-allowlisted licenses and need a per-exception ADR (mitigated since most mature crates are MIT/Apache dual-licensed), and SPDX-expression drift (a crate declaring MIT but carrying a GPL fragment) is not caught by the tool — mitigated by the quarterly Legal sample audit (`specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md:120-131`).

# Status vs shipped code

The "7-osi" slug **undercounts the live allowlist**. The shipped `deny.toml [licenses].allow` is wider
than seven entries and additionally permits `BSL-1.0` (`deny.toml:146`) and `CDLA-Permissive-2.0`
(`deny.toml:155`) — both ratified by ADR-S32-001 (`BSL-1.0` for `ryu`/`ryu-js`, the CDLA data license).
The default-deny + `copyleft = "deny"` posture and the per-license ADR-addition process are intact; only
the headline count is stale — read the allowed set off `deny.toml`, not the slug.

# Citations

1. `specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md:23-35` — Context: closed-source WASM + copyleft exposure risk + industry pattern.
2. `specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md:37-90` — Decision: allowlist, banned set, default-deny, private exemption, addition process.
3. `specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md:92-118` — Rationale: allowlist-not-denylist, copyleft-deny, MPL-2.0, Unicode licenses.
4. `specs/03_architecture/adrs/ADR-S12-046-license-allowlist-7-osi.md:120-131` — Consequences and mitigated trade-offs.
