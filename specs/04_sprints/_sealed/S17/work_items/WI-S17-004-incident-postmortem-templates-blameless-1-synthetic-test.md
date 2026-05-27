---
id: "WI-S17-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
parent: "S-17"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "FAILURE-MODES"
  - "OBSERVABILITY-MODEL"
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
tags: ["wi", "s17", "ops", "incident-template", "post-mortem", "blameless", "5-why", "synthetic-test", "all-hands-training", "standard"]
---

# WI-S17-004 — Incident Template `specs/_templates/incident.md` (Header com Severity SEV-1/2/3 + Start/end ts + Services Affected + Customer Impact Estimate; Timeline com Events ts + Actor; Resolution com Actions Taken + Verification) + Post-mortem Template `specs/_templates/post_mortem.md` Blameless Culture Enforced (Sin Nomeação de Blame; Foco em System/process Improvements; 5-Why Analysis Structured Questions; Action Items com Owner + Due Date + Status Tracking; Lessons Learned Positive + Negative; SRE Lead Reviews per Quality Standard 14.s17.3) + 1 Synthetic Incident Test Full Flow (Synthetic SEV-2 Simulado em Staging; Team Works Through Incident Response per Templates; Post-mortem Produced + Reviewed + Action Items Tracked) + Engineering All-hands Training (1.5h Session; Blameless Culture Reinforce; 5-Why Technique Training; Q&A; Community Reinforcement) + Post-mortems Retroactively Linked a Sprint (Sprint Owner Accepts/rejects Action Items per Spec Contract §5.4 R-S17-11)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-17](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S17-004 |
| Título | Incident + post-mortem templates blameless + 1 synthetic incident test + engineering all-hands training. |
| Sprint | S-17 |
| Lane | STANDARD |
| Forcing factors | none (templates discipline + culture training; não introduz tenant data path novo) |

## 1. Intent

Templates `specs/_templates/incident.md` + `specs/_templates/post_mortem.md` + 1 synthetic incident test (SEV-2 simulado em staging com full flow) + engineering all-hands training (blameless culture reinforce + 5-Why technique).

```markdown
<!-- File: specs/_templates/incident.md -->
---
id: "INC-YYYY-MM-DD-NNN"
type: "incident"
severity: "SEV-1 | SEV-2 | SEV-3"
status: "open | mitigated | resolved | closed"
start_ts: "YYYY-MM-DDTHH:MM:SSZ"
end_ts: "YYYY-MM-DDTHH:MM:SSZ"
services_affected: ["api", "audit", "dsr", ...]
customer_impact_estimate: "..."
incident_commander: "..."
correlation_id: "..."  # PAT-CORRELATION-ID-001
---

# INC-YYYY-MM-DD-NNN — <título>

## Header
- Severity: SEV-X
- Start ts: ...
- End ts: ...
- Services affected: [...]
- Customer impact estimate: ...
- Incident commander: ...

## Timeline
| ts | actor | event |
|---|---|---|
| ... | ... | ... |

## Resolution
- Actions taken: ...
- Verification: ...
- MTTA (mean time to acknowledge): X seconds (target SEV-1 < 300s)
- MTTR (mean time to resolve): X seconds (target SEV-1 < 1800s)

## Post-mortem
- Doc link: PM-YYYY-MM-DD-NNN
```

```markdown
<!-- File: specs/_templates/post_mortem.md -->
---
id: "PM-YYYY-MM-DD-NNN"
type: "post_mortem"
incident_id: "INC-YYYY-MM-DD-NNN"
severity: "SEV-X"
blameless: true  # culture enforcement; NUNCA false
sprint_link: "S-XX"  # retroactive link per R-S17-11
---

# PM-YYYY-MM-DD-NNN — <título>

## Summary (blameless; sin nomeação de blame; foco em system/process improvements)
...

## 5-Why Analysis
1. Why? ...
2. Why? ...
3. Why? ...
4. Why? ...
5. Why? (root cause em system/process)

## Action Items
| ID | Item | Owner | Due Date | Status |
|---|---|---|---|---|
| AI-1 | ... | ... | YYYY-MM-DD | open |

## Lessons Learned
### Positive
- ...

### Negative (system/process gaps)
- ...

## Sprint Linkage
Sprint S-XX owner accepts/rejects action items (per R-S17-11):
- AI-1: accepted | rejected | deferred (decision rationale)

## Reviews
- SRE lead: ... (signed YYYY-MM-DD)
- [if security incident] Security: ...
- [if privacy incident] Privacy officer: ...
```

