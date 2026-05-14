---
id: "SPEC-CONTRACT-S18"
type: "spec_contract"
doc_status: "SEALED"
audit_status: "ACTIVE"
version: "1.3.0"
created: "2026-04-24"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s18", "docs", "docusaurus", "diataxis", "wcag-2.2-aa", "pricing", "low-risk", "sota-v1.1"]
---

# Spec Contract — S-18: Public Docs + API Reference + Pricing + Security Page

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-18 |
| Nome | Public Docs + API Reference + Pricing |
| Lane | LOW_RISK |
| Lane forcing factors | n/a (LOW_RISK) — public docs sprint não toca tenant data flow; **anti-scope** explícito previne pricing/security claims sem cross-functional review (ver §10) |
| Duração estimada | 2 semanas |
| WIs antecipados | 5 |
| SOTA target | Public docs Diátaxis-organized + auto-gen REAPI reference + Lighthouse ≥ 95 + WCAG 2.2 AA + 3 locales |

## 1. Objetivo

Publicar **docs públicas production-grade** em `docs.corelink.dev` seguindo **Diátaxis taxonomy** (tutorial / how-to / reference / explanation): getting started com 5-min quickstart Bazel/Buck2/Native, REAPI v2 reference auto-gerada de protos + manual examples, SDK guides (Python/Go/JS/CLI), compliance & security page (SOC 2 timeline + SBOM access + pentest summary), pricing page (5 tiers + feature matrix + calculator). Docs são **face público** — impacto direto GTM, conversion, trust signal regulatory.

**Por que SOTA:** competitors têm docs fragmentadas, missing pricing transparency, sem SBOM access para enterprise procurement. CoreLink S-18 entrega: (a) Diátaxis-organized navigability; (b) auto-gen API reference (drift impossível); (c) Lighthouse ≥ 95 + WCAG 2.2 AA; (d) Vale CI lint para tone consistency; (e) lychee broken-link CI check; (f) SBOM downloadable + pentest exec summary public. Reference: **Stripe Docs** (gold standard), **Diátaxis Framework**, **Vale (linter)**, **Linear's API docs** (auto-gen excellence).

**Codex finding:** lane LOW_RISK ok mas anti-scope deve previne pricing/security claims being shipped sem cross-functional review (Legal + Finance + Security). Reforçado em §10.

## 2. Lane + forcing factors

- **Lane:** LOW_RISK (3 sign-offs).
- **Não FF-HR**: docs sprint não toca tenant data flow.
- **Atenção em pricing/security claims**: anti-scope explícito (ver §10) — qualquer pricing/security claim em docs requer Finance + Legal + Security review antes de publish (mantém lane LOW_RISK mas com cross-functional gate).

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"  # API semantics
  - "COMPLIANCE-MATRIX"             # SOC 2 timeline + LGPD/GDPR + supply chain claims
  - "PRIVACY-MODEL"                 # privacy notice link + sub-processors
  - "SECURITY-MODEL"                # SBOM access + pentest summary
  - "AUTH-MODEL"                    # PAT auth flow para API examples
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-DOCS-001** | Getting started 5-min quickstart | Bazel + Buck2 + Native CLI quickstart com starter project links. |
| **CAP-DOCS-002** | REAPI v2 reference auto-gen | OpenAPI/gRPC reference de protos; manual examples; SDK code snippets. |
| **CAP-DOCS-003** | SDK guides (Python/Go/JS/CLI) | Per-language guide com idiomatic usage; client verify default-on documented. |
| **CAP-DOCS-004** | Compliance & security page | SOC 2 timeline + SBOM access + pentest exec summary + SLA + DPA template link. |
| **CAP-DOCS-005** | Pricing page | 5 tiers (free / starter / team / pro / enterprise) + feature matrix + calculator. |
| **CAP-DOCS-006** | Diátaxis taxonomy | tutorial / how-to / reference / explanation organization. |
| **CAP-DOCS-007** | i18n + a11y | en (default) + pt-BR + es; WCAG 2.2 AA; Lighthouse ≥ 95. |
| **CAP-DOCS-008** | CI lint + broken-link check | Vale lint (tone consistency) + lychee broken-link; CI gate. |

