---
id: "DEMO-5-MIN-DEEPDIVE"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "VPMkt"
final_approver: "Gustavo Schneiter"
reviewers: ["CTO", "VPSec", "CS-OC"]
supersedes: null
superseded_by: null
parent: "marketing/launch/LAUNCH-CHECKLIST-V2.md §2 (T-7d demos recorded)"
tags:
  - "marketing"
  - "launch"
  - "demo"
  - "deep-dive"
  - "r8"
---

# 5-minute deep dive demo — CoreLink GA

> **Format:** website hero embed (1920×1080 native, web-optimized for autoplay-muted with captions burned in).
> **Audience:** technical evaluator who clicked "watch demo" from `corelink.humangr.com` after reading the press release or one blog post.
> **Tooling:** asciinema for the terminal lane (cast from `cli-asciinema-script.sh`); OBS or screen.studio for cuts; Loom-style picture-in-picture acceptable but optional.
> **Voice:** narrator off-screen. Pace = unhurried but no dead air. **300 words/minute of speech is the ceiling.** Below that = reads boring on engineering audiences.
> **Cross-refs:** `60-SEC-ELEVATOR.md`, `cli-asciinema-script.sh`, `admin-ui-screenshot-guide.md`, `byok-deep-dive-demo.md`, `competitive-comparison-demo.md`.

---

## Time budget — exact

| Section | Duration | Cumulative | Lane |
|---|---|---|---|
| 0. Cold open + framing | 0:00 → 0:20 | 0:20 | Narrator + title card |
| 1. Tenant signup (onboarding flow) | 0:20 → 1:10 | 1:10 | Admin UI (screen recording) |
| 2. First CAS write (CLI) | 1:10 → 2:00 | 2:00 | Terminal (asciinema) |
| 3. Cross-language retrieval | 2:00 → 2:50 | 2:50 | Split: terminal + code snippet |
| 4. Audit trail viewing | 2:50 → 3:40 | 3:40 | Admin UI (screen recording) |
| 5. BYOK setup walkthrough | 3:40 → 4:40 | 4:40 | Admin UI |
| 6. Wrap-up + CTA | 4:40 → 5:00 | 5:00 | End card |

Total = 5 minutes. Soft cap +10s tolerated. Section 5 (BYOK) is the most under-pressure; if you bleed, eject the dual-approval click but **not** the kill-switch click. The kill-switch is the moment buyers tell us they "got it."

---

## Section 0 — Cold open + framing (0:00 → 0:20, 20s)

**Visual:** title card.

```
CoreLink — content-addressable cache for modern build systems
GA · May 2026
5-minute deep dive
```

**Voiceover (~18s):**

> "CoreLink is a multi-tenant content-addressable cache for Bazel, Buck2, container images, and ML model weights — with per-tenant audit trails, customer-managed encryption keys across four providers, and a region-pinning model that respects data residency. In the next five minutes, we'll walk a brand-new tenant from signup to a production-grade BYOK configuration."

**Cut:** crossfade into the admin UI sign-in page (`/sign-in`).

---

## Section 1 — Tenant signup (0:20 → 1:10, 50s)

**Goal:** show that the path from "never seen CoreLink" to "first PAT in clipboard" is six clicks and under one minute.

### 1.1 — Sign-in (0:20 → 0:30)

**Screen:** browser at `/sign-in`. Clerk-rendered sign-in UI with Google / GitHub / email buttons.

**Click:** "Sign in with GitHub" → OAuth redirect handled instantly (use a warm-cached test account `demo-eval@humangr.com` so OAuth round-trip is sub-second).

**Voiceover (~9s):**

> "Sign in with your existing SSO. We use Clerk under the hood — no separate credential to manage, no credit card prompted until you leave the sandbox tier."

### 1.2 — Tenant creation (0:30 → 0:42)

**Screen:** `/[locale]/onboarding/tenant`. Form has fields: tenant display name, slug, default residency region.

**Action:** type `acme-build-cache` as display name → slug auto-fills `acme-build-cache`. Select region `us-west-2` from the residency dropdown.

**Click:** "Create tenant".

**Voiceover (~11s):**

> "Pick a tenant name and a default region. Residency is enforced cryptographically and at the routing layer — once you set `us-west-2`, your blobs do not leave that region without an explicit replication policy."

