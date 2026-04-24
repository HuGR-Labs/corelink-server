---
id: "COMPLIANCE-MATRIX"
type: "compliance_matrix"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-04-23"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from: ["SECURITY-MODEL", "PRIVACY-MODEL", "OBSERVABILITY-MODEL", "AUTH-MODEL", "KEY-MANAGEMENT"]
tags: ["architecture", "compliance", "soc2", "iso27001", "lgpd", "gdpr"]
---

# Compliance Matrix — SOC 2, ISO 27001, LGPD, GDPR

> **doc_status:** DRAFT
> **Versão:** 0.1.0
> **Última atualização:** 2026-04-23
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Revisores:** ⚠️ **staffing-blocked** — promoção a `doc_status: FROZEN` bloqueada até ≥ 2 reviewers nomeados conforme roles indicados (endereça F-09 audit Lote 3+4)
> **Supersedes:** —
> **Superseded By:** —

> **Propósito:** fonte canônica (Nível 3) do mapeamento entre frameworks regulatórios/de auditoria e os controles internos do CoreLink (`CTRL-XXX` em `security_model.md` + `CTRL-PRIV-XXX` em `privacy_model.md`). Consumido por:
> - `specs/_templates/production_readiness_review.md §11` (verificação de controles em produção)
> - `specs/_templates/sprint_contract.md §16` (delta de compliance do sprint)
>
> Regra: todo novo control interno **DEVE** ter pelo menos 1 mapping para framework externo OU justificativa em ADR do por quê não aplica.

---

## Sumário

