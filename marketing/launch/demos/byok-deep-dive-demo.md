---
id: "DEMO-BYOK-DEEP-DIVE"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "VPMkt"
final_approver: "Gustavo Schneiter"
reviewers: ["CTO", "VPSec", "Crypto SME"]
supersedes: null
superseded_by: null
parent: "marketing/launch/LAUNCH-CHECKLIST-V2.md §2 (T-7d demos recorded)"
tags:
  - "marketing"
  - "launch"
  - "demo"
  - "byok"
  - "enterprise"
  - "r8"
---

# 7-minute BYOK deep dive — enterprise buyer cut

> **Audience:** enterprise security buyer, CISO, compliance lead, or a sceptical principal engineer mapping CoreLink against their KMS posture.
> **Goal at end of 7 minutes:** the viewer has seen, end-to-end, the four-provider BYOK matrix, an actual key rotation, a fail-CLOSED kill-switch event mirrored in the audit chain, and the FIPS attestation surface — and is convinced the cryptographic boundary is real, not a checkbox.
> **Tone:** dense, plain, no superlatives. Buyers of this profile detect inflated claims in the first 30 seconds. Numbers and specifics over adjectives.
> **Cross-refs:** `5-MIN-DEEPDIVE.md` §5 (this doc is the deeper version of §5), `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md` (narrative source), `admin-ui-screenshot-guide.md` shots #10–#15.

---

## Time budget — exact

| Section | Duration | Cumulative | Lane |
|---|---|---|---|
| 0. Framing + threat model | 0:00 → 0:40 | 0:40 | Narrator + title card |
| 1. 4-provider matrix walkthrough | 0:40 → 1:50 | 1:50 | Admin UI |
| 2. Initial activation (dual approval) | 1:50 → 2:50 | 2:50 | Admin UI |
| 3. First encrypted put (envelope under hood) | 2:50 → 3:35 | 3:35 | Split: terminal + diagram |
| 4. Key rotation flow | 3:35 → 4:50 | 4:50 | Admin UI |
| 5. Kill switch (fail-CLOSED) | 4:50 → 5:50 | 5:50 | Admin UI + terminal |
| 6. Audit trail of rotation + kill | 5:50 → 6:30 | 6:30 | Admin UI |
| 7. FIPS attestation surface | 6:30 → 7:00 | 7:00 | Trust center |

Total = 7 minutes. Hard cap. Section 5 (kill switch) is non-negotiable — that's the buyer's "I get it" moment. If you have to compress, eject the diagram in §3 and the FIPS deep zoom in §7 first.

---

## Section 0 — Framing + threat model (0:00 → 0:40, 40s)

**Visual:** title card listing the three adversaries.

```
CoreLink BYOK — threat model
  1. Compromised vendor operator (insider attack)
  2. Coerced vendor (lawful order, malicious court)
  3. Customer-initiated erasure (regulator audit)
```

**Voiceover (~36s):**

> "BYOK is the most over-claimed term in the vendor security market. In its weakest form it means we hold an opaque token you sent us. In the strongest form it means *we cannot read your bytes — structurally — without your KMS authorising every operation*. CoreLink ships the strong form across four providers. In the next seven minutes we'll show the activation, a rotation, a kill switch, and the audit trail your auditor can replay. Three adversaries in the threat model: a compromised CoreLink operator, a coerced CoreLink operator, and a regulator-driven erasure event."

---

## Section 1 — 4-provider matrix walkthrough (0:40 → 1:50, 70s)

**Lane:** admin UI at `/en/admin/tenants/acme-prod` → "Encryption" tab.

**Screenshot reference:** Shot #10 (BYOK provider matrix).

**Visible:** four cards in a 2×2 grid:

| Provider | Status surface | Trust anchor |
|---|---|---|
| AWS KMS | KMS key ARN + IAM role | AWS account, FIPS 140-2 L3 on HSM-backed keys |
| GCP KMS | Resource name + workload identity | GCP project + Cloud HSM (FIPS 140-2 L3) |
| Azure Key Vault | Key URI + managed identity | Azure tenant + Premium HSM tier |
| HashiCorp Vault | Vault address + AppRole + transit path | Self-hosted; FIPS posture depends on customer build |

