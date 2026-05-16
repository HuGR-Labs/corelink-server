---
id: "DEMO-COMPETITIVE-COMPARISON"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "VPMkt"
final_approver: "Gustavo Schneiter"
reviewers: ["CTO", "VPSec", "Legal"]
supersedes: null
superseded_by: null
parent: "marketing/launch/LAUNCH-CHECKLIST-V2.md §2 (T-7d demos recorded)"
tags:
  - "marketing"
  - "launch"
  - "demo"
  - "competitive"
  - "r8"
---

# 3-minute competitive comparison demo — CoreLink vs `bazel-remote`

> **Audience:** technical evaluator already running `bazel-remote` (or similar OSS remote cache) who's questioning whether to replace a working OSS component with a vendor.
> **Goal:** demonstrate concretely — same artifact, two stores, side-by-side — what `bazel-remote` does *not* surface that CoreLink does: audit chain, per-tenant boundary, BYOK envelope, residency pin.
> **Tone:** respectful of OSS. We are not attacking `bazel-remote`; we explain a different scope. Engineering audiences punish sloppy comparison spray.
> **Cross-refs:** `5-MIN-DEEPDIVE.md`, `byok-deep-dive-demo.md`, `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md`.

---

## Time budget — exact

| Section | Duration | Cumulative | Lane |
|---|---|---|---|
| 0. Frame the comparison honestly | 0:00 → 0:20 | 0:20 | Title card |
| 1. Setup — same artifact, two stores | 0:20 → 0:50 | 0:50 | Split: 2× terminal |
| 2. Side-by-side store + retrieve | 0:50 → 1:30 | 1:30 | Split: 2× terminal |
| 3. Audit chain — what each surfaces | 1:30 → 2:15 | 2:15 | Split: bazel-remote logs + CoreLink admin UI |
| 4. Multi-tenant boundary | 2:15 → 2:45 | 2:45 | CoreLink admin UI |
| 5. Honest summary table | 2:45 → 3:00 | 3:00 | Static table |

Total = 3 minutes. Hard cap. Section 3 is the load-bearing beat — the visible difference between a flat access log and a Merkle-rooted audit chain.

---

## Section 0 — Frame the comparison honestly (0:00 → 0:20, 20s)

**Visual:** title card.

```
bazel-remote   vs   CoreLink
Same job:     remote cache for Bazel artifacts.
Different scope.
```

**Voiceover (~18s):**

> "`bazel-remote` is a great piece of software. It is a single-tenant, low-overhead remote cache for Bazel and Buck2. If you ship into a single org, on hardware you control, with no regulatory pressure, you may not need anything else. CoreLink is a different scope: multi-tenant, audit-chained, residency-pinned, BYOK-capable. This three-minute clip shows the difference on a single artifact."

---

## Section 1 — Setup (0:20 → 0:50, 30s)

**Visual:** split-screen terminal — left lane `bazel-remote`, right lane `corelink`.

**Pre-condition:** a local `bazel-remote` instance running at `http://localhost:9092` (the canonical default port). A CoreLink sandbox tenant `acme-build-cache` ready.

**Commands shown (no VO — let viewers read):**

Left lane (bazel-remote, the canonical CLI is `bazel run` with `--remote_cache`):

```bash
# Same artifact on both sides — a 4MB release tarball.
$ ls -la build/release-v2.tar.gz
-rw-r--r-- 1 user user 4194304 May 15 12:00 build/release-v2.tar.gz

$ sha256sum build/release-v2.tar.gz
3f4e1c2a9b8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2  build/release-v2.tar.gz
```

Right lane (CoreLink):

```bash
$ corelink version
corelink 1.0.0 (build sha=abc1234 slsa=…)

$ corelink doctor
8/8 checks PASS
```

**Voiceover (~25s):**

> "Same 4-megabyte release tarball. SHA-256 fingerprint computed on the left for bazel-remote; CoreLink will compute its own BLAKE3 digest on the right. Note both stores are happy — no errors, no setup theatre. This is a working comparison, not a strawman."

---

## Section 2 — Side-by-side store + retrieve (0:50 → 1:30, 40s)

**Visual:** split-screen terminal continues.

**Left lane (bazel-remote — store via HTTP PUT, the documented surface):**

```bash
$ curl -X PUT --data-binary @build/release-v2.tar.gz \
    "http://localhost:9092/cas/3f4e1c2a9b8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2"
# HTTP/1.1 200 OK

$ curl -o /tmp/restored.tgz \
    "http://localhost:9092/cas/3f4e1c2a9b8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2"
# 4194304 bytes retrieved
```

**Right lane (CoreLink):**

```bash
$ corelink put build/release-v2.tar.gz
digest=9c4d2a1f…  size=4.0 MB  uploaded=ok

$ corelink get 9c4d2a1f… --output /tmp/restored.tgz
downloaded 4.0 MB, blake3 verified ok
```

**Voiceover (~35s):**

> "Store and retrieve on both sides. Both work. The transport surfaces are different — bazel-remote is HTTP PUT/GET by SHA-256; CoreLink is `corelink put`/`get` by BLAKE3 with mandatory client-side verification. CoreLink will refuse to hand you bytes that fail re-verify. bazel-remote returns what the disk gave it. Neither is wrong — they're choices. Notice neither one took more than a second. Latency is not the differentiator. The next two beats are."

---

## Section 3 — Audit chain (1:30 → 2:15, 45s)

**Visual:** split-screen — left = a `bazel-remote` log tail; right = CoreLink admin UI audit page.

**Left lane (`bazel-remote` logs):**