## 2. Narrative

Google SRE Book Ch 15 (Postmortem Culture) + Atlassian Blameless Post-Mortem Template são canonical references. CoreLink S-17 entrega templates committed + 1 synthetic SEV-2 incident test full flow + engineering all-hands training (blameless culture reinforce + 5-Why technique).

Blameless culture enforced em template structure: `blameless: true` field mandatory; sin nomeação de blame em summary; foco em system/process improvements em 5-Why root cause analysis; SRE lead reviews per Quality Standard 14.s17.3.

**Risk justification STANDARD lane**:
- Templates + culture training são process discipline (não cripto-load-bearing).
- Reuse PAT-CORRELATION-ID-001 já em resilience_patterns.
- Não introduz tenant data path novo.

## 3. Customer Impact & Journey

**Persona — Engineer / Oncall**:
- Templates committed = clear incident response baseline.
- Blameless culture = psychological safety baseline.
- 5-Why technique = systematic root cause analysis.
- Sprint linkage = action items tracked + accountable.

**Persona — Compliance auditor**:
- Templates committed em git = audit trail forensic-grade.
- Blameless culture documented + trained = SOC 2 incident response baseline.
- Sprint linkage = continuous improvement evidence.

## 4. Capability Mapping

- **CAP-OPS-004** (incident template + post-mortem workflow) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4` + Google SRE Book Ch 15 + Atlassian template.

## 5. Tipo

Templates + culture training WI; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **Incident template** `specs/_templates/incident.md`:
   - Front matter per `incident` type schema.
   - Header com severity SEV-1/2/3 + start/end ts + services affected + customer impact estimate + incident commander + correlation_id (PAT-CORRELATION-ID-001 reflection).
   - Timeline com events ts + actor.
   - Resolution com actions taken + verification + MTTA + MTTR.
   - Post-mortem doc link.

2. **Post-mortem template** `specs/_templates/post_mortem.md`:
   - Front matter per `post_mortem` type schema.
   - `blameless: true` field mandatory (NUNCA false; culture enforcement).
   - Summary blameless (sin nomeação de blame; foco em system/process improvements).
   - 5-Why analysis (5 structured questions; root cause em system/process).
   - Action items com owner + due date + status tracking.
   - Lessons learned positive + negative.
   - Sprint linkage (sprint owner accepts/rejects action items per R-S17-11).
   - Reviews (SRE lead canonical; Security if security incident; Privacy officer if privacy incident).

3. **1 synthetic incident test full flow**:
   - Synthetic SEV-2 incident simulado em staging.
   - Team works through incident response per templates.
   - Incident doc + post-mortem produced.
   - Action items tracked + retroactive sprint linkage.
   - Reviewed pelo SRE lead + Engineering.

4. **Engineering all-hands training** (1.5h session):
   - Blameless culture reinforce (Google SRE Book Ch 15 reference).
   - 5-Why technique training (structured root cause analysis).
   - Q&A.
   - Community reinforcement (sin name shaming culture).
   - Recorded + archived em `docs/internal/2026-XX-XX-blameless-culture-training.md`.

5. **Métricas Prometheus** snake_case canonical:
   - `corelink_postmortem_total{severity, blameless}` (counter; alert se blameless=false).
   - `corelink_incident_mtta_seconds{severity}` (gauge; SEV-1 target < 300s).
   - `corelink_incident_mttr_seconds{severity}` (gauge; SEV-1 target < 1800s).

### 6.2 Out-of-scope (deferred)

- AI-powered RCA (pós-GA Q1+; manual blameless analysis at GA per spec contract §10).
- Multi-vendor incident orchestration (xMatters, FireHydrant; PagerDuty only at GA).
- Public status page customer-facing automation (manual at GA).
- Continuous incident response training (pós-GA Q1+).

## 7. Anti-Scope

- Skip blameless culture enforcement (culture violation = post-mortem trigger).
- Skip 5-Why analysis (root cause baseline gap).
- Skip 1 synthetic incident test (full flow validation gap).
- Skip engineering all-hands training (culture reinforce gap).
- Templates without sprint linkage (continuous improvement gap).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Incident + post-mortem templates + blameless + synthetic test

  Scenario: Incident template committed
    Given specs/_templates/incident.md
    When inspected
    Then front matter per incident schema
    And header + timeline + resolution sections canonical
    And MTTA + MTTR fields included

  Scenario: Post-mortem template committed
    Given specs/_templates/post_mortem.md
    When inspected
    Then front matter blameless: true mandatory
    And 5-Why analysis structure
    And action items table com owner + due date
    And lessons learned positive + negative
    And sprint linkage section

  Scenario: 1 synthetic SEV-2 incident test full flow
    Given synthetic incident simulado em staging
    When team works through response
    Then incident doc produced per template
    And post-mortem doc produced per template
    And action items tracked
    And SRE lead reviews

  Scenario: Engineering all-hands training delivered
    Given 1.5h session scheduled
    When session executed
    Then blameless culture content delivered
    And 5-Why technique trained
    And Q&A captured
    And recording archived

  Scenario: Sprint linkage retroactive
    Given post-mortem com sprint_link field
    When sprint owner reviews action items
    Then accepted/rejected/deferred decision documented
    And rationale captured

  Scenario: Blameless culture violation trigger
    Given post-mortem com blameless: false (violation)
    When committed
    Then SRE lead review flags violation
    And reinforcement training triggered
    And post-mortem rewritten

  Scenario: Métricas Prometheus snake_case
    Given incident + post-mortem committed
    When métricas emitted
    Then corelink_postmortem_total counter incremented
    And corelink_incident_mtta_seconds gauge set
    And corelink_incident_mttr_seconds gauge set

  Scenario: PAT-CORRELATION-ID-001 reflection
    Given incident template
    When committed
    Then correlation_id field em header
    And propagated em timeline events
```

