---
id: "TIMING-GATES-CUMULATIVE"
type: "architecture"
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
tags: ["architecture", "timing", "observation-gates", "cumulative", "wall-clock", "ga"]
---

# Timing Gates Cumulative — Wall-Clock Calendar de Observation Periods

> **Propósito**: calendarizar observation gates declarados em sprint contracts (72h, 4 weeks, 30d sustained, etc.) em wall-clock real para entender se GA é factível dentro de 12 meses + identificar overlap real entre periods de observação.
>
> Adicionado em Lote 9.5b endereçando **Opus R3 H-N3-02** + Codex R2 H-04: "S-09 (72h synthetic), S-17 (4 weeks chaos), S-06 (30d post-sprint observation), S-20 (30d sustained staging) cumulativos formam calendar invisible".

---

## Sumário

1. [Observation gates declarados](#1-observation-gates-declarados)
2. [Wall-clock calendar](#2-wall-clock-calendar)
3. [Concurrent vs serial gates](#3-concurrent-vs-serial-gates)
4. [GA gate liberation criteria](#4-ga-gate-liberation-criteria)
5. [Risk: timing impossibilities](#5-risk-timing-impossibilities)

---

## 1. Observation gates declarados

| Gate | Sprint | Sustained period | Concurrent? | Wall-clock dependency |
|---|---|---|---|---|
| Synthetic canary | S-09 | 72h sustained 24/7 (3 regiões) | Yes (sustained ongoing pós-S-09 SEAL) | S-09 SEAL → continuous |
| Chaos automation | S-17 | 4 weeks staging | Yes (concurrent S-18..S-20) | S-17 SEAL → 4w period extends until S-20 start |
| GC post-sprint observation | S-06 | 30d staging concurrent S-07/S-08 | Yes | S-06 SEAL → 30d concurrent |
| Refcount drift sustained | S-06 | 7d sustained < 0.1% | Yes | S-06 SEAL → 7d window |
| Reconciliation drift | S-10 | 30d sustained zero drift > 0.1% | Yes | S-10 SEAL → continuous to S-20 |
| BYOK kill switch chaos drill | S-14 | weekly + 30d sustained | Yes | S-14 SEAL → continuous |
| GA staging sustained | S-20 | 30d sustained zero SEV-1 | **Sequential — pré-GA gate** | Final gate antes de GA |
| External pentest + retest | S-20 | 2w + 1w | Sequential dentro S-20 | S-20 sprint window |
| 3 lighthouse customers SLA met | S-20 | 30d observation | Concurrent com S-20 staging | S-20 sprint window |

---

## 2. Wall-clock calendar

Assumindo timeline ideal (S-00 começou 2026-04-24; cada sprint sequencial com lane-typical duration + buffer):

| Sprint | Lane | Duração + buffer | Cumulative day | Cumulative date |
|---|---|---|---|---|
| S-00 | STANDARD | 5d + 0d | D+5 | 2026-04-29 |
| S-01 | HIGH_RISK | 3w + 5d | D+30 | 2026-05-24 |
| S-02 | HIGH_RISK | 3w + 5d | D+55 | 2026-06-18 |
| S-03 | HIGH_RISK | 4w + 7d | D+90 | 2026-07-23 |
| S-04 | HIGH_RISK | 3w + 5d | D+115 | 2026-08-17 |
| S-05 | HIGH_RISK | 3w + 5d | D+140 | 2026-09-11 |
| S-06 | HIGH_RISK | 4w + 7d | D+175 | 2026-10-16 |
| S-07 | STANDARD | 2.5w + 3d | D+193 | 2026-11-03 |
| S-08 | HIGH_RISK | 2.5w + 5d | D+213 | 2026-11-23 |
| S-09 | HIGH_RISK | 2.5w + 3d | D+231 | 2026-12-11 |
| S-10 | HIGH_RISK | 3w + 7d | D+259 | 2027-01-08 |
| S-11 | HIGH_RISK | 3w + 7d | D+287 | 2027-02-05 |
| S-12 | HIGH_RISK | 2.5w + 3d | D+305 | 2027-02-23 |
| S-13 | HIGH_RISK | 2.5w + 3d | D+323 | 2027-03-13 |
| S-14 | HIGH_RISK | 4w + 10d | D+361 | 2027-04-20 |
| S-15 | STANDARD | 2.5w + 3d | D+379 | 2027-05-08 |
| S-16 | STANDARD | 3w + 5d | D+405 | 2027-06-03 |
| S-17 | STANDARD | 4w + 5d | D+438 | 2027-07-06 |
| S-18 | LOW_RISK | 2w + 2d | D+454 | 2027-07-22 |
| S-19 | HIGH_RISK | 2.5w + 3d | D+472 | 2027-08-09 |
| S-20 | HIGH_RISK | 4w + 10d | D+510 | 2027-09-16 |

**Total wall-clock**: ~510 dias = **~17 meses** (não 12 meses como roadmap S-00 ideal).

**Reality check**: 12 meses só factível com paralelismo em STANDARD/LOW_RISK (S-15/S-16/S-17/S-18 podem rodar paralelos com HIGH_RISK final stretch).

---

## 3. Concurrent vs serial gates

### 3.1 Concurrent observation periods (não somam wall-clock)

- **S-06 30d obs** + **S-07/S-08** sprints: S-06 SEAL D+175; S-07 começa D+175; durante D+175..D+205 (S-07 sprint) S-06 obs roda concurrent.
- **S-09 72h synthetic** + S-10..S-20: S-09 SEAL D+231; 72h period D+231..D+234; depois ongoing continuous (não bloqueia next sprint).
- **S-17 4w chaos** + **S-18/S-19/S-20**: S-17 SEAL D+438; 4w period D+438..D+466; S-18 começa D+438 concurrent; S-20 começa D+472 concurrent (S-17 4w já terminou).

### 3.2 Sequential gates (somam wall-clock)

- **S-20 30d staging sustained** dentro do sprint S-20: 30d sequencial dentro do 4w + 10d buffer = **timing impossível** (Codex R3-03 + Opus C-04 reaffirmed).
- **S-20 lighthouse customers 30d observation** dentro do sprint: 30d sequencial.
- **S-20 pentest 2w + retest 1w**: 5w sequencial dentro de 4w + 10d buffer = timing impossível.

---

## 4. GA gate liberation criteria

Para S-20 SEAL → GA announcement:

### 4.1 Engineering gate (binary, dentro do sprint)

- All 7 WIs SEALED (S-20 §6.1 atualizado Lote 9.4).
- All TLA+ specs em registry §4.1 GREEN sustained.
- All 26+ runbooks dry-run em últimos 90d (S-17 + S-20 cumulative cadence).
- SBOM CycloneDX 1.5+ signed published.
- Zero active waivers em controles CRITICAL.

### 4.2 Observation gate (concurrent, requer pré-S-20 start)

**Decisão Lote 9.5b**: observation periods que requerem 30d devem **começar antes do sprint S-20 start**, não dentro:

- **30d staging sustained**: começa D-30 antes de S-20 (D+440 se S-20 começa D+472). Concurrent com S-19. Documented em S-20 §13 timeline.
- **30d lighthouse customer observation**: começa após customer migration (S-20 sprint window D+472 + 30d = D+502). Sprint S-20 close D+510 → 8d margin (tight; aceitável).
- **External pentest 2w + retest 1w**: scheduling antecipado D+462 (10d antes S-20 start) → pentest D+462..D+476 → retest D+476..D+483 → remediation D+483..D+497 → S-20 close D+510 OK.

### 4.3 Launch gate (separated from engineering; soft)

- Marketing prep (WI-S20-008): non-blocking pra engineering gate per Lote 9.4 split.
- Press release + blog posts + case studies: paralelo com observation gate.
- GA announcement: launch orchestration after engineering + observation gates ambos green.

---

## 5. Risk: timing impossibilities

| Risk | Source | Mitigation |
|---|---|---|
| **S-20 30d staging dentro sprint window 4w+10d**: matemática infactível (Codex R3-03; Opus C-04) | S-20 §13 | Lote 9.5b §4.2 acima: observation começa D-30 antes S-20 start. Atualizar S-20 §13 timeline. |
| **Lighthouse 30d obs vs customer migration time**: customer might not migrate at D+472 exactly | S-20 R-S20-002 | Engage lighthouse customers at S-19 SEAL D+472 minus 60d (pre-engagement); migration target window D+472..D+475 |
| **Pentest scheduling slippage**: external firm availability | S-20 R-S20-001 | Schedule contract D+200 (early); flex 1-2 weeks |
| **Roadmap 12 vs 17 months reality**: serial timing exceeds budget | S-00 roadmap | Parallelism em STANDARD/LOW_RISK lanes; staffing decision (mais engineers); OR reduce GA scope |

---

## 6. Update needed em sprint contracts (Lote 9.5b followup)

- [ ] **S-20 §13 timeline**: documentar explicitamente que 30d staging + pentest + lighthouse obs começam pré-S-20 sprint start (não dentro).
- [ ] **S-06 §13 timeline**: já documentado "concurrent S-07/S-08" — OK.
- [ ] **S-09 §6 DoD**: 72h synthetic é continuous post-SEAL; OK como-está.
- [ ] **S-17 §13 timeline**: 4w chaos é concurrent post-SEAL; já documentado parcialmente.

---

## 7. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 0.1.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação timing-gates calendar (Lote 9.5b Phase 9 — Opus H-N3-02 + Codex R2 H-04 fix). |

---

**Cross-reference**: invariant_registry §4 (TLA+ obligation matrix), S-06/S-09/S-17/S-20 sprint contracts §13 timeline.