```
$ tail -n 5 /var/log/bazel-remote/access.log
[12:34:56] 127.0.0.1 PUT /cas/3f4e1c2a... 200 4194304
[12:34:57] 127.0.0.1 GET /cas/3f4e1c2a... 200 4194304
```

**Right lane (CoreLink, `/en/admin/audit` — Shot #06 from `admin-ui-screenshot-guide.md`):**

A populated audit table with the `put` and `get` events visible — actor PAT prefix, source IP, digest, bytes, signed-root reference. Click into the detail page (Shot #07) — Merkle proof block visible.

**Voiceover (~40s):**

> "Left: bazel-remote's access log. Useful — you can see something happened. There's no actor identity, no per-tenant scope, no cryptographic chain, no signed root. If you wanted to hand this to a SOC 2 auditor as evidence of artifact integrity, you'd be hand-rolling a lot of glue. Right: CoreLink's audit page. Same event. Actor identity. Tenant scope. BLAKE3 digest. Click in — Merkle inclusion proof. The daily root is signed by our transparency-log key and the auditor verifies the proof against the public root. Don't trust — verify. This is the load-bearing difference for anyone shipping under SOC 2, HIPAA, LGPD, or any regulator that asks for cryptographic provenance."

---

## Section 4 — Multi-tenant boundary (2:15 → 2:45, 30s)

**Visual:** CoreLink admin UI.

**Screenshot reference:** Shot #09 (tenant list with multi-tenant boundary visible).

**Action sequence:**

1. Show `/en/admin/tenants` — two tenants visible (`acme-build-cache`, `acme-prod`) plus the "phantom" row indicator showing N other tenants exist but are not visible to this admin.
2. Cut to terminal: attempt a `corelink stat` against a digest known to exist in a tenant we don't have access to (using a digest known from another tenant by collision-of-luck).

```bash
$ corelink stat 9c4d2a1f… --tenant other-customer
error: COR_AUTH_TENANT_FORBIDDEN
       PAT does not have access to tenant 'other-customer'.
```

The CLI exits with code 1 — show it.

**Voiceover (~25s):**

> "Multi-tenant boundary. bazel-remote is single-tenant by design — if you need multi-tenancy you run multiple instances and federate them yourself. CoreLink is multi-tenant by design. Even if I knew an exact digest in another customer's tenant, my PAT cannot read it — the boundary is enforced at the request, not at the URL. This is the difference between 'we trust the network' and 'we trust nothing'."

---

## Section 5 — Honest summary table (2:45 → 3:00, 15s)

**Visual:** static table.

| Capability | `bazel-remote` | CoreLink |
|---|---|---|
| BLAKE3 + client-verify mandatory | No (SHA-256 trust-on-write) | **Yes** |
| Multi-tenant by design | No | **Yes** |
| Per-tenant audit chain (Merkle-rooted, signed) | No | **Yes** |
| BYOK (4 providers, envelope encryption) | No | **Yes** |
| Residency pin enforced at routing | No | **Yes** |
| Dual-approval sensitive ops | No | **Yes** |
| Single-binary, local-cache, zero-config | **Yes** | Partial (CLI yes; server no) |
| Cost at small scale | **Near-zero** | Higher (sandbox free; paid above) |
| Recommended scope | Single-org, no regulator | Multi-tenant, regulated, audit-required |

**Voiceover (~13s):**

> "If your scope is single-org, no regulator, and you trust your network, bazel-remote is excellent. If your scope is multi-tenant, regulated, or audit-driven, CoreLink ships the additional surface as a product, not a roadmap. The choice is scope-driven, not better-versus-worse."

**End card hold for 2s with `corelink.humangr.com` URL.**

---

## Pre-flight checklist

- [ ] Local `bazel-remote` running at `http://localhost:9092` with a 4MB tarball already cached (avoid first-run setup on stage).
- [ ] CoreLink sandbox tenant `acme-build-cache` warm + a 4MB artifact pre-put for §2.
- [ ] A second tenant `other-customer` exists (with no PAT-level access for the demo persona) for §4's forbidden-stat demo.
- [ ] Pre-recorded backup of `bazel-remote` log output in case the local instance behaves oddly on stage.
- [ ] Captions burned in; "Honest summary table" rendered as crisp SVG (table images compress poorly).

## Comparison-claim audit

Each "No" or "Partial" in the summary table is sourced — challenge gracefully.

- **`bazel-remote` BLAKE3 + client-verify:** `bazel-remote` uses SHA-256 by default; client-side verification is optional and not the default. (Source: `bazel-remote` README, accessed 2026-05-15.)
- **`bazel-remote` multi-tenancy:** README explicitly scopes single-cache, single-namespace. Federation is left to the operator.
- **`bazel-remote` audit chain:** access logs are the surface; no signed-root concept exists in the project.
- **`bazel-remote` BYOK:** not in scope of the project.
- **`bazel-remote` residency:** not in scope of the project (network-level concern).
- **`bazel-remote` dual approval:** not in scope of the project.

If `bazel-remote` adds any of these capabilities post-launch, **update this demo within 30 days** — comparison drift is reputational risk.

## Cross-reference

- `5-MIN-DEEPDIVE.md` — orientation demo if the viewer is brand new to CoreLink.
- `byok-deep-dive-demo.md` — follow-up if BYOK was the differentiator that landed.
- `admin-ui-screenshot-guide.md` shots #06, #07, #09 — capture sources.
- `marketing/launch/BLOG-POSTS/05-fast-cache-hit-economics.md` — cost-side narrative complement.
- `marketing/launch/BLOG-POSTS/03-audit-chain-merkle-proofs.md` — audit-chain technical depth.
- `marketing/launch/LAUNCH-CHECKLIST-V2.md` row T-7d ("demos recorded") — gating dependency.