### 1.3 — Region plan (0:42 → 0:52)

**Screen:** `/[locale]/onboarding/region-plan`. Shows region matrix (us-west-2 selected, plus an optional EU-Frankfurt and AP-Sydney secondary).

**Action:** leave defaults, click "Continue".

**Voiceover (~9s):**

> "You can add secondary regions for replication or read-through caches — we'll skip that for now and stay single-region."

### 1.4 — Skip billing (sandbox tier) (0:52 → 1:00)

**Screen:** `/[locale]/onboarding/billing`. Banner at top says "Sandbox tier — no credit card required."

**Action:** click "Start with sandbox tier" (skip-billing CTA).

**Voiceover (~7s):**

> "We'll use the sandbox tier — no credit card, twenty-four-hour scratch tenant. The path to production is one upgrade click later."

### 1.5 — PAT issuance (1:00 → 1:10)

**Screen:** `/[locale]/onboarding/pat`. The PAT is shown once, with a "Copy" button and an explicit "I have saved this token" checkbox.

**Action:** click "Copy". The PAT format `corelink_sandbox_t_xxx.xxx.xxx` is highlighted on-screen with a brief redaction overlay (real characters blurred for the recording).

**Voiceover (~9s):**

> "Your Personal Access Token. Shown once. Copy it to a secret manager — never paste it into source. Note that the CoreLink CLI deliberately *refuses* a `--pat` flag — credentials only via env var or config file. That's security control CTRL-CRED-001."

**Cut:** to terminal.

---

## Section 2 — First CAS write (1:10 → 2:00, 50s)

**Lane:** terminal full-bleed.
**Source:** asciinema cast from `cli-asciinema-script.sh` step 2.

### 2.1 — Export PAT + doctor (1:10 → 1:25)

```bash
$ export CORELINK_PAT="corelink_sandbox_t_xxx.xxx.xxx"
$ corelink doctor
8/8 checks PASS  — net OK, auth OK (tenant=acme-build-cache, ttl=23h58m), storage OK, BYOK n/a (sandbox), region OK (us-west-2), quota OK, client-verify OK, telemetry OK
```

**Voiceover (~13s):**

> "The eight diagnostic checks confirm network, auth, storage backend, BYOK posture, region routing, quota headroom, client-side verifier, and telemetry. Anything red here would block your CI before it ever sends a byte."

### 2.2 — Config defaults (1:25 → 1:35)

```bash
$ corelink config set defaults.tenant_id acme-build-cache
$ corelink config list
auth.pat                  = ***REDACTED***
defaults.tenant_id        = acme-build-cache
telemetry.enabled         = false
telemetry.anonymized_id   = 6c2f-…
```

**Voiceover (~8s):**

> "Set tenant default once. Telemetry is opt-in, default-off, with an anonymised ID you can rotate at any time."

### 2.3 — First put (1:35 → 1:50)

```bash
$ echo "hello, corelink — $(date)" > /tmp/hello.txt

$ corelink put /tmp/hello.txt --output json | tee /tmp/put.json
{"digest":"af1c3e9b8d2c5e7f4a6b9c1d3e5f7a8b2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1","size":43,"uploaded":true,"tenant_id":"acme-build-cache"}
```

**Voiceover (~13s):**

> "Upload any file. CoreLink computes the BLAKE3 digest client-side — that's a 256-bit content address, collision-resistant, and tree-hashable for streaming. The digest *is* the storage key — no path namespace to keep coherent across machines."

### 2.4 — Stat + ls (1:50 → 2:00)

```bash
$ DIGEST=$(jq -r .digest /tmp/put.json)
$ corelink stat "$DIGEST"
digest=af1c3e9b…  size=43  exists=true  first_seen=2026-05-15T12:34:56Z  last_seen=2026-05-15T12:34:56Z
```

**Voiceover (~8s):**

> "`stat` shows existence, size, and the first/last seen timestamps — useful for cache-hit analysis."

---

## Section 3 — Cross-language retrieval (2:00 → 2:50, 50s)

**Lane:** split screen — left = terminal (40% width), right = Python source code being typed in by a fake "second engineer".

