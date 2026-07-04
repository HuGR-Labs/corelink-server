---
type: "Runbook"
title: "Release / GA tag process"
description: "How the v1.0.0-GA tag is cut: the dual-key 2-signer sign-off (ADR-0034b fallback), the spec-corpus + production-wiring + compliance freeze state recorded in the tag, and the acknowledged open carve-outs tracked for post-GA closure."
source_files:
  - "docs/release/v1.0.0-GA-tag-draft-final.txt"
  - "crates/corelink-container/src/routes/audit_drain.rs"
  - "crates/corelink-container/src/main.rs"
checkpoint_sha: "488155ba24dabea3d17a2cc0c02c7c9880b433ad"
provenance: "AUTHORED"
tags: ["ops", "release", "ga", "sign-off", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# Release / GA tag process

The GA tag is not a `git tag -m`: it is a governed cutover artifact that records, in the annotated tag
body itself, the exact frozen state of the product at release — the sealed spec corpus, the production
wiring, the attested compliance frameworks, and the open carve-outs — and is applied only after a
**two-key sign-off** (Owner + on-call SRE Lead, or the ADR-0034b dual-hat fallback when the second role is
unstaffed). The draft tag carries `__OWNER_SHA__` / `__SREL_SHA__` placeholders that the Owner replaces
with real DocuSign envelope IDs or signed-commit SHAs at cutover, so the tag is its own provenance record.
This is the release counterpart to the [reproducible-build process](/ops/reproducible-build.md) that
proves the artifact, and to the GA staffing waiver in
[ADR-0034](/adr/adr-0034-prr-staffing-waiver-solo-tier.md).

# Role
- The release gate: defines what MUST be green and signed before v1.0.0 ships.
- The freeze record: the tag body is the canonical snapshot of corpus + wiring + compliance at GA freeze.
- The accountability artifact: a 2-key signature with a documented dual-hat fallback, not a solo push.

# How it works
1. The tag is applied at wave-27 via `scripts/cut-v1-0-0-ga-tag.sh`, only after both 2-key signatures are
   filed per RB-GA-CUTOVER §8 + ADR-0034b (`docs/release/v1.0.0-GA-tag-draft-final.txt:7-11`).
2. The corpus freeze is recorded: 21 sprints sealed (S01..S21), 197 registry invariants (81+
   TLA+-verified), 262 spec docs, with `validate_specs.py` + `validate_references.py` at exit 0
   (`docs/release/v1.0.0-GA-tag-draft-final.txt:13-24`).
3. The production wiring is enumerated: 4-target Cloudflare binding (D1 ×5 regions, R2 25 buckets, KV,
   Durable Objects) + BYOK 4 providers + the hash-chained audit chain
   (`docs/release/v1.0.0-GA-tag-draft-final.txt:26-44`). ⚠️ **Designed ≠ wired** — this tag section is the
   GA *target* posture, not a uniform statement of live wiring. Three tiers coexist here:
   **(a) wired-and-live** — the CF 4-target binding (D1 / R2 / KV / Durable Objects);
   **(b) wired-but-dormant-until-keyed** — the hash-chained **audit-chain producer/seal** (the S-09
   drain): the `POST /_internal/audit/drain` route is mounted and actually seals the live `audit_outbox`
   into the BLAKE3 tamper-evident chain — now with an Ed25519-SIGNED chain head (CF-6, migration 0080) —
   when the erase/internal auth key + D1 are bound
   (`crates/corelink-container/src/main.rs:690`, now mounted directly alongside the
   `/_internal/dsr/{erase,verify,access,portability,rectification}` family that the DSAR-completion work
   expanded; seal logic at
   `crates/corelink-container/src/routes/audit_drain.rs:394`/`:738`/`:851`), and stays mounted-inert
   (fail-CLOSED unmount) when the key+D1 are absent — it is NOT a designed-only skeleton (this matches the
   audit-chain concept's WIRED treatment, [compliance/audit-chain.md](/compliance/audit-chain.md));
   **(c) DESIGNED-but-UNWIRED skeletons** enumerated as the target GA posture, not running GA wiring —
   the BYOK-4-providers orchestrator, the Neon shadow-sync, WebAuthn admin-enrollment, and the customer
   **R2 audit-EXPORT exporter + its cron** (the seal is wired per (b), but exporting the sealed chain to the
   customer's R2 + the scheduled drain cron remain deferred) (see the BYOK envelope-encryption concept
   [storage/byok-envelope-encryption.md](/storage/byok-envelope-encryption.md), the audit chain
   [compliance/audit-chain.md](/compliance/audit-chain.md), and the erasure-attestation persist/serve
   deferral noted in [ADR-S14-007](/adr/adr-s14-007-erasure-attestation-ed25519-jcs.md)).
4. The quality bar is attested: adversarial-review mean 9.41/10 across waves 21-25, chaos + 24h endurance
   + perf-regression harnesses green (`docs/release/v1.0.0-GA-tag-draft-final.txt:46-64`).
5. The compliance posture is frozen into the tag: SOC 2 Type 2, ISO 27001:2022, GDPR, LGPD, LFPDPPP, PCI
   DSS v4.0 (SAQ A-EP), CCPA/CPRA (`docs/release/v1.0.0-GA-tag-draft-final.txt:66-80`).
6. Open carve-outs are acknowledged in the tag, not hidden: DEBT-003 SOC2 interim path, DEBT-026 external
   pentest post-GA, and the unstaffed FW-H reviewer role (`docs/release/v1.0.0-GA-tag-draft-final.txt:82-93`).
7. Two signers sign: Signer 1 = Owner / final GA Go authority, Signer 2 = on-call SRE Lead, each via
   DocuSign, with the dual-hat fallback when a role is unstaffed
   (`docs/release/v1.0.0-GA-tag-draft-final.txt:107-132`).
8. The Owner fills the `__OWNER_SHA__` / `__SREL_SHA__` + timestamp placeholders at D-day before running
   the cut script (`docs/release/v1.0.0-GA-tag-draft-final.txt:9-11`).

# Invariants
- The tag is applied ONLY after BOTH 2-key signatures are filed — a single-key cut is not a valid GA
  release (`docs/release/v1.0.0-GA-tag-draft-final.txt:7-9`).
- The spec gates (`validate_specs.py` + `validate_references.py`) MUST be at exit 0 at the base SHA before
  freeze (`docs/release/v1.0.0-GA-tag-draft-final.txt:23-24`).
- Open carve-outs MUST be acknowledged in the tag with a tracked closure target, never silently dropped —
  GA ships with known, dated debt, not hidden debt (`docs/release/v1.0.0-GA-tag-draft-final.txt:82-93`).
- The release carries a DCO `Signed-off-by` + the Co-Authored-By provenance trailer
  (`docs/release/v1.0.0-GA-tag-draft-final.txt:134-138`).

# Gotchas
- The 24h endurance run is GREEN only at a 10-min dress-run at freeze time; the full 24h slot is scheduled
  for T-7d ±2h per RB-GA-CUTOVER §2.3 — "endurance green" at freeze means the dress-run, not the full slot
  (`docs/release/v1.0.0-GA-tag-draft-final.txt:57-59`).
- The cutover is explicitly NOT a regulatory breach event (LGPD/ANPD note) — do not trip the 72h
  breach-notification procedure on the deploy itself (`docs/release/v1.0.0-GA-tag-draft-final.txt:73-75`).
- The external penetration test (DEBT-026) is scope-frozen but its field work begins POST-GA — GA ships
  before that pentest's findings land, by design (`docs/release/v1.0.0-GA-tag-draft-final.txt:88-90`).

# Citations
1. `docs/release/v1.0.0-GA-tag-draft-final.txt:7-11` — cut script + 2-key sign-off + placeholder fill.
2. `docs/release/v1.0.0-GA-tag-draft-final.txt:13-24` — spec-corpus freeze state + validator exit 0.
3. `docs/release/v1.0.0-GA-tag-draft-final.txt:26-44` — production wiring (CF 4-target, BYOK, audit chain).
4. `docs/release/v1.0.0-GA-tag-draft-final.txt:46-64` — quality bar (adversarial 9.41, chaos, endurance).
5. `docs/release/v1.0.0-GA-tag-draft-final.txt:57-59` — 24h endurance dress-run vs scheduled full slot.
6. `docs/release/v1.0.0-GA-tag-draft-final.txt:66-80` — attested compliance frameworks.
7. `docs/release/v1.0.0-GA-tag-draft-final.txt:73-75` — cutover is not a breach event (LGPD note).
8. `docs/release/v1.0.0-GA-tag-draft-final.txt:82-93` — acknowledged open carve-outs (DEBT-003/026, FW-H).
9. `docs/release/v1.0.0-GA-tag-draft-final.txt:88-90` — DEBT-026 external pentest begins post-GA.
10. `docs/release/v1.0.0-GA-tag-draft-final.txt:107-132` — the two signers + ADR-0034b dual-hat fallback.
11. `docs/release/v1.0.0-GA-tag-draft-final.txt:134-138` — DCO + Co-Authored-By provenance trailer.