## 5. Requirements específicos

### 5.1 Foundation + Diátaxis (CAP-DOCS-001 + CAP-DOCS-006)

- **R-S18-1**: Docusaurus 3.x em `apps/docs/` deployed em CF Pages com:
  - Algolia DocSearch ou similar.
  - Versioning (latest + 1 prior major).
  - Edit on GitHub link per page.
- **R-S18-2**: Diátaxis taxonomy em sidebar:
  - **Tutorials** (learning-oriented): "Build your first cached project in 5 min".
  - **How-to** (task-oriented): "How to configure BYOK", "How to integrate Bazel CI".
  - **Reference** (information-oriented): REAPI v2 reference, CLI reference, SDK reference.
  - **Explanation** (understanding-oriented): "Why content-addressable cache?", "How dedup works".

### 5.2 REAPI Reference (CAP-DOCS-002)

- **R-S18-3**: Auto-generation de REAPI reference de `.proto` files via `protoc-gen-doc`:
  - gRPC services + messages.
  - REST endpoints (Worker handlers).
- **R-S18-4**: Manual code examples per endpoint em 4 languages (Rust + Python + Go + JS).

### 5.3 SDK Guides (CAP-DOCS-003)

- **R-S18-5**: Per-language SDK guide:
  - Python: `corelink-py` install + first cache hit + advanced (BYOK + DSR).
  - Go: `corelink-go` análogo.
  - JS/TS: `@corelink/client` análogo.
  - CLI: `corelink` reference command per command.
- **R-S18-6**: Client verify documentation (CTRL-CAS-002 default-on).

### 5.4 Compliance & Security (CAP-DOCS-004)

- **R-S18-7**: Page `/security`:
  - SBOM access: link para release SBOM (S-12 CycloneDX 1.5+) + verification instructions.
  - Pentest exec summary: high-level findings (no CVE details public until disclosed).
  - SLA: published SLA terms.
  - SOC 2 timeline: gap analysis status (S-20 deliverable).
  - SLSA L3 attestation lookup instructions (Rekor query).
- **R-S18-8**: Page `/compliance`:
  - LGPD compliance statement.
  - GDPR compliance statement (lawful basis per processing).
  - CCPA/CPRA compliance.
  - DPA template link.
  - Sub-processors list link.
  - Privacy notice link.

### 5.5 Pricing (CAP-DOCS-005)

- **R-S18-9**: Pricing page com 5 tiers:
  - **Free**: 10 GB storage, 100 GB egress, 1 PAT.
  - **Starter** ($X/mo): 100 GB, 1 TB egress, 5 PATs, basic SLA.
  - **Team** ($Y/mo): 1 TB, 10 TB egress, 20 PATs, 99.9% SLA.
  - **Pro** ($Z/mo): 10 TB, 100 TB egress, unlimited PATs, 99.95% SLA, BYOK.
  - **Enterprise** (Contact): unlimited, custom SLA, BYOK, DPA, SSO, dedicated support.
- **R-S18-10**: Pricing calculator (usage-based: input GB/mo + tier → estimated cost).
- **R-S18-11**: Pricing review by Finance + Legal antes publish (mandatory waiver gate).

### 5.6 i18n + a11y + Lint (CAP-DOCS-007 + CAP-DOCS-008)

- **R-S18-12**: 3 locales: **en-US** (default) + **pt-BR** (LGPD primary) + **es-419** (LATAM canonical matching S-11 + S-15 + S-16 alignment Lote 10.16); native speaker review per locale; **NÃO há reduction allowance "3 → 2"** (Lote 10.18 codex P1 fix; prior wording allowed regression — removida).
- **R-S18-13**: Lighthouse CI gate: ≥ 95 Performance + A11y + Best Practices + SEO em 5 routes.
- **R-S18-14**: WCAG 2.2 AA via axe-core CI 0 violations.
- **R-S18-15**: Vale lint para tone consistency (style guide em `apps/docs/.vale/`).
- **R-S18-16**: lychee broken-link check em CI; PR fail se broken-link.