**Action sequence:**

1. Hover each card — tooltip surfaces the FIPS level claim (or the explicit "depends on customer build" warning for Vault).
2. Open the AWS KMS card. Form shows: KMS key ARN field, IAM role to assume, region pin (auto-locked to the tenant's residency region).
3. Briefly show GCP, Azure, Vault forms — same surface, different field names, same proof-of-access "Test access" button.

**Voiceover (~62s):**

> "Four providers. AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault. The first three FIPS-certified at FIPS 140-2 Level 3 on HSM-backed keys; the Vault posture depends on your build, and we surface that honestly in the tooltip. Region pinning is automatic — your KMS key is tied to the tenant's residency region, and CoreLink will refuse to attempt unwrap from a different region. The cryptographic primitive is envelope encryption: a per-blob data-encryption key is generated, that DEK encrypts the blob, then your KMS wraps the DEK. We never store an unwrapped DEK on disk. Our operators cannot read your bytes without your KMS authorising every single unwrap call — and every unwrap is logged on your side."

---

## Section 2 — Initial activation (dual approval) (1:50 → 2:50, 60s)

**Lane:** admin UI.

**Screenshot reference:** Shots #11 (configure form with green test-access), #12 (sensitive ops queue), #13 (op detail page).

**Action sequence:**

1. Fill the AWS KMS form with the demo ARN (`arn:aws:kms:us-west-2:111122223333:key/abcd-1234`) and IAM role (`arn:aws:iam::111122223333:role/CorelinkKMSAccess`).
2. Click "Test access". Green check appears within 2s — show this clearly. The check means CoreLink did an encrypt-then-decrypt round-trip against the customer's KMS.
3. Click "Activate BYOK". Modal warns: "This is a sensitive operation. Two distinct approvers required."
4. Click "Submit for approval". Page navigates to `/en/admin/ops` → new row visible in pending state (Shot #12).
5. Click the row → `/en/admin/ops/[op_id]` (Shot #13). The requestor (logged-in user) sees the "Approve" button greyed out with the hint "Requestor cannot self-approve".
6. Switch personas (cut to a second browser profile already signed in as a different admin). That admin sees the same op and approves. Status flips to "Approved (2/2) — activating".

**Voiceover (~55s):**

> "Submit the KMS config. CoreLink performs an encrypt-then-decrypt round-trip against your KMS — the green check means your KMS logged two calls, and CoreLink received bytes back. Activation is a sensitive operation, not a config edit — it enters the dual-approval queue. Two **distinct** admins. The requestor's approve button is greyed out, with a hint explaining why. This separation-of-duties is enforced server-side, not on the button alone — try to API-flip your own request and the server refuses. The same workflow gates BYOK rotation, kill switch arming, tenant data export, and account deletion."

---

## Section 3 — First encrypted put (envelope under hood) (2:50 → 3:35, 45s)

**Lane:** split. Left = terminal. Right = an envelope-encryption diagram (static SVG, animates in over 4s).

**Diagram (right pane):**

```
                ┌──────────────────────┐
                │  CoreLink data plane │
   put bytes -> │                      │
                │  1. gen DEK (random) │
                │  2. AES-256-GCM      │
                │     blob = E(DEK,b)  │
                │  3. wrap DEK         │
                │     via customer KMS │ ──KMS API──> ┌────────────────┐
                │  4. store {blob,     │              │ Customer KMS   │
                │     wrapped_DEK}     │              │ (audit logged) │
                └──────────────────────┘              └────────────────┘
```

**Terminal (left pane):**

```bash
$ corelink put build/release-v2.tar.gz
digest=9c4d2a1f…  size=18.2 MB  uploaded=ok  encryption=byok:aws-kms

$ corelink stat 9c4d2a1f…
digest=9c4d2a1f…  size=18.2 MB  exists=true  encryption=byok:aws-kms  key_fp=arn:…/abcd-1234
```

**Voiceover (~40s):**

> "First encrypted put. The CLI output now shows `encryption=byok:aws-kms` — the per-blob data key was wrapped by your KMS for this upload. On the right: how it works. CoreLink generates a fresh random DEK, encrypts the blob with AES-256-GCM, calls your KMS to wrap the DEK, and stores the ciphertext alongside the wrapped DEK. Every retrieval calls your KMS to unwrap. Your KMS audit log mirrors ours. If you revoke the IAM role, every subsequent unwrap fails — there is no out-of-band cached DEK on our side."

---

## Section 4 — Key rotation flow (3:35 → 4:50, 75s)

**Lane:** admin UI at `/en/admin/tenants/acme-prod` → "Encryption" tab.

**Screenshot reference:** Shot #14 (audit page filtered for `byok.rotate`).

**Action sequence:**

1. On the encryption tab, click "Rotate key". Modal explains: new KMS key ARN required; current blobs are NOT re-encrypted blob-by-blob — instead, the DEK wrapping is rotated lazily on access (a "lazy DEK re-wrap" policy).
2. Paste a new key ARN. Click "Test access". Green check.
3. Click "Submit rotation for approval" → again, sensitive op queue.
4. Switch personas; second admin approves.
5. Watch the encryption tab: status transitions through `pending → rotating → active`. A "Rotation progress" widget shows blobs touched today vs total — the lazy policy means full propagation takes one access-cycle, not a full re-encrypt.
6. Navigate to `/en/admin/audit?event_type=byok.rotate` (Shot #14). The event row shows old key fingerprint, new key fingerprint, both approvers, four-phase timestamps.

**Voiceover (~70s):**

> "Rotation. New KMS key ARN. Test access again — same encrypt-then-decrypt round-trip. Sensitive-op queue, dual approval, same workflow. The interesting design choice is *lazy re-wrap*: existing blobs are not bulk-re-encrypted on rotation. Instead, the DEK wrapping is rotated on next access. For a petabyte-scale cache that's the difference between hours and weeks of background work — and the security posture is identical because the old wrapping is invalidated the moment you click 'Disable old key' on your KMS side. We document the trade-off in our BYOK blog post — and we surface the rotation progress so you know exactly how many blobs are still under the old wrap at any time."

---

## Section 5 — Kill switch (fail-CLOSED) (4:50 → 5:50, 60s)

**Lane:** admin UI + terminal split.

**Action sequence:**

1. On the encryption tab, scroll to the "Kill switch" section. Status: armed. Toggle "Activate kill switch — fail-CLOSED on next request".
2. Modal: "This will refuse ALL CAS reads + writes against this tenant until disarmed. Two-approver workflow." Submit.
3. Second admin approves.
4. Cut to terminal. Run a `corelink get` against an existing digest under this tenant:

```bash
$ corelink get 9c4d2a1f… --output /tmp/restored.tgz
error: COR_BYOK_KILL_SWITCH_ACTIVE
       Tenant acme-prod has BYOK kill switch armed and active.
       Operator action required to disarm. See: /en/admin/tenants/acme-prod/encryption
       Audit event id: byok_kill_22a8f9e1
```

The CLI exits with code 1. **Show the exit code** (`echo $?` after the command — viewers in this audience want to see it).

5. Cut back to admin UI. The audit page now has the kill event at the top.

**Voiceover (~52s):**

> "Kill switch. Toggle armed. Dual approval. Once active, every CAS read and write against this tenant fails closed with `COR_BYOK_KILL_SWITCH_ACTIVE`. Not 'queued for later' — refused. Exit code one. This is the mechanism your incident response needs when you suspect a key compromise: one click, two approvers, full freeze, audited. We test this monthly per our runbook drills — the cast goes in the audit evidence pack with the operator's name and the actual recorded duration. There is no quiet bypass for CoreLink operations — we cannot disarm your kill switch ourselves. You disarm it."

**Reset step (off-camera):** disarm the kill switch via the same dual-approval flow so the rest of the demo can continue.

---

## Section 6 — Audit trail of rotation + kill (5:50 → 6:30, 40s)

**Lane:** admin UI at `/en/admin/audit`.

**Action sequence:**

1. Filter audit to event types `byok.*`. Show the chain:
   - `byok.activate` (from §2)
   - `byok.put` × N (from §3 and §4)
   - `byok.rotate.requested` → `byok.rotate.approved` → `byok.rotate.complete`
   - `byok.kill_switch.armed.requested` → `byok.kill_switch.armed.approved` → `byok.kill_switch.refusal` × N (one per failed CLI call)
   - `byok.kill_switch.disarmed.*`
2. Click the `byok.rotate.complete` event → detail page → show the Merkle inclusion proof.
3. Click "Export evidence (last 24h)" → JSON downloads. Open in editor briefly — show the signed daily-root hash at the top of the export.

**Voiceover (~36s):**

> "Every BYOK event is a leaf in the same Merkle audit tree as your CAS events. Activation, rotation, refusal, disarm — all of it. Export as JSON. The signed daily root is at the top of the export; your auditor verifies it against the public transparency log. They don't trust CoreLink — they verify CoreLink. That's the difference between a SOC 2 report and a cryptographic proof."

---

## Section 7 — FIPS attestation surface (6:30 → 7:00, 30s)

**Lane:** browser navigates to the public Trust Center (`corelink.humangr.com/trust`) **and** the tenant overview (`/en/admin/tenants/acme-prod`).

**Screenshot reference:** Shot #15 (tenant overview with FIPS badge + kill-switch toggle).

**Action sequence:**

1. On the tenant overview, point at the "Encryption: AWS KMS (FIPS 140-2 Level 3 attested)" badge.
2. Navigate to `corelink.humangr.com/trust` → scroll to "FIPS attestation" section → show the published attestation document with HSM serial numbers, KMS provider statements, and the timestamp of the most-recent customer-side attestation test.
3. End on the Trust Center page with the URL highlighted.

**Voiceover (~26s):**

> "FIPS 140-2 Level 3 attestation is surfaced both inside your tenant view and on our public Trust Center. We publish HSM serial numbers, KMS provider attestation statements, and the timestamp of the most recent attestation round-trip we ran from your KMS. If your auditor asks 'prove the HSM was real on the day my blob was encrypted', the answer is here. corelink.humangr.com/trust."

---

## Pre-flight checklist

- [ ] `acme-prod` tenant pre-provisioned with **fresh** KMS keys per provider (rotate the demo keys monthly so the screenshots stay honest).
- [ ] Two admin personas pre-set in browser profiles: `requestor@example.com` and `approver@example.com`.
- [ ] AWS KMS key ARN + assumable IAM role ready; same for GCP + Azure + Vault.
- [ ] Kill-switch disarm step rehearsed and pre-recorded as a safety net cast (in case it fails on stage).
- [ ] `corelink doctor` returns 8/8 for `acme-prod` (specifically: BYOK row green).
- [ ] Trust Center FIPS attestation page is current within 30 days.
- [ ] Audit-event filter URL `event_type=byok.*` returns rows in last 24h (re-run the demo once to seed events if needed).

## Cross-reference

- `5-MIN-DEEPDIVE.md` §5 — surface-level version of this demo.
- `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md` — narrative source; every claim in this demo traces to a section of that post.
- `admin-ui-screenshot-guide.md` shots #10, #11, #12, #13, #14, #15 — capture sources for stills.
- `cli-asciinema-script.sh` — base CLI cast (extend with the kill-switch refusal frame for the §5 terminal beat).
- `marketing/launch/LAUNCH-CHECKLIST-V2.md` row T-7d ("demos recorded") — gating dependency.
- `competitive-comparison-demo.md` — companion demo for buyers also comparing CoreLink to `bazel-remote`.