## 9. Design Decisions

### 9.1 Why blameless culture enforced via template field + programmatic content check + 2-reviewer sign-off (Lote 10.17 codex P2 canonical fix)

- **Layer 1 — `blameless: true` field mandatory**: code-level enforcement vs convention-only (validate_post_mortem.py CI gate rejects PR se field missing or false).
- **Layer 2 — programmatic blameful-content lint** (Lote 10.17 codex P2 strengthening): regex/keyword scanner em CI checks for blameful phrases (e.g., "should have", "fault of", "failed to", "negligent", first-person blame "X did/didn't"); flagged content blocks PR. Canonical phrase blocklist em `scripts/blameless_lint.py` reviewed annually by SRE lead.
- **Layer 3 — 2-reviewer sign-off mandatory** (não self-certification): post-mortem requires 2 reviewers (SRE lead + 1 peer outside affected team) approving before final-state; prevents author self-certifying their own post-mortem as blameless.
- Google SRE Book Ch 15 reference = industry-leading.
- Atlassian template reference.
- Codex P2 finding: `blameless: true` field alone era insufficient — convention/marker field can be set without actually being blameless content; layered enforcement (field + lint + reviewer) addresses this.

### 9.2 Why 5-Why technique (não Fishbone ou other)

- 5-Why = simple, scalable, root cause focus em system/process.
- Fishbone = good for category exploration; less effective for root cause.
- 5-Why scaled successfully em Toyota Production System + Google SRE.

### 9.3 Why 1 synthetic incident test (não 0 ou 3)

- 1 = baseline validation full flow.
- 0 = templates untested = quality gap.
- 3 = scope creep within sprint.

### 9.4 Why engineering all-hands training (não async docs)

- Culture training = synchronous reinforcement (Google SRE pattern).
- Async docs sufficient for templates; insufficient for culture.
- 1.5h session balance time investment + quality.

### 9.5 Why MTTA < 5min + MTTR < 30min targets SEV-1 (canonical)

- Google SRE / PagerDuty industry-leading targets.
- CoreLink S-17 differentiator vs competitors.
- Per spec contract §9.8 canonical.

### 9.6 ADR potencial?