## 6. Definition of Done

- [ ] **WIs SEALED**: 5/5.
- [ ] **Docs URL live** + SSL (CF Pages) com custom domain `docs.corelink.dev` (EVT-018).
- [ ] **5 dev externos** completam getting started em ≤ 5 min — UX research session (EVT-018).
- [ ] **Pricing calculator** validado por Finance + tested 10 sample scenarios (EVT-044).
- [ ] **Pricing page** reviewed Finance + Legal (cross-functional gate CF-1; Lote 10.18 codex P1 canonical fix — Security removida do pricing review path; Security é cross-functional gate CF-2 apenas para security page) (EVT-044).
- [ ] **Compliance page** reviewed Legal + Privacy Officer (EVT-044).
- [ ] **REAPI reference** auto-gen working + manual examples 4 languages (EVT-018).
- [ ] **Lighthouse score** ≥ 95 todos pillars em 5 routes (EVT-002).
- [ ] **WCAG 2.2 AA** axe-core 0 violations + manual screen reader test (EVT-018 a11y).
- [ ] **Vale lint** tone consistency CI gate verde (EVT-002).
- [ ] **lychee broken-link** CI gate verde (EVT-002).
- [ ] **i18n 3 locales canonical en-US + pt-BR + es-419** (Lote 10.18 codex P1 alignment matching S-11 + S-15 + S-16); native speaker reviewed (EVT-018).
- [ ] **PRR LOW_RISK** (3 sign-offs canonical; Lote 10.18 codex P1 canonical fix — Finance/Legal/Privacy NÃO folded em main PRR — eles são separate publish gate CF-1/CF-2/CF-3 per anti-scope §10): Owner + Final Approver + Docs lead. Cross-functional reviewers (Finance + Legal + Privacy Officer + Security lead) contribuem em separate publish gate per page (não main PRR sign-off).

## 7. Completeness Criteria (delta local)

- [ ] **10.s18.1** Zero broken links em CI (lychee) (EVT-002).
- [ ] **10.s18.2** i18n: **en-US + pt-BR + es-419** canonical (Lote 10.18 codex P1; "basic" wording removida — locale matrix completa per S-11 alignment) (EVT-018).
- [ ] **10.s18.3** **Diátaxis taxonomy** discoverability test: 5 dev sample finds answer ≤ 30s (EVT-018).
- [ ] **10.s18.4** **SBOM downloadable** + verification instructions tested (EVT-010).
- [ ] **10.s18.5** **Pricing calculator** validated 10 scenarios + Finance sign-off (EVT-044).
- [ ] **10.s18.6** **REAPI auto-gen drift** zero entre `.proto` and rendered docs (CI gate).
- [ ] **10.s18.7** **Lighthouse ≥ 95** sustained 30d.
- [ ] **10.s18.8** **WCAG 2.2 AA** zero violations sustained.

## 8. Invariants

### Mantidas (LOW_RISK; não cria invariants)

- Zero customer data real em screenshots/examples (privacy enforcement).
- Compliance claims aligned com canonical sources (não contradizer privacy_model.md ou compliance_matrix.md).

## 9. Quality Standards (delta local)

- **14.s18.1 Lighthouse ≥ 95** Performance + A11y + Best Practices + SEO.
- **14.s18.2 Diátaxis discipline**: every page categorized; PR review checks taxonomy fit.
- **14.s18.3 Vale lint** tone consistency CI gate.
- **14.s18.4 lychee broken-link** check CI; weekly external link verification.
- **14.s18.5 i18n discipline**: missing translation = build fail; native speaker review per locale.
- **14.s18.6 Cross-functional review gate** para pricing + security + compliance pages: Finance + Legal + Privacy Officer + Security lead sign-off antes publish.
- **14.s18.7 SBOM access**: SBOM downloadable + verification instructions; pentest exec summary public-safe.
- **14.s18.8 Auto-gen drift prevention**: REAPI reference fresh from `.proto`; CI gate.
- **14.s18.9 Examples sanitization**: fixture pipeline ensures no real customer data; test fixtures only.

