# HuGR Labs — build infra for regulated polyglot orgs

CoreLink is the REAPI v2 + multi-package-manager cache for engineering
teams that need BYOK, residency honesty, and a re-derivable audit log.

## Featured

**[corelink-server](https://github.com/HuGR-Labs/corelink-server)**
Multi-tenant content-addressable cache — Bazel, Cargo, npm, pip, OCI.
REAPI v2 compatible. Per-tenant BYOK (AWS KMS; GCP/Azure/Vault on the
roadmap). RFC-6962-style audit chain
(BLAKE3 + Ed25519). TLA+-proved cross-tenant isolation. Cloudflare Workers
+ Durable Objects + Containers + R2 + D1. Free tier: 10 GB / 500k req/mo.

**[corelink-cli](https://github.com/HuGR-Labs/corelink-cli)**
Install in one line; drop into any Bazel, Buck2, Cargo, Docker, or ML
pipeline. Cross-compiled binaries for Linux x64/arm64, macOS arm64/x64,
and Alpine. Zero config after `corelink login` — it saves your token and
resolves your tenant; `corelink bazel-init` wires a Bazel repo in one shot.

**[HuGR-Labs](https://github.com/HuGR-Labs/HuGR-Labs)**
This profile — maintained as a source-of-truth index of what ships and
what is still in progress. Honesty policy: nothing is listed as
shipped unless CI turns green.

**[.github](https://github.com/HuGR-Labs/.github)**
Org-wide community health: CODE_OF_CONDUCT, CONTRIBUTING, SECURITY, and
the engineering philosophy that governs every repo in the org.

## About HuGR

HuGR = Human Guardrail. We ship developer infrastructure that defaults to
BYOK, residency honesty, and verifiable audit logs — for the engineering
teams whose compliance posture means "the auditor will ask, and the answer
must be reproducible."

Current status: **launched, self-serve** (GA since 2026-07-10). Free tier
available; no card required to start.

What is verifiable today:

- BLAKE3 + SHA-256 addressing; per-tenant CAS namespace
- Tenant isolation modelled in TLA+ (invariant breaks CI on regression)
- Per-tenant append-only Merkle audit chain, Ed25519-signed, replayable
- Multi-region on Cloudflare R2: US + EU today (WNAM/ENAM/WEUR); South
  America (SAM) and APAC are on the roadmap and rejected at signup today
- Rust workspace: `#![forbid(unsafe_code)]`, cargo-deny lockdown, fuzz,
  property tests, mutation testing

What is not shipped yet (important — not buried):

- BYOK: AWS KMS planned first; GCP / Azure / Vault providers on the roadmap — not yet shipped
- SOC 2 Type II: gap analysis complete; Type II observation window is on the roadmap
- Production SLA contract: today it's best-effort against published target SLOs
- Signed DPA / SCC: Common Paper template only

## Founder

Gustavo Schneiter — solo founder, pre-PMF, build-in-public. Background in
platform engineering and distributed systems. CoreLink is the tool I wished
existed when I ran build infrastructure for polyglot orgs under SOC 2 audit.

No VC, no team yet — I run this solo, so monitoring and incident response
are best-effort rather than a staffed on-call rotation. First paying
customer gets a named thank-you in the changelog.

## Contact

- Engineering and product questions: gustavo@humangr.com
- Twitter / X: [@corelinkdev](https://twitter.com/corelinkdev)
- Sign up: [corelink-docs.humangr.com/pilot/apply](https://corelink-docs.humangr.com/pilot/apply)
- Docs: [corelink-docs.humangr.com](https://corelink-docs.humangr.com)