- Não — templates + culture training reuse industry references.

## 10. Completeness Criteria

- [ ] **10.s17.004.1** Incident template committed em `specs/_templates/incident.md`.
- [ ] **10.s17.004.2** Post-mortem template committed em `specs/_templates/post_mortem.md`.
- [ ] **10.s17.004.3** Templates reviewed Engineering + SRE (EVT-016).
- [ ] **10.s17.004.4** 1 synthetic SEV-2 incident test full flow executed (EVT-016).
- [ ] **10.s17.004.5** Engineering all-hands training delivered + recorded.
- [ ] **10.s17.004.6** Post-mortem retroactively linked a sprint (R-S17-11).
- [ ] **10.s17.004.7** Métricas snake_case Prometheus emitting.
- [ ] **10.s17.004.8** Blameless culture violation trigger documented em post-mortem hooks.
- [ ] **10.s17.004.9** PAT-CORRELATION-ID-001 reflection em incident template.
- [ ] **10.s17.004.10** MTTA < 5min + MTTR < 30min targets SEV-1 instrumented.

## 11. DoD

- [ ] Incident template committed.
- [ ] Post-mortem template committed.
- [ ] 1 synthetic incident test executed + post-mortem produced.
- [ ] All-hands training delivered.
- [ ] Métricas emitting.
- [ ] Adversarial scenarios 4+ documented.

## 12. Invariants Validated

