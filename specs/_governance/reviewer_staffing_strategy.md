---
id: "REVIEWER-STAFFING-STRATEGY"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-25"
updated: "2026-04-25"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["governance", "reviewers", "staffing", "process", "framework-freeze"]
---

# Reviewer Staffing Strategy — Path do DRAFT/REVIEW para FROZEN

> **Propósito**: definir estratégia executável para staffing de reviewers humanos required para promover meta-contract + framework de REVIEW → FROZEN.
>
> Adicionado em Lote 9.5b endereçando **Opus R3 C-N3-02** + Codex R3-18: "Meta-contract REVIEW status com 0 reviewers humanos = governance circular (AI auditando spec criada com input AI)".

---

## Sumário

1. [Status atual da pilha de governança](#1-status-atual-da-pilha-de-governança)
2. [Reviewer roles definitions](#2-reviewer-roles-definitions)
3. [Tier 1 — Meta governance (urgent)](#3-tier-1--meta-governance-urgent)
4. [Tier 2 — Lane-specific bench](#4-tier-2--lane-specific-bench)
5. [Tier 3 — Domain SMEs (deferred)](#5-tier-3--domain-smes-deferred)
6. [Path para FROZEN](#6-path-para-frozen)
7. [Fallback strategies](#7-fallback-strategies)

---

## 1. Status atual da pilha de governança

| Doc | Status atual | Reviewers staffed | Path para FROZEN |
|---|---|---|---|
| `00_framework.md` | DRAFT v1.0.0-rc1 | 0 humanos | Tier 1 staffing |
| `_sprint_creation_contract.md` | REVIEW v1.0.1 | 2 AI (Codex GPT R1+R2, Opus 4.7 R2 independent) | Tier 1 staffing humano + jsonschema CI |
| 14 canonical sources `03_architecture/` | DRAFT (várias versões) | 0 humanos | Tier 2/3 |
| 21 sprint contracts | DRAFT v1.1 (1 FROZEN: S-01) | 0 humanos | Per-sprint Tier 2 |
| 19 ADRs | DRAFT (várias versões) | 0 humanos | Per-ADR Tier 2/3 |

**Risk circular governance** (Opus C-N3-02): AI gerou specs + AI auditou specs + nenhum humano qualificado validou. Single-point-of-failure se Founder/Owner indisponível.

---

## 2. Reviewer roles definitions

### 2.1 Architecture Reviewer

- **Background**: ≥ 5 anos de design distribuído (Cloudflare/AWS/GCP); experience com REAPI/remote cache OR similar protocol-design space.
- **Coverage**: framework + sprint contracts (HIGH_RISK lane) + ADRs arquiteturais.
- **Time commitment**: ~5-10h/sprint review.

### 2.2 Security Reviewer

- **Background**: ≥ 3 anos AppSec OR application security; OWASP ASVS familiarity; threat modeling (STRIDE/LINDDUN); pentest experience.
- **Coverage**: security_model.md + privacy_model.md + auth_model.md + sprints com FF-HR-005.
- **Time commitment**: ~3-5h/HIGH_RISK sprint review.

### 2.3 Privacy / Compliance Reviewer

- **Background**: ≥ 3 anos privacy ops; LGPD + GDPR + CCPA familiarity; DPO certification preferred.
- **Coverage**: privacy_model.md + compliance_matrix.md + S-11 + S-14 + S-19.
- **Time commitment**: ~2-4h per privacy-touching sprint.

### 2.4 SRE / Operations Reviewer

- **Background**: ≥ 3 anos production SRE; SLO discipline; incident response.
- **Coverage**: observability_model.md + slo_catalog.md + failure_modes.md + S-09/S-13/S-17.
- **Time commitment**: ~2-4h per HIGH_RISK sprint review.

### 2.5 Product / DX Reviewer

- **Background**: ≥ 5 anos product management OR developer experience (DX); Bazel/Buck2 ecosystem familiarity.
- **Coverage**: PRFAQ + capabilities catalog + S-15/S-16/S-18/S-19.
- **Time commitment**: ~2-4h per product-facing sprint review.

### 2.6 Crypto SME (advisory)

- **Background**: cryptographic engineering; HMAC/Argon2id/BLAKE3/Ed25519; FIPS 140-3 familiarity.
- **Coverage**: key_management.md + ADRs crypto-touching (0014/0015/0018/0021) + S-01/S-03/S-04/S-14.
- **Time commitment**: ~1-2h per crypto-touching sprint review.
- **Frequency**: advisory; engaged ad-hoc per sprint demand.

---

## 3. Tier 1 — Meta governance (urgent)

**Goal**: 2 reviewers humanos para framework + meta-contract → enable promotion REVIEW → FROZEN.

| Role | Priority | Required for FROZEN |
|---|---|---|
| Architecture Reviewer | P0 | framework `00_framework.md` |
| Security Reviewer | P0 | framework + meta-contract security clauses |

**Recommended sourcing options:**

1. **Internal hire** (preferred long-term): full-time SRE + Tech Lead (Founder backup).
2. **External advisory** (interim): contract reviewers from Cloudflare/Stripe/Auth0 partner network OR open source community.
3. **Peer projects** (short-term): trusted contacts em projetos adjacentes (mesmo founder); 1-2h review sessions.
4. **AI as Tier 0** (current; persistent): Codex GPT + Opus 4.7 audits — NÃO substituem humano mas reduzem effort humano para 30-50% de "checkpoint review" vs "full read".

**Action items pré-FROZEN promotion:**

- [ ] Hire OR engage Architecture Reviewer (signed advisory contract OR full hire).
- [ ] Hire OR engage Security Reviewer.
- [ ] Both review framework v1.0.0-rc1 → produce comments doc.
- [ ] Address comments → bump v1.0.0-rc2 → v1.0.0 release.
- [ ] Both review meta-contract v1.0.1 → REVIEW → FROZEN.

---

## 4. Tier 2 — Lane-specific bench

**Goal**: per-lane specialized reviewers para sprint sealing.

### 4.1 HIGH_RISK lane bench

Required per HIGH_RISK sprint sealing (10-12 sign-offs):

- Architecture Reviewer (Tier 1).
- Security Reviewer (Tier 1).
- SRE / Operations Reviewer.
- 2 peer code reviewers (engineering team; rotate).
- Compliance Officer.
- Privacy Officer (if FF-HR-003 active).
- AppSec advisor (if FF-HR-005 active).
- Crypto SME (if crypto-touching).

### 4.2 STANDARD lane bench

5-8 sign-offs per sprint:

- Tech Lead (Founder OR Architecture Reviewer).
- SRE Lead.
- 2 peer reviewers.
- Product (DX Reviewer if product-facing).
- Compliance Officer (if compliance-touching).

### 4.3 LOW_RISK lane bench

3 sign-offs:

- Owner (Founder).
- 1 peer reviewer.
- Domain reviewer (e.g., Docs lead for S-18).

---

## 5. Tier 3 — Domain SMEs (deferred to per-sprint engagement)

Engaged ad-hoc per sprint:

- Crypto SME (S-01/S-03/S-04/S-14): HKDF + Argon2id + BLAKE3 + Ed25519 + KMS BYOK review.
- DevX advisor (S-15): SDK FFI ergonomics review.
- a11y advisor (S-16): WCAG 2.2 AA audit.
- Legal (S-11/S-14/S-18/S-19): DPA + Schrems II TIA + Terms.
- Finance (S-10/S-18): pricing + revenue recognition.
- DPO interim (Founder until separate hire) (S-11/S-14).

---

## 6. Path para FROZEN

### 6.1 Framework v1.0.0-rc1 → v1.0.0 (FROZEN)

```
Month 0 (current Lote 9.5b): REVIEW status with AI reviewers stamped
Month 1: Tier 1 hire/engage; review v1.0.0-rc1 → comments
Month 1.5: v1.0.0-rc2 with comments addressed
Month 2: Tier 1 final approval → v1.0.0 FROZEN
```

### 6.2 Meta-contract v1.0.1 → FROZEN

Same path; concurrent with framework.

### 6.3 Per-sprint contracts → SEALED

Per-sprint Tier 2 bench engages per sprint kick-off.
Sprint contracts já em DRAFT v1.1 esperam Tier 2 staffing antes de sprint kick-off.

### 6.4 Schema validator dependency (jsonschema)

Independent fix:
- [ ] Add `jsonschema` to CI Python deps (pyproject.toml or requirements-ci.txt).
- [ ] CI workflow runs `python3 scripts/validate_specs.py` em PR.
- [ ] All new docs require schema-valid front matter.

---

## 7. Fallback strategies

Se Tier 1 staffing não materialize em 2 meses:

### 7.1 AI-only continuance (ATUAL DEFAULT)

- Continuar com Codex GPT + Opus 4.7 R3+ rounds.
- Document explicitly em meta-contract: "AI-stamped REVIEW until human Tier 1 staffed".
- Limitation: framework não promove a FROZEN; sprints permanece em DRAFT WI-level.
- **Risk**: governance gap se AI errs sem humano para catch.

### 7.2 Founder-only sign-off (STANDARD lane only)

- Founder/Owner é único Tier 1 reviewer.
- HIGH_RISK sprints aguardam staffing.
- LOW_RISK + STANDARD sprints podem proceder.
- **Risk**: HIGH_RISK sprints stuck; Bazel/Buck2 cache foundation S-01..S-06 já feita; S-09+ pode atrasar.

### 7.3 Phase 1 GA com waivers explícitos

- GA com Tier 1 staffing partial; waivers documentos per item missing.
- Pos-GA Q1 hire para complete bench.
- **Risk**: customer trust impact se waivers visíveis em compliance docs.

### 7.4 Open source contribution pathway

- Promote framework + canonical sources como open source.
- Public review process via GitHub PRs.
- Community reviewers Tier 2/3.
- **Risk**: timeline incerta; community engagement requer effort dedicated.

---

## 8. Recommendation for Lote 9.5b sealing

**Decisão**: continuar com **fallback 7.1 (AI-only continuance)** documentado explicitly em meta-contract REVIEW status, **PARALELO** com:

1. **Action item P0**: engage Architecture Reviewer (advisory contract; 4h/month commitment) dentro de 30d pós-Lote 9.5b sealing.
2. **Action item P0**: engage Security Reviewer (advisory contract; 2-3h/month) dentro de 30d.
3. **Action item P1**: install jsonschema CI dependency (1h trabalho); enable validate_specs.py em CI.

Com Tier 1 advisory engaged + jsonschema CI live → meta-contract pode promover REVIEW → FROZEN em ~Month 1.5.

Sem isso, S-02 implementation pode começar (S-02 spec contract v1.1 + sprint.md done) MAS PRR completion fica blocked até Tier 1 reviewers stamped.

---

## 9. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação reviewer staffing strategy doc (Lote 9.5b Phase 11 — Opus R3 C-N3-02 + Codex R3-18 fix). |

---

**Cross-reference**: meta-contract `_sprint_creation_contract.md` REVIEW status; framework `00_framework.md` DRAFT v1.0.0-rc1.