**Goal:** prove the four SDKs share the same canonical verifier and that a digest produced by the CLI is round-trippable from any client.

### 3.1 — Retrieve via CLI (2:00 → 2:15)

```bash
$ corelink get af1c3e9b… --output /tmp/restored.txt
downloaded 43 bytes, blake3 verified ok

$ diff /tmp/hello.txt /tmp/restored.txt && echo "MATCH"
MATCH
```

**Voiceover (~13s):**

> "Same machine, fresh path. CoreLink re-verifies the BLAKE3 digest client-side before handing you the bytes. A mismatch fails closed with `COR_CAS_DIGEST_MISMATCH`. You never see silently corrupted data."

### 3.2 — Retrieve via Python SDK (2:15 → 2:35)

**Visual:** type out the snippet at ~80 wpm in a code editor lane on the right.

```python
import os
from corelink import CoreLinkClient

client = CoreLinkClient(
    pat=os.environ["CORELINK_PAT"],
    tenant_id="acme-build-cache",
)
data = client.get("af1c3e9b8d2c5e7f4a6b9c1d3e5f7a8b2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1")
print(data.decode())
# hello, corelink — Fri May 15 12:34:56 UTC 2026
```

**Voiceover (~18s):**

> "Same digest from Python. All four SDKs — Rust, Python, Go, JavaScript — call the same canonical Rust verifier via FFI. There's no per-language reimplementation of BLAKE3 to drift; one verifier, four bindings."

### 3.3 — Show Go + JS one-liner (2:35 → 2:50)

**Visual:** quick montage — three code panes flash through for ~5s each, no VO over them.

```go
data, _ := client.Get("af1c3e9b…")
```

```ts
const data = await client.get("af1c3e9b…");
```

```rust
let data = client.get("af1c3e9b…").await?;
```

**Voiceover (~12s):**

> "Same digest. Same verifier. Go, JavaScript, Rust. If you've used a content-addressable store before, this is the surface you wanted — minus the operational drift."

---

## Section 4 — Audit trail viewing (2:50 → 3:40, 50s)

**Lane:** admin UI full-bleed.

### 4.1 — Open audit page (2:50 → 3:05)

**Screen:** navigate to `/[locale]/admin/audit`. Page lists recent events. Filter pre-applied: last 5 minutes.

**Visible rows:** the `put` and `get` from sections 2 and 3, plus the four SDK gets from section 3.

**Voiceover (~13s):**

> "Audit. Every put and get against your tenant is recorded with actor identity, source IP, digest, byte count, and a timestamp. Filter by event type, actor, digest, or time window."

### 4.2 — Drill into one event (3:05 → 3:25)

**Action:** click the row for the original `put` event → navigate to `/[locale]/admin/audit/[event_id]`.

**Visible on detail page:** event metadata, the actor's PAT prefix (token suffix redacted), source IP, and a "Merkle proof" section showing the leaf hash + inclusion proof against the daily root.

**Voiceover (~18s):**

> "Each event is a leaf in a daily Merkle tree. The root is published, signed with our transparency-log key, and replayable. Your compliance team verifies the proof against the published root — they don't have to trust us, they verify."

### 4.3 — Export evidence (3:25 → 3:40)

**Action:** click "Export" button → download JSON evidence pack for the last hour.

**Voiceover (~13s):**

> "Export evidence as JSON — signed roots included — and hand it to your auditor. The same export feeds your SIEM if you want continuous attestation."

---

## Section 5 — BYOK setup walkthrough (3:40 → 4:40, 60s)

**Lane:** admin UI.

**Note:** the sandbox tenant does **not** support BYOK — this section uses a parallel "production-tier" tenant `acme-prod` that was pre-provisioned. State this explicitly on-screen so viewers don't try BYOK on a sandbox.

### 5.1 — Switch to prod tenant (3:40 → 3:50)

**Action:** tenant switcher → `acme-prod`. URL: `/[locale]/admin/tenants/acme-prod`.

**Voiceover (~8s):**

> "BYOK is an enterprise-tier feature. We've switched to a pre-provisioned production tenant for this part."

### 5.2 — Open KMS config (3:50 → 4:05)

**Action:** click "Encryption" tab on the tenant detail page. Shows the four-provider matrix: AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault. Currently "Vendor-managed" is selected.