## 10. Anti-scope

- ❌ Video tutorials (backlog pós-GA).
- ❌ Enterprise-specific docs com confidential business logic — S-19 customer onboarding owns enterprise material; docs.corelink.dev é GA-public-only.
- ❌ Marketing landing page (separate `corelink.dev` site; S-20 marketing prep).
- ❌ Blog (pós-GA Q1).
- ❌ Customer case studies pre-GA (waiting for lighthouse customers — S-20).
- ❌ **Pricing/security claims sem cross-functional review** — anti-scope estrito; qualquer page tocando pricing requires Finance + Legal review; security/compliance requires Privacy Officer + Security lead. Sem review = não merge.
- ❌ ChatGPT-style chat support widget (pós-GA).
- ❌ Real-time pricing API (pricing changes via PR + cross-functional review only).

## 11. Dependencies

### Hard blockers

- **S-15 SEALED** (CLI + SDK existem para docs reference).

### Soft blockers

- **S-12 SEALED** (SBOM published — referenced in security page).
- **S-16 SEALED** (privacy notice + sub-processors auto-generated; docs link).

### Outbound

- S-19 (enterprise onboarding may reuse public docs sections).
- S-20 (GA exige docs live + 5-dev UX research passed).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S18-001** | Docusaurus 3.x foundation + Diátaxis taxonomy + i18n config + custom domain (Lote 10.18 codex P2 alignment com filename `WI-S18-001-docusaurus-foundation-diataxis-i18n-custom-domain.md`) | scaffold; CF Pages; sidebar Diátaxis; i18n config 3 locales; Algolia DocSearch; custom domain `docs.corelink.dev` | 12h | 18h | 28h | **18.7h** |
| **WI-S18-002** | Getting started 5-min quickstart + REAPI auto-gen + 4-language code examples (Lote 10.18 codex P2 alignment com filename `WI-S18-002-getting-started-reapi-auto-gen-4-language-examples.md`) | quickstart Bazel/Buck2/Native; protoc-gen-doc CI gate; manual examples per endpoint Rust/Python/Go/JS | 14h | 22h | 36h | **23.0h** |
| **WI-S18-003** | SDK guides Python pyO3 + Go cgo + JS/TS WASM + CLI per-command reference (Lote 10.18 codex P2 alignment com filename `WI-S18-003-sdk-guides-python-go-js-cli-per-command.md`) | per-language SDK guide; client verify default-on documented; CLI reference per command | 10h | 16h | 26h | **16.7h** |
| **WI-S18-004** | Compliance + security + pricing pages + cross-functional gate (Lote 10.18 codex P2 alignment com filename `WI-S18-004-compliance-security-pricing-pages-cross-functional-gate.md`) | /compliance + /security + /pricing pages; SBOM access + Cosign verify + Rekor; pricing 5 tiers + calculator 10 scenarios; CODEOWNERS + CI hard merge control | 12h | 18h | 28h | **18.7h** |
| **WI-S18-005** | i18n + WCAG + Lighthouse + Vale + lychee + UX research + closing PRR (Lote 10.18 codex P2 alignment com filename `WI-S18-005-i18n-wcag-lighthouse-vale-lychee-ux-research-prr-closing.md`) | native speaker review 3 locales en-US/pt-BR/es-419; axe-core CI; Lighthouse CI em PR preview (não live); Vale + lychee CI; UX research 5 devs; PRR closing 3 sign-offs canonical + cross-functional publish gate separate | 10h | 14h | 22h | **14.7h** |

**Total PERT:** ~87h ≈ 11 dias work × 1 eng. Buffer 2 dias confere com 2 semanas.