1. [Estado atual e roadmap de certificações](#1-estado-atual-e-roadmap-de-certificações)
2. [SOC 2 Type II — Trust Services Criteria](#2-soc-2-type-ii--trust-services-criteria)
3. [ISO/IEC 27001:2022](#3-isoiec-270012022)
4. [LGPD (Brasil)](#4-lgpd-brasil)
5. [GDPR (UE)](#5-gdpr-ue)
6. [Outras regulamentações relevantes](#6-outras-regulamentações-relevantes)
7. [Sub-processor compliance posture](#7-sub-processor-compliance-posture)
8. [Evidence collection strategy](#8-evidence-collection-strategy)
9. [Gap analysis](#9-gap-analysis)
10. [Referências](#10-referências)

---

## 1. Estado atual e roadmap de certificações

| Framework              | Escopo                                | Estado (hoje)           | Target                                       |
|------------------------|----------------------------------------|-------------------------|-----------------------------------------------|
| SOC 2 Type I           | Security + Availability + Confidentiality | Planejado              | 6 meses após GA (audit gap analysis + 3m readiness) |
| SOC 2 Type II          | Acima + Privacy + Processing Integrity | Planejado              | 12–18 meses após Type I (6m observation)      |
| ISO/IEC 27001:2022      | ISMS — escopo CoreLink CAS/AC          | Planejado              | 18 meses após Type II                         |
| LGPD                   | Brasil (operações sam region)          | **Compliance by design** | Fase 1 (GA)                                   |
| GDPR                   | UE (operações weur region)             | **Compliance by design** | Fase 1 (GA)                                   |
| HIPAA                  | Saúde US (opcional, plano enterprise)  | Conditional              | Sob demanda cliente (BAA + audit)             |
| PCI-DSS                | **Explicitamente fora de escopo**       | N/A                      | Não processamos cartão (Stripe)              |
| FedRAMP                | US gov                                  | Sob demanda              | 36+ meses se demanda                           |

> **Princípio:** construir os controles **agora**, certificar quando houver demanda. Controles sem certificação ainda valem (clientes pedem SIG ou CAIQ para diligence).

---

## 2. SOC 2 Type II — Trust Services Criteria

### 2.1 Escopo (TSC 2017, rev. 2022)

Planejamento: **Security + Availability + Confidentiality + Privacy + Processing Integrity**.

### 2.2 Mapping — Common Criteria (CC)

| TSC Criterion | Requisito resumido                                | CTRLs internos                                | Evidence                              |
|---------------|---------------------------------------------------|-----------------------------------------------|----------------------------------------|
| CC1.1         | Demonstrar comprometimento com integridade/ética   | Code of Conduct, HR onboarding                | EVT-032                        |
| CC1.2..1.5    | Governança, autoridade, competência                | Org chart, RACI, training                     | EVT-033                    |
| CC2.1..2.3    | Informação + comunicação (policies, ethics)         | Public policies; incident disclosure          | EVT-013 (status)        |
| CC3.1..3.4    | Risk assessment                                     | `failure_modes.md`; annual risk review         | EVT-034                         |
| CC4.1..4.2    | Monitoring activities                                | Observability stack (§observability_model)     | EVT-013                 |
| CC5.1..5.3    | Control activities                                  | All CTRL-* catalogs                           | CTRL-AUDIT-001..005                     |
| CC6.1         | Logical access / authentication                     | CTRL-AUTH-001, -004, -007, -010               | EVT-025                    |
| CC6.2         | Authorization                                        | CTRL-AUTHZ-001, -002                          | EVT-022                   |
| CC6.3         | Access provisioning                                  | `auth_model.md §5`; quarterly review           | EVT-035                     |
| CC6.6         | Logical/physical boundaries                           | Trust boundaries §3 security_model            | EVT-036                   |
| CC6.7         | Transmission / in-flight                             | CTRL-CRYPTO-001                                | EVT-037 (SSL Labs)            |
| CC6.8         | Malicious code / unauthorized software               | CTRL-SUPPLY-001..005                           | EVT-011                   |
| CC7.1..7.5    | Detection + incident response                        | Observability + runbooks + incident template   | EVT-019               |
| CC8.1         | Change management                                    | PR review, progressive rollout, dual-approval  | EVT-001 + EVT-038           |
| CC9.1         | Risk mitigation                                      | Compensating controls via `waiver.md`          | EVT-039                     |
| CC9.2         | Vendor management                                    | `§7 sub-processors`                            | EVT-040                     |

### 2.3 Mapping — Availability (A series)

| TSC Criterion | Requisito resumido | CTRLs internos | Evidence |
|---------------|---------------------|----------------|----------|
| A1.1         | Identify/monitor availability                        | SLOs `slo_catalog.md` + alerts                | EVT-013                |
| A1.2         | Recovery + contingency                               | Runbooks + chaos tests                         | EVT-023 + EVT-017 |
| A1.3         | Recovery infrastructure testing                      | DR drill semestral                             | EVT-041                          |

### 2.4 Mapping — Confidentiality (C series)

| TSC Criterion | Requisito resumido | CTRLs internos | Evidence |
|---------------|---------------------|----------------|----------|
| C1.1         | Identify + classify confidential                      | `privacy_model.md §2`                          | EVT-043           |
| C1.2         | Protection controls                                   | CTRL-CRYPTO-002, CTRL-ISO-001..005             | EVT-005                         |

### 2.5 Mapping — Processing Integrity (PI series)

| TSC Criterion | Requisito resumido | CTRLs internos | Evidence |
|---------------|---------------------|----------------|----------|
| PI1.1..1.5   | Inputs/processing/outputs correctness                 | CTRL-CAS-001, -002; CTRL-AC-001, -002; input validation CTRL-INPUT-001..004; reconciliation PAT-RECONCILE-001 | EVT-022 + EVT-002 |

### 2.6 Mapping — Privacy (P series)

| TSC Criterion | Requisito resumido | CTRLs internos | Evidence |
|---------------|---------------------|----------------|----------|
| P1.1         | Privacy notice                                        | `/privacy` page; versioned                     | EVT-044                       |
| P2.1         | Consent                                               | CTRL-PRIV-CONSENT-001..004 (privacy_model §5.6) | EVT-049 (consent events) + EVT-046 (LIA quando aplicável) |
| P3.1..3.2    | Collection limited to purpose                         | CTRL-PRIV-003                                  | EVT-026                 |
| P4.1..4.3    | Use, retention, disposal                              | `privacy_model.md §8`                          | EVT-042                      |
| P5.1..5.2    | Access + correction                                   | CTRL-PRIV-022 DSR self-service                 | EVT-048 (DSR_EVIDENCE)          |
| P6.1..6.7    | Disclosure + notification                             | DPA + breach runbook                           | EVT-017                |
| P7.1         | Data quality                                          | Reconciliation                                 | EVT-002              |
| P8.1         | Monitoring + enforcement                              | Privacy Officer role + quarterly review        | EVT-034                        |

---

## 3. ISO/IEC 27001:2022

### 3.1 Escopo

Statement of Applicability (SoA) cobre os 93 controles Anexo A:2022. Mapping compacto (full SoA em `_audits/iso27001-soa.csv`).

### 3.2 Temas críticos (seleção)

| Anexo A (2022) | Tema                                      | CTRLs internos                          | Notas                                 |
|----------------|-------------------------------------------|-----------------------------------------|----------------------------------------|
| A.5 Org. controls | Policies, roles, confidentiality, threat intel | Policies públicas; Security Lead    | Aplicável 100%                         |
| A.6 People     | Screening, training, NDA, sanctions         | HR process                              | Aplicável                              |
| A.7 Physical   | Site access, equipment                      | Cloudflare data centers (herda)          | Shared responsibility — evidence do sub-processor |
| A.8 Technological | Authentication, access, crypto, vuln mgmt, backup, logging, config mgmt, testing, software dev | CTRL-AUTH-*, CTRL-CRYPTO-*, CTRL-AUDIT-*, CTRL-SUPPLY-* | Core deste doc      |

### 3.3 Controles específicos relevantes

| Controle Anexo A | Implementação CoreLink | Evidence |
|------------------|------------------------|----------|
| A.8.2 (Privileged access) | Just-in-time admin; MFA mandatório | CTRL-AUTH-010 |
| A.8.9 (Config mgmt)       | Terraform + drift detection        | PAT-DRIFT-DETECTION-001 |
| A.8.24 (Cryptography)     | Algoritmos e key mgmt              | §7 security_model |
| A.8.25 (Secure dev)       | Code review + SAST                 | CTRL-INPUT-002, CTRL-SUPPLY-005 |
| A.8.28 (Secure coding)    | Rust + lints + fuzz                | EVT-008   |
| A.8.29 (Security testing) | Pentest anual                       | EVT-025 |
| A.8.30 (Outsourced dev)   | N/A (in-house)                      | — |

---

## 4. LGPD (Brasil)

### 4.1 Bases principais

| Art.   | Requisito                                             | Como atendemos                            | Evidence                         |
|--------|-------------------------------------------------------|--------------------------------------------|----------------------------------|
| Art. 6 | Finalidade, adequação, necessidade                    | CTRL-PRIV-003 (purpose_tag)                | EVT-026            |
| Art. 7 | Base legal                                             | `privacy_model.md §9`                      | EVT-044                 |
| Art. 11 | Dado sensível — não coletamos default                | §2 privacy_model                            | EVT-026            |
| Art. 15 | Término do tratamento                                  | §8 privacy_model (retention)                | EVT-042                 |
| Art. 17 | Titular tem direito                                    | §6 privacy_model (DSRs)                     | EVT-048 (DSR_EVIDENCE)     |
| Art. 18 I–IX | Direitos específicos                              | §6 privacy_model                            | EVT-048 (DSR_EVIDENCE)     |
| Art. 33 | Transferência internacional                           | §7 privacy_model (SCC, residency)          | EVT-044                 |
| Art. 37 | Registro de operações                                  | Audit events CloudEvents                    | EVT-047 (AUDIT_EVENT)      |
| Art. 38 | Relatório de impacto à proteção de dados (RIPD)       | DPIA por WI HIGH_RISK                       | EVT-045                         |
| Art. 41 | Encarregado (DPO)                                      | Privacy Officer nomeado (Gustavo interim)   | EVT-032                  |
| Art. 48 | Comunicação de incidente à ANPD                        | `RB-BREACH-NOTIF`                           | EVT-017           |

### 4.2 Ponto crítico: tenants que usam CoreLink para dados pessoais

CoreLink pode armazenar blobs contendo dado pessoal do **cliente final do tenant**. Neste caso:

- **Tenant = controlador** do dado final.
- **HuGR = operador** (Art. 5, VII LGPD / Art. 4 GDPR).
- Contrato: DPA (ver `legal/dpa/`).
- Obrigações HuGR: suporte a DSR do tenant, breach notification 48h, minimização/segurança, não uso fora do contrato.

---

## 5. GDPR (UE)

### 5.1 Artigos-chave

| Art.      | Requisito                                          | Como atendemos                            |
|-----------|----------------------------------------------------|--------------------------------------------|
| Art. 5    | Principles                                          | `privacy_model.md §1`                      |
| Art. 6    | Legal basis                                         | §9 privacy                                  |
| Art. 13/14 | Information to data subject                       | Privacy notice `/privacy`                  |
| Art. 15–22 | Rights of data subject (DSRs)                    | §6 privacy                                  |
| Art. 25   | Data protection by design/default                   | Todo este spec framework                    |
| Art. 28   | Processor obligations                               | DPA + sub-processors mgmt                   |
| Art. 30   | Records of processing                               | Audit events                                |
| Art. 32   | Security of processing                              | `security_model.md`                         |
| Art. 33   | Breach notification to DPA (72h)                    | `RB-BREACH-NOTIF`                           |
| Art. 34   | Breach notification to subjects                     | `RB-BREACH-NOTIF`                           |
| Art. 35   | DPIA                                                | Por WI HIGH_RISK                            |
| Art. 44–49 | International transfer                             | SCC; §7 privacy                              |

### 5.2 Schrems II readiness

- TIA (Transfer Impact Assessment) template em `legal/tia/`.
- Sob demanda cliente EU, entregamos TIA + SCC + lista sub-processores.

---

## 6. Outras regulamentações relevantes

### 6.1 HIPAA (US — saúde)

Opt-in por cliente enterprise. Requer:
- BAA (Business Associate Agreement) assinada.
- Audit anual pelo cliente.
- Controles adicionais: PHI encryption opcional (BYOK), audit logs com 6+ anos.

### 6.2 CCPA/CPRA (Califórnia)

Aligned com GDPR em essência. Direitos de "opt-out of sale" — **CoreLink não vende dados** (declarar na privacy notice).

### 6.3 SOC 3

Report público derivado de SOC 2 Type II. Distribuível livremente (marketing). Planejado pós Type II.

---

## 7. Sub-processor compliance posture

| Sub-processor         | Uso                              | Certificações                                              | DPA link                | Residency            |
|-----------------------|----------------------------------|-------------------------------------------------------------|--------------------------|---------------------|
| Cloudflare (Workers, R2, KV, DO, D1) | Infra core           | SOC 2 Type II, ISO 27001, ISO 27018, PCI-DSS, HIPAA (plano) | CF DPA standard          | Region selectable    |
| Neon                   | Postgres (control plane)          | SOC 2 Type II, HIPAA, ISO 27001                             | Neon DPA                 | US or EU             |
| Grafana Cloud          | Metrics/logs/traces                 | SOC 2 Type II, ISO 27001, HIPAA                             | Grafana DPA              | EU/US selectable     |
| Stripe                 | Billing                             | PCI-DSS Level 1, SOC 1/2                                    | Stripe DPA               | US (some EU)         |
| GitHub                 | Source + CI                         | SOC 1/2, FedRAMP                                             | GitHub DPA               | US                   |
| Sigstore               | Supply chain attestation            | OSS                                                          | —                        | US (public logs)      |
| PagerDuty              | Oncall                              | SOC 2, ISO 27001                                             | PagerDuty DPA            | US/EU                |

> Lista mantida em `/privacy/sub-processors` (público) + `legal/sub-processors.md` (source).

---

## 8. Evidence collection strategy

### 8.1 Princípio

**Audit evidence is a byproduct, não um chore.** Todo control gera evidence automaticamente via CI/observability. Humano só revisa e assina.

### 8.2 Tipos de EVT consumidos (taxonomia completa em `00_framework.md §35.7`)

| EVT category        | Frequência automática  | Retention           |
|---------------------|-------------------------|---------------------|
| EVT-001          | Cada PR                | 7y (audit-relevant) |
| EVT-005       | Cada PR                | 7y                  |
| EVT-010            | Cada release            | 7y                  |
| EVT-011 | Cada release            | 7y                  |
| EVT-025  | Anual + ad-hoc          | 7y                  |
| EVT-022 | PR que toca INV CRITICAL | 7y                  |
| EVT-019 | Pós incident SEV-1/2 | 7y                  |
| EVT-013 | Mensal (auto)       | 7y                  |
| EVT-047 (AUDIT_EVENT) | Contínuo                | 7y (Object Lock)    |

### 8.3 Audit log schema (CloudEvents)

Ver `observability_model.md §7`. Imutabilidade: R2 Object Lock Governance Mode; hash-chain verificado diariamente (PAT-AUDIT-VERIFY-001).

### 8.4 Access to evidence durante audit

- Auditor externo ganha acesso read-only via SSO + ticket temporário (expira 30d).
- Todas as consultas de auditor logadas em audit trail (meta-audit).
- Evidence redacted por tenant quando aplicável (auditor não precisa ver `tenant_id` ≠ generic).

---

## 9. Gap analysis

Este doc é o acordo atual de roadmap. Gaps conhecidos:

| Gap ID | Descrição                                                     | Plano                                         | Owner            |
|--------|---------------------------------------------------------------|-----------------------------------------------|------------------|
| GAP-01 | DPO formal vs. interim Privacy Officer                         | Contratar DPO antes de ingressar EU enterprise | Gustavo         |
| GAP-02 | BCP/DRP documentado                                             | WI em fase "pré-GA hardening"                 | SRE Lead        |
| GAP-03 | TIA template                                                    | Criar junto com primeiro tenant EU             | Legal           |
| GAP-04 | SOC 2 readiness gap analysis                                    | Mês 3 pós GA                                  | Compliance Officer |
| GAP-05 | ISO 27001 SoA                                                   | Depende de SOC 2 Type II                       | Compliance Officer |
| GAP-06 | Legal hold process automation                                   | WI explícito pós GA                           | Legal + SRE      |
| GAP-07 | Evidence collection automation end-to-end                       | Mês 6 pós GA (Drata/Vanta integração avaliada) | Compliance Officer |

---

## 10. Referências

### 10.1 Frameworks

- **AICPA Trust Services Criteria 2017 (rev. 2022)** — SOC 2.
- **ISO/IEC 27001:2022** + Anexo A.
- **NIST SP 800-53 rev. 5** — para crosswalk.
- **CIS Controls v8** — para crosswalk com ISO.
- **Cloud Security Alliance CCM v4** — shared responsibility.

### 10.2 Legislação

- LGPD — Lei 13.709/2018.
- GDPR — Regulamento (UE) 2016/679.
- CCPA/CPRA.
- HIPAA Security Rule (45 CFR 164.302-318).

### 10.3 Auditores potenciais

- Big 4 (Deloitte, KPMG, EY, PwC) — SOC 2.
- Schellman, A-LIGN — SOC 2 + ISO 27001, experiência SaaS.
- Prescient Assurance — SOC 2 rápido.

### 10.4 Ferramentas de evidence automation

- **Drata** — popular em SaaS early-stage.
- **Vanta** — competitor, bom para multi-framework.
- **Secureframe** — competitor.
- **OneTrust** — enterprise, privacy-first.

---

**Fim de COMPLIANCE-MATRIX.** Mudanças em frameworks externos (ex: SOC 2 2024 revision) requerem update deste doc dentro de 90d da publicação.
