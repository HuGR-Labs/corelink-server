# CoreLink GA — Security Readiness Index

> **Owner:** Security Lead `(a nomear)` (co-owned with Orchestrator until appointed).
> **Audience:** orchestrator, Security Lead, Legal Counsel, GA gatekeeper, external pentest vendor.
> **Status:** ACTIVE for GA gate (S-20). This is the landing page for all pre-GA security documents.

This page is the canonical entry point to the CoreLink GA security readiness corpus. It indexes the contractual SOW, the day-1 evidence pack, the consolidated pentest scope brief, the operational engagement checklist, and the per-sprint internal pentest reports.

---

## Pre-GA external pentest pack

| Doc | Purpose | Canonical path |
|---|---|---|
| Consolidated scope brief | Executive scope, attacker model, ASVS matrix, attack chains, handover artifacts | [`specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md`](../../specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md) |
| Statement of work | Contractual SOW (countersigned by vendor + Legal) | [`specs/_audits/sealed/pentest/SOW-S20-EXTERNAL-PENTEST.md`](../../specs/_audits/sealed/pentest/SOW-S20-EXTERNAL-PENTEST.md) |
| Evidence package | Day-1 deliverable for vendor kickoff | [`specs/_audits/sealed/pentest/PENTEST-EVIDENCE-PACKAGE.md`](../../specs/_audits/sealed/pentest/PENTEST-EVIDENCE-PACKAGE.md) |
| Vendor onboarding | Playbook for vendor onboarding (T-28 → T-0) | [`specs/_audits/sealed/pentest/VENDOR-ONBOARDING.md`](../../specs/_audits/sealed/pentest/VENDOR-ONBOARDING.md) |
| Access provisioning | Credentials + VPN + endpoint catalogue | [`specs/_audits/sealed/pentest/access-provisioning.md`](../../specs/_audits/sealed/pentest/access-provisioning.md) |
| Findings template | Machine-readable finding format | [`specs/_pentest/findings-template.md`](../../specs/_pentest/findings-template.md) |
| Vendor shortlist | Schellman / A-LIGN / Bishop Fox candidates | [`specs/_audits/sealed/pentest/vendor-shortlist.md`](../../specs/_audits/sealed/pentest/vendor-shortlist.md) |
| Engagement checklist | Operational checklist for the 7-week engagement | [`pentest-engagement-checklist.md`](pentest-engagement-checklist.md) |

---

## Internal pentest corpus (S-01 .. S-20)

See [`specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md`](../../specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md) §5 for the full coverage matrix mapping each report to its §2 in-scope asset and the §7 attack chain(s) the external vendor must re-validate.

Per-sprint internal pentests (8):

- `specs/_audits/sealed/2026-04-30-pentest-s02-internal.md` — CAS plane
- `specs/_audits/sealed/2026-05-01-pentest-s03-internal.md` — Auth real (Clerk / PAT / WebAuthn / audit chain)
- `specs/_audits/sealed/2026-05-01-pentest-s04-internal.md` — Multipart upload
- `specs/_audits/sealed/2026-05-01-pentest-s05-internal.md` — Action Cache
- `specs/_audits/sealed/2026-05-02-pentest-s06-internal.md` — REAPI gRPC
- `specs/_audits/sealed/2026-05-14-pentest-s14-byok.md` — BYOK envelope (4 KMS providers)
- `specs/_audits/sealed/2026-05-14-security-walkthrough-s12.md` — Supply chain
- `specs/_audits/sealed/2026-05-14-security-walkthrough-s13.md` — Admin plane

Adversarial summaries (16) and cross-cutting hardening audits (7) are enumerated in scope doc §5.

---

## Security model + privacy model canonical sources

| Doc | Purpose |
|---|---|
| [`specs/03_architecture/security_model.md`](../../specs/03_architecture/security_model.md) | CTRL registry canonical source |
| [`specs/03_architecture/privacy_model.md`](../../specs/03_architecture/privacy_model.md) | Privacy CTRLs canonical source |
| [`specs/03_architecture/key_management.md`](../../specs/03_architecture/key_management.md) | BYOK canonical source |
| [`specs/03_architecture/invariant_registry.md`](../../specs/03_architecture/invariant_registry.md) | INV-* canonical definitions (136+ INVs) |
| [`specs/03_architecture/compliance_matrix.md`](../../specs/03_architecture/compliance_matrix.md) | LGPD / GDPR / SOC 2 mapping |
| [`specs/_security/vulnerability-disclosure-policy.md`](../../specs/_security/vulnerability-disclosure-policy.md) | Coordinated disclosure |

---

## Operational security runbooks

- [`docs/internal/secrets-checklist.md`](secrets-checklist.md) — single source of truth for production secrets
- [`docs/internal/secrets-runbook.md`](secrets-runbook.md) — secret rotation procedures
- [`docs/internal/side-channel-defense.md`](side-channel-defense.md) — constant-time + timing-oracle defense posture
- [`docs/internal/slsa-l3-pipeline.md`](slsa-l3-pipeline.md) — SLSA L3 build-pipeline integrity
- [`docs/internal/dep-policy.md`](dep-policy.md) — dependency policy (cargo-audit / cargo-deny / Dependabot)
- [`docs/internal/dt-dr-runbook.md`](dt-dr-runbook.md) — disaster recovery
- [`docs/internal/auth-event-taxonomy.md`](auth-event-taxonomy.md) — EVT-* registry for auth events
- [`docs/internal/forensics-guide.md`](FORENSICS-GUIDE.md) — incident forensics
- [`specs/_runbooks/`](../../specs/_runbooks/) + [`specs/05_runbooks/`](../../specs/05_runbooks/) — full runbook corpus

---

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-16 | Gustavo Schneiter (via Claude Opus 4.7, wave-19 R-prep stream) | Initial security readiness index landing page authored alongside the consolidated pentest scope brief. Indexes the contractual SOW, evidence pack, vendor onboarding pack, engagement checklist, internal pentest corpus, security/privacy canonical sources, and operational runbooks. |