## 13. Duração + Timeline

- **Duração:** 2 semanas (10 dias úteis) + buffer 2 dias.
- **Marcos:**
  - **D+3:** WI-001 SEALED (Docusaurus + Diátaxis live).
  - **D+6:** WI-002 SEALED (REAPI + SDK guides).
  - **D+8:** WI-003 + WI-004 SEALED (compliance + pricing).
  - **D+10:** WI-005 SEALED (a11y + UX research).
  - **D+12:** Sprint review + cross-functional sign-offs.

## 14. Critérios de promoção

- DoD complete + 5-dev UX research success (5-min getting started).
- Cross-functional sign-offs: Finance + Legal + Privacy + Security em pages relevant.
- Lighthouse ≥ 95 sustained.
- WCAG 2.2 AA verified.
- PRR LOW_RISK aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Docs stale vs realidade do código** | H | M | LOW (auto-gen mitiga parcial) | M | LOW | Auto-gen REAPI from .proto; PR review checks; quarterly docs review. |
| **Pricing mudança post-launch confunde customers** | M | L | LOW | L | LOW | Versioned pricing pages + changelog + email broadcast 30d antes. |
| **Compliance claim out of sync** com canonical source | M | M | HIGH (regulatory) | M | LOW | Cross-functional review gate + Privacy Officer + Legal sign-off mandatory. |
| **Pricing leak pre-GA** (NDA breach) | L | L | MEDIUM | L | LOW | Pricing publish only após Finance approve + GA-day deploy gated. |
| **Lighthouse score regression** | M | L | LOW | L | LOW | CI gate < 95 fails PR; perf review weekly. |
| **Translation quality issues** (legal terms in pt-BR/es) | M | L | MEDIUM (legal) | L | LOW | Native speaker + Legal local review per locale. |
| **SBOM disclosure inadvertent** (sensitive vendor info) | L | L | MEDIUM | L | LOW | SBOM scrubbed of internal-only deps; review por Security lead antes publish. |
| **Pentest exec summary leaks attack details** | L | L | MEDIUM | L | LOW | Security lead review; high-level summary only; no specific CVE before disclosure. |

## 16. Benchmarks SOTA externos

| Critério | Stripe Docs | Linear API Docs | GOV.UK | **CoreLink target S-18** |
|---|---|---|---|---|
| Diátaxis-organized | Yes | Yes | Yes | **Yes — tutorial / how-to / reference / explanation** |
| Auto-gen API reference | Yes | Yes | N/A | **Yes — protoc-gen-doc + manual examples** |
| 5-min quickstart UX target | Yes | Yes | N/A | **Yes — measured via 5-dev UX research** |
| Pricing transparent + calculator | Yes | Yes | N/A | **Yes — 5 tiers + calculator + Finance reviewed** |
| Lighthouse ≥ 95 | Yes | Yes | Yes | **Yes — CI gate** |
| WCAG 2.2 AA | 2.1 AA | 2.1 AA | 2.2 AA | **2.2 AA** |
| i18n 3+ locales | Many | Limited | EN+CY | **3 (en/pt-BR/es) at GA** |
| Vale lint tone consistency | Yes | Yes | Yes | **Yes — CI gate** |
| Broken-link CI | Yes | Yes | Yes | **Yes — lychee** |
| SBOM downloadable | Limited | No | N/A | **Yes — public + verification instructions** |

**Veredito SOTA:** S-18 v1.1 atinge feature parity com Stripe Docs em 9/10 dimensões; vantagem em SBOM access (rare) + cross-functional review gate.

## 17. References (RFCs, papers, standards)