- **PAT-CORRELATION-ID-001** (correlation_id em incidents) — IMPLEMENTA primary reflection.
- **CTRL-PRIV-001** (zero PII em post-mortem reports sanitized).
- **Não introduz INVs novas** (sprint operational; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Incident template | `specs/_templates/incident.md` | Markdown |
| Post-mortem template | `specs/_templates/post_mortem.md` | Markdown |
| Synthetic incident test report | `specs/_audits/2026-XX-XX-synthetic-incident-test.md` | Markdown |
| All-hands training recording | `docs/internal/2026-XX-XX-blameless-culture-training.md` + video link | Markdown + video |
| Synthetic post-mortem doc | `specs/_audits/2026-XX-XX-PM-synthetic-sev2.md` | Markdown |

## 14. Quality Standards

- **14.s17.004.1** Post-mortem blameless culture enforced — review by SRE lead; no name shaming (per Quality Standard 14.s17.3).
- **14.s17.004.2** PAT-CORRELATION-ID-001 reflection em incident template.
- **14.s17.004.3** Sprint linkage retroactive (R-S17-11).
- **14.s17.004.4** Cost regression gate: training infra ≤ $0/mês (internal sessions).

## 15. Test Plan

### Templates committed
- Verify front matter schemas valid.
- Verify required sections all present.
- Verify blameless: true field default.

### 1 synthetic SEV-2 incident test
- Synthetic incident simulado em staging (chaos test trigger SEV-2).
- Team responds per templates.
- Incident doc + post-mortem produced.
- Action items tracked.
- SRE lead reviews.

### Engineering all-hands training
- 1.5h session delivered.
- Recording archived.
- Q&A captured.

### Adversarial scenarios (4+)
1. Post-mortem culture violated (blame surfacing): SRE lead reviews; flags; reinforcement training; per spec contract §15 row 5 mitigation.
2. Synthetic incident test omits 5-Why: template enforcement; rewrite.
3. Sprint linkage missed: R-S17-11 enforcement; sprint owner accepts/rejects retroactive.
4. Blameless field set false (violation): post-mortem hook trigger fires.

### Verification
- Verify templates committed git.
- Verify synthetic incident report committed audit dir.
- Verify training recording archived.
- Verify métricas emitting.

## 16. Failure Modes

- **FM-202** (runbook stale): post-mortem may surface FM-202; mitigation via WI-S17-003 cycle.
- Post-mortem culture violation: covered em post-mortem hooks trigger.

## 17. Controls

- **PAT-CORRELATION-ID-001** in incident template + post-mortem timeline.
- **CTRL-PRIV-001** zero PII em post-mortem reports.

## 18. Resilience Patterns

- **PAT-CORRELATION-ID-001** (correlation_id em incidents) — primary reflection.
- Blameless culture (psychological safety pattern).
- 5-Why technique (root cause analysis pattern).

## 19. Observability

`corelink_incident_*` + `corelink_postmortem_*` métricas Prometheus snake_case:
- `corelink_postmortem_total{severity, blameless}` (counter; alert se blameless=false).
- `corelink_incident_mtta_seconds{severity}` (gauge; alert SEV-1 > 300s).
- `corelink_incident_mttr_seconds{severity}` (gauge; alert SEV-1 > 1800s).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: incident commander identified; correlation_id propagated.
- **Tampering**: templates git-tracked; post-mortems committed git.
- **Repudiation**: EVT-016 (HUMAN_SIGNOFF) + git history audit trail.
- **Information disclosure**: post-mortem sanitized; CTRL-PRIV-001 enforced; security incidents reviewed by Security; privacy incidents reviewed by Privacy officer.
- **DoS**: incident response cadence sustainable.
- **Elevation of privilege**: incident commander authority scoped.

**LINDDUN delta**:
- Linkability: incident métricas tenant-agnostic.
- Identifiability: post-mortem sanitized (no individual blame).
- Non-repudiation: git history.
- Detectability: post-mortem culture violation alerts.
- Disclosure: post-mortem scope-limited internal.
- Unawareness: blameless culture training reinforces psychological safety.

## 21. Dependencies

### Hard blockers
- PAT-CORRELATION-ID-001 já em resilience_patterns.

### Soft blockers
- WI-S17-001 SEALED (chaos automation reuse for synthetic incident trigger).

### Outbound
- WI-S17-005 (oncall manager reviews fadigue alerts via post-mortem cycle).
- WI-S17-006 (game day scenarios use post-mortem template).

## 22. Effort PERT

O: 8h, M: 14h, P: 22h → PERT **14.3h** (per spec contract §12; templates + synthetic test + all-hands training).

## 23. Cost Analysis

**Direct cost**:
- Internal training session: $0 (engineering team time).
- Recording infra: included em existing tier.

**Total**: $0/mês.

**Indirect cost**: Blameless culture violation avoided + MTTA/MTTR baseline = priceless.

## 24. Post-mortem Hooks

- Post-mortem culture violation (blame surfacing) → engineering review + reinforcement training.
- Synthetic incident test omits 5-Why → template enforcement.
- Sprint linkage missed → R-S17-11 enforcement review.

## 25. Rollback / Recovery

Templates rollback: git revert if breaking changes; backward compat baseline.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Post-mortem culture violated (blame surfacing) | M | M | MEDIUM (psych safety) | M | LOW | SRE lead reviews; engineering all-hands training; iterate |
| R-002 | Synthetic incident test omits steps | L | M | LOW | L | LOW | Template enforcement; SRE lead review |
| R-003 | Sprint linkage missed (R-S17-11) | M | L | LOW | L | LOW | Template field mandatory; sprint owner reviews |
| R-004 | All-hands training low attendance | M | L | LOW | L | LOW | Recording archived; async catch-up; Q&A captured |

## 27. Knowledge Transfer

- All-hands training (1.5h): "Blameless culture + 5-Why technique em CoreLink".
- Doc `docs/internal/s17-incident-response-procedure.md` — incident response procedure.
- Onboarding test (3 questions): blameless culture rationale + 5-Why technique + sprint linkage.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | SRE Lead | _TBD; emphatic — blameless culture review canonical_ | _pending_ | _pending_ |
| 4 | Engineer | Gustavo Schneiter | _pending_ | _pending_ |
| 5 | Oncall Manager | _TBD_ | _pending_ | _pending_ |
| 6 | QA | _TBD_ | _pending_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — post-mortem privacy incidents review_ | _pending_ | _pending_ |

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S17-004 (cycle 12.S17.0; STANDARD lane; incident + post-mortem templates blameless + 1 synthetic test + all-hands training). |

## 30. Anti-patterns evitados

- Skip blameless culture enforcement.
- Skip 5-Why analysis (root cause gap).
- Skip 1 synthetic incident test.
- Skip engineering all-hands training.
- Templates without sprint linkage.
- Blameless field set false.
- Post-mortem with name shaming.

---

**Fim WI-S17-004.**