**Voiceover (~13s):**

> "AWS KMS is supported at GA, with GCP, Azure, and HashiCorp Vault on the roadmap. The cryptographic boundary is real — CoreLink uses envelope encryption where the data-encryption key is wrapped by your customer-managed key. Our operators cannot read your bytes without your KMS authorising every unwrap."

### 5.3 — Configure AWS KMS (4:05 → 4:25)

**Action:** select "AWS KMS" → form shows: KMS key ARN, IAM role to assume, region. Paste a pre-built role ARN. Click "Test access". Green check appears within 2s.

**Voiceover (~18s):**

> "Paste your KMS key ARN and the IAM role CoreLink should assume. We verify access by performing a test encrypt-then-decrypt round-trip — your KMS records every call. The audit log on your side mirrors ours."

### 5.4 — Sensitive-op approval queue (4:25 → 4:40)

**Action:** click "Activate BYOK". A modal explains this is a sensitive operation that requires **two distinct approvers** per security control. URL: `/[locale]/admin/ops` → new pending op row visible.

**Voiceover (~13s):**

> "Activation is a sensitive operation. It enters a queue requiring two distinct admins to approve — not one admin clicking twice. The kill switch, key rotation, and emergency revoke all share this workflow. See our BYOK blog post or the deeper seven-minute BYOK demo for the rotation + kill-switch flow."

---

## Section 6 — Wrap-up + CTA (4:40 → 5:00, 20s)

**Visual:** end card.

```
corelink.humangr.com/quickstart      — 10-minute tutorial
corelink.humangr.com/trust           — TLA+ specs, SBOM, pentest
corelink.humangr.com/pricing         — sandbox is free
```

**Voiceover (~18s):**

> "That's the deep-dive. Ten-minute quickstart at corelink.humangr.com. The trust center has our TLA+ specs, full SBOM, and the third-party pentest letter. Sandbox is free, no credit card. If you're evaluating for a regulated workload, our seven-minute BYOK deep dive walks the four-provider matrix and the kill-switch flow. Thanks for watching."

**Hold final URL card for 2s. End.**

---

## Pre-flight checklist

- [ ] Two tenants pre-provisioned: `acme-build-cache` (sandbox) for §1–§4; `acme-prod` (enterprise tier) for §5.
- [ ] Pre-seeded blob `af1c3e9b…` in the sandbox tenant so timestamps look organic.
- [ ] AWS KMS sandbox key + assumable IAM role pre-built for §5.3 (do **not** hand-edit on stage).
- [ ] Admin UI dark/light mode locked to **light** for screen-recording contrast.
- [ ] Captions: generate via Whisper + manually review against the VO transcript in this doc — never trust raw ASR for technical terms like "BLAKE3", "BYOK", "Merkle".
- [ ] `corelink doctor` returns 8/8 on the demo laptop right before take-1.
- [ ] Asciinema cast pre-recorded and trimmed; live typing only for the Python snippet in §3.2 (humanises the recording).

## Cross-reference

- `60-SEC-ELEVATOR.md` — short cousin; same beats compressed.
- `cli-asciinema-script.sh` — executable command set for §2 + §3.1 + step-5 of asciinema steps.
- `admin-ui-screenshot-guide.md` — shots #01 (sign-in) through #15 (BYOK ops queue) cover the admin lane.
- `byok-deep-dive-demo.md` — seven-minute follow-up specifically for enterprise buyers.
- `competitive-comparison-demo.md` — three-minute side-by-side vs `bazel-remote`.
- `marketing/launch/BLOG-POSTS/01-introducing-corelink.md` — narrative source.
- `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md` — referenced verbatim in §5 VO ("BYOK blog post").
- `marketing/launch/BLOG-POSTS/03-audit-chain-merkle-proofs.md` — §4 VO ("transparency-log key").
- `apps/docs/docs/tutorials/quickstart-10min.mdx` — every command in §2 cross-checked against this.
- `crates/corelink-cli/src/main.rs` — every CLI subcommand cross-checked against the canonical surface.
- `marketing/launch/LAUNCH-CHECKLIST-V2.md` row T-7d ("demos recorded") — gating dependency.
