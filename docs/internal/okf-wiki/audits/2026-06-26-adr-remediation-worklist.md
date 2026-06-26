---
title: "OKF Wiki ADR-wave (wave-3) — Remediation Worklist"
type: "Audit"
description: "Exact transcribe-only fixes for the 14 MINOR findings from the 76-ADR adversarial verification. 0 BLOCKER, 0 MAJOR."
status: "CLOSED (2026-06-26) — all 14 applied + lead-spot-checked; 76 ADRs TRUTH-CERTIFIED; validator green at 114."
tags: ["okf", "audit", "remediation", "adr"]
---

# ADR-wave remediation worklist (apply each EXACTLY; transcribe-only)

All targets are `docs/knowledge/adr/*.md`. Most fixes WIDEN or ADD a `path:line` cite so a claim that
folds in text from the ADR's "Alternatives rejected"/"Rationale"/"Addendum" sub-section is grounded on
that sub-section. Before writing any new cite, OPEN the ADR file at that line and confirm it shows the
claim. Do NOT change code. Keep `checkpoint_sha` unchanged. After all fixes: `python3 scripts/okf_index.py`
then `python3 scripts/validate_okf.py` → must stay GREEN.

## Framing fixes (reword — 3)
- adr-0024-dependency-track-self-host.md: change "that capability **is met by** a self-hosted OWASP Dependency-Track instance…" → "the **decision is to meet** it via a self-hosted OWASP Dependency-Track instance…" (DT is decided, not yet deployed — ADR-0014 tracks DT infra as not-deployed). Keep the existing cite.
- adr-0025-deploy-gate-hard-cosign-keyless.md: add a parenthetical "(a DRAFT, ratified at WI-S12-003 SEAL)" to the lead/decision sentence, for parity with sibling DRAFT concepts. Cite `:26`.
- adr-s20-rsa-marvin-mitigation.md: change "Four mitigations **are in force**: …" → "Four mitigations are **specified to take force on the ADR's ACTIVE date** (shipping as follow-on WIs in R-2 W2): …". Keep cite `:127-141`.

## Citation widen/add fixes (11)
- adr-0013-promote-remote-cache-canonical.md: repoint citation #1 `:30` → `:36` (the "de facto mas não de jure" conclusion; still within Context).
- adr-0042-gc-worker-scheduler.md: (a) ADD a citation to `:87-95` for the "Addendum §A1 pins tla2tools.jar v1.8.0 + SHA-256" claim; (b) EXTEND the rejected-alternatives cite from `:41-51` to also cover `:53-59`.
- adr-0065-event-log-do-thin-append-only-primitive.md: add cite `:62-63` for "a D1-table log loses the total-order guarantee under concurrency" (Alternatives-rejected).
- adr-0066-transparency-log-integrate-rekor.md: repoint/extend the "rejected alternatives (CoreLink-operated log / deferring)" cite from `:56-61` to `:48-54`.
- adr-0067-secrets-broker-d1-encrypted-lease-deferred.md: add cite `:55` for "Plaintext-in-D1 is rejected" (Alternatives-rejected).
- adr-0068-per-tenant-monthly-dollar-ceiling.md: add cite `:55` for the "post-hoc billing alerts (fail-open) only detect overspend after it happens" clause (keep the existing `:24-30` for the rate-vs-cost claim).
- adr-0069-pat-verification-fast-hash-vs-argon2id.md: add cite `:79-81` for "a rejected stop-gap was raising the verify-cache TTL".
- adr-s11-005-consent-symmetric-grant-revoke-schema.md: drop "and no fail-open swap" from citation #2's label (it is already grounded by citation #3 `:68-79`), OR repoint that clause to `:78-79`.
- adr-s11-008-sub-processor-default-subscribed-tier-team-plus.md: add cite `:68-74` for the "v1.0 had conflated marketing_email with sub_processor_notifications" correction.
- adr-s11-011-region-migration-cooldown-30d.md: add cite `:54-60` for the "7d/60d rejected, 14d considered" Alternatives-Considered claim.
- adr-s14-008-dpa-amendment-schrems-ii-tia-legal-externo.md: extend the D6 cite to `:117-124` (the WAIVER-S14-001 trigger / 90-day expiry / D+60 hard deadline).

## Upstream ADR defects (track separately — NOT fixed in this docs PR)
- `corelink-bazel-bridge/src/lib.rs:49` typo `INV-BAZEL-NO-GROPC` (code).
- ADR H1 mismatches: ADR-S13-002 H1 says "S13-001"; ADR-S14-004 H1 "S14-001"; ADR-S14-008 H1 "S14-007"; ADR-S14-006 closes "ADR-S14-001"; ADR-S12-046 H1 "7" but lists 12.
- ADR-0037 carries leftover v1.0.0 `BLAKE3(merkle_root)` text at src lines 99/106/115 (concept correctly adopts the v1.1.0 reading).