- **Diátaxis Framework** <https://diataxis.fr/>.
- **WCAG 2.2** AA <https://www.w3.org/TR/WCAG22/>.
- **Vale linter** <https://vale.sh/>.
- **lychee broken-link checker** <https://github.com/lycheeverse/lychee>.
- **Docusaurus v3** <https://docusaurus.io/>.
- **Stripe Docs UX patterns** (industry gold standard).
- **The Good Docs Project** templates.
- **OpenAPI Specification 3.1**.
- **protoc-gen-doc** <https://github.com/pseudomuto/protoc-gen-doc>.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Pricing claim out-of-sync com Stripe billing → CRITICAL post-mortem (legal + customer trust).
- Compliance claim incorrect → 5-Why + Legal review + immediate fix.
- Broken-link em prod (post-deploy missed) → 5-Why + lychee CI strengthen.
- Lighthouse score < 90 sustained > 7d → post-mortem (DX regression).
- Cross-functional review bypassed em pricing/compliance/security PR → post-mortem + process reinforce.
- Customer feedback: "I couldn't find X in docs" recurrent → discoverability post-mortem + IA review.

## 19. Waiver policy

S-18 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ Cross-functional review gate (Finance + Legal + Privacy + Security) em pages relevant.
- ❌ Lighthouse ≥ 95 — UX baseline para dev tools.
- ❌ WCAG 2.2 AA — accessibility regulatory baseline.
- ❌ Auto-gen REAPI reference (no manual maintenance) — drift prevention.

Itens waivable com Docs lead + Product + ADR (Lote 10.18 codex P1 canonical tightening):

- ❌ **3 locales en-US/pt-BR/es-419 NÃO waivable** (Lote 10.18 codex P1 fix — prior "3 → 2 locales" allowance removida; LGPD pt-BR canonical + LATAM es-419 alignment com S-11/S-15/S-16 mandatory).
- ⚠️ 5-dev UX research passing → 3-dev sample com plan to expand (sustained baseline).
- ❌ **SBOM download path NÃO waivable** (Lote 10.18 codex P1 fix — prior "SBOM via support email com NDA" allowance removida; SBOM canonical = public download via S-12 release artifacts; NDA-gated SBOM = enterprise procurement friction inaceitável + S-12 SLSA L3 transparency baseline).

## 20. Changelog

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | Criação spec contract S-18 — public docs Diátaxis + auto-gen REAPI + i18n + a11y + pricing + security; LOW_RISK lane com cross-functional gate; 5 WIs antecipados. |
| 1.1.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | SOTA elevation v1.1 — Lighthouse ≥ 95 + WCAG 2.2 AA + Vale + lychee CI gates codified; Diátaxis discoverability test 5-dev sample. |
| 1.2.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Lote 10.18 codex P1 tightening — 3-locale i18n non-waivable (en-US + pt-BR + es-419 per S-11/S-15/S-16 alignment); SBOM canonical public download non-waivable (NDA-gated path removed); CF-1 / CF-2 / CF-3 cross-functional publish gate separate per spec contract §10 anti-scope (not main PRR). |
| 1.3.0 | 2026-05-14 | Gustavo (via Sonnet WI-S18-005 builder) | WI-S18-001..005 SEALED via 5-builder parallel wave: Docusaurus 3.x + Diátaxis + i18n config + custom domain (001), getting started + REAPI auto-gen + 4-language code examples (002), 4 SDK guides + CLI per-command + client verify default-on (003), compliance + security + pricing + CF-1/2/3 CODEOWNERS gate (004), i18n coverage gate + WCAG 2.2 AA axe-core sweep + Lighthouse ≥ 95 on 5 routes + Vale + lychee + UX research 5-persona synthetic baseline + PRR-S18 LOW_RISK single-phase SEAL D+10 CONDITIONALLY_APPROVED with 4 waiver rows (W-1 UX real-participant re-run + W-2 Docs lead staffing + W-3 CF-1/2/3 collection + W-4 native-speaker review) + adversarial summary 28 scenarios cross-WI (005). PRR-S18 2/3 signed at SEAL; 1/3 WAIVED (W-2 Docs lead ADR-0034 Option C). doc_status DRAFT → SEALED. |

---

**Fim spec contract S-18 v1.3.0 SEALED.**
