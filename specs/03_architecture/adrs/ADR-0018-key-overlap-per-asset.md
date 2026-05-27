---
id: "ADR-0018"
type: "adr"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "key-management", "rotation", "overlap"]
---

# ADR-0018 — INV-KEY-OVERLAP é per asset class, não global 24h

## Context

Round 2 audit (Opus C-01) detectou que `INV-KEY-OVERLAP` tinha **3 valores incompatíveis** no repo:

- `key_management.md §3.2` definia o invariante como "até 24h overlap" (single value).
- `S-13` declara `TDK overlap 7d` em DoD + 14.s13.4, alegando seguir `INV-KEY-OVERLAP`.
- `S-14` declara `Ed25519 attestation key rotation overlap 30d` (R-S14-7).

Resultado: o invariante canônico é violado pelos sprints que dizem segui-lo. Property test não tem oracle. Auditor SOC 2 Type I (S-20 prep) pega isso na primeira passada — NIST SP 800-57 Pt 1 Rev 5 §5.3 exige rotation policy clear.

## Decision

**`INV-KEY-OVERLAP` é per asset class**, não global single-value. A canonical reference é a tabela em `key_management.md §3.2.1`:

| Asset class | Overlap target | Justificativa |
|---|---|---|
| PAT signing key | 24h | Curto blast radius; tokens rotated rápido |
| Audit chain key (per-region) | 24h | Hash chain integrity precisa rotation rápida |
| Admin signing key (HMAC para dual-approval) | 24h | Curto blast radius (igual ao PAT signing); admin op é dual-approver-bound; rotation frequente reduz surface de signature replay |
| TDK (tenant derivation key) | 7d | Re-wrap envelope CAS é background TB-scale; 24h causa starvation |
| BYOK customer CMK | 7d | Customer trigger; CoreLink-side DEK cache TTL window |
| Ed25519 attestation key | 30d | Long-lived signing; attestations 7y retention |

**Hard upper bound**: 30d. Excede sem ADR explícito + Security lead sign-off.

## Rationale

- **PAT/Audit/Admin signing (24h)**: blast radius pequeno; signature inline; speed de rotation prioridade.
- **TDK/BYOK (7d)**: re-wrap operations em scale (TB-EB) demandam janela operacional realista; 24h em workload típico (1k tenants × 50TB cada) é fisicamente infactível pra completar background re-wrap sem starvation de live traffic.
- **Ed25519 attestation (30d)**: long-lived signing key (não wrapping); attestations geradas hoje precisam ser verifiable 7y depois; rotation muito rápida invalida verifiability. NIST SP 800-57 Pt 1 Rev 5 Table 4 categoriza signing keys com cryptoperiod 1-3y; 30d overlap é fração disso.

## Consequences

**Positive:**
- Sprint contracts S-13/S-14 ficam coerentes com `INV-KEY-OVERLAP`.
- Property test tem oracle claro per asset class.
- Audit/SOC 2 Type I tem documentação consistente.
- Ad-hoc inflation de overlap fica gated por ADR + Security review.

**Negative:**
- Property test mais complexo (5 asset classes × test scenarios).
- Documentation overhead per nova asset class futuro.

## Alternatives considered

- **Single global 24h**: rejected — incompatível com TB-scale re-wrap operacional + Ed25519 long-lived signing semantics.
- **Single global 7d**: rejected — relax indevido para PAT/audit que não precisam.
- **Per-tenant overlap configurável**: rejected — operational complexity sem benefit; uniformity per asset class é simpler.
- **30d global hard upper**: rejected — overkill para PAT/audit; defensável apenas se justified per ASSET.

## References

- NIST SP 800-57 Pt 1 Rev 5 §5.3 (rotation policy).
- NIST SP 800-57 Pt 1 Rev 5 Table 4 (cryptoperiods).
- `specs/03_architecture/key_management.md §3.2.1` (canonical table).
- `specs/03_architecture/invariant_registry.md §3.13` (registry entry).
- Opus Round 2 audit C-01 finding (`specs/_audits/sealed/2026-04-24-opus-independent-sota-review-r2.md:40-50`).
