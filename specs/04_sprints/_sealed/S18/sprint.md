---
id: "S-18"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "LOW_RISK"
# lane_forcing_factors omitted: LOW_RISK lane permite empty (REG-LANE-003 só obriga se lane=HIGH_RISK; schema minItems:1 rejeita empty array)
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "COMPLIANCE-MATRIX"
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "AUTH-MODEL"
tags: ["sprint", "s18", "docs", "docusaurus", "diataxis", "wcag-2.2-aa", "pricing", "security-page", "compliance-page", "i18n", "low-risk"]
---

# Sprint S-18 — Public Docs + API Reference + Pricing + Security Page (Docusaurus 3.x + Diátaxis Taxonomy + Algolia DocSearch + Custom Domain `docs.corelink.humangr.com` + i18n 3 Locales en-US/pt-BR/es-419 Native Speaker Reviewed; REAPI v2 Reference Auto-gen via `protoc-gen-doc` Zero Drift CI Gate + Manual Code Examples 4 Languages Rust/Python/Go/JS; SDK Guides Python pyO3 + Go cgo + JS/TS WASM + CLI per Command Reference; Compliance + Security + Pricing Pages com Cross-functional Gate Mandatory Finance/Legal/Privacy Officer/Security Lead Sign-off + SBOM Downloadable S-12 CycloneDX 1.5+ + Cosign Verify + SLSA L3 Rekor Lookup Instructions + Pentest Exec Summary Public-safe + 5 Tiers + Pricing Calculator Validated 10 Scenarios + Finance Sign-off; Lighthouse ≥ 95 em 5 Routes + WCAG 2.2 AA axe-core 0 Violations + Vale Tone Lint CI + lychee Broken-link Weekly + UX Research 5 Dev Sample Finds Answer ≤ 30s + PRR LOW_RISK 3 Sign-offs Canonical + Cross-functional Gate Publish Separate

> **doc_status:** DRAFT · **lane:** LOW_RISK · **Versão:** 1.0.0 · **2026-04-29**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (LOW_RISK lane; public docs sprint sem tenant data flow path; cross-functional review gate canonical)

> **Phase boundary:** Fase 6 — GTM enablement + enterprise procurement readiness pré-GA (S-20 hard dependency: docs live em `docs.corelink.humangr.com` + 5-dev UX research success ≤ 5 min getting started + Lighthouse ≥ 95 sustained + cross-functional review sign-offs Finance/Legal/Privacy/Security para pricing/compliance/security pages).
> **LOW_RISK lane rationale (zero forcing factors):** docs sprint não toca tenant data flow path (consumes existing REAPI v2 specs from S-01..S-04 + AUTH-MODEL PAT format from S-03 + SECURITY-MODEL SBOM from S-12 + COMPLIANCE-MATRIX statements + PRIVACY-MODEL notice from S-16). Não introduz cripto-load-bearing controles novos. Não introduz novo path tenant data flow. Não há regulatory residency exposure direct (compliance page LINKS canonical sources). Auto-gen REAPI reference reuses single source of truth (`.proto` files from S-04). Cross-functional review gate (Finance/Legal/Privacy/Security) é process discipline (não cripto-load-bearing). Privacy Officer review garante zero PII em screenshots/examples (CTRL-PRIV-001 reuse). Anti-scope estrito previne pricing/security claims sem cross-functional review (per spec contract §10).

> **Single-phase SEAL D+10 rationale:** S-18 DoD §6 não requer 30d+ observation criteria — todos os critérios são instant-verifiable at sprint close (Lighthouse score em 5 routes; WCAG axe-core 0 violations; Vale lint green; lychee broken-link green; cross-functional sign-offs coletados; UX research 5 devs em ≤ 30s; pricing calculator 10 scenarios validated). 2-week sprint LOW_RISK sem post-sprint observation window required. Single-phase SEAL canônico LOW_RISK (vs STANDARD two-phase quando 30d sustained needed; HIGH_RISK two-phase sempre).

---

## 1. Objetivo

Publicar **docs públicas production-grade** em `docs.corelink.humangr.com` com **Diátaxis taxonomy discipline** (tutorial / how-to / reference / explanation): getting started 5-min quickstart Bazel/Buck2/Native CLI com starter project links + measured via UX research session 5 dev sample (sample finds answer ≤ 30s per Quality Standard 14.s18.2 + completes getting started ≤ 5 min per spec contract §6); REAPI v2 reference **auto-gerada** de `.proto` files via `protoc-gen-doc` (zero drift entre proto + rendered docs CI gate per spec contract §9.8) + manual code examples per endpoint em 4 languages (Rust + Python + Go + JS); SDK guides per-language production-ready (Python `corelink-py` install + first cache hit + advanced BYOK + DSR; Go `corelink-go` análogo; JS/TS `@corelink/client` análogo; CLI `corelink` reference command per command); compliance + security + pricing pages com **cross-functional review gate canonical mandatory** (Finance + Legal sign-off pricing + feature matrix + calculator validated 10 scenarios; Legal + Privacy Officer sign-off compliance LGPD/GDPR/CCPA statements + DPA template link + sub-processors list + privacy notice link; Security lead + Privacy Officer sign-off security page SBOM downloadable S-12 CycloneDX 1.5+ + Cosign verify instructions + SLSA L3 attestation lookup via Rekor + pentest exec summary public-safe + SLA + SOC 2 timeline gap analysis status); i18n 3 locales canonical en-US (default) + pt-BR (LGPD primary) + es-419 (matching S-11 + S-15 + S-16 alignment per Lote 10.16) com native speaker review per locale; WCAG 2.2 AA axe-core CI 0 violations + Lighthouse ≥ 95 em 5 routes (`/`, `/docs/getting-started`, `/docs/sdk/python`, `/security`, `/pricing`); Vale tone consistency lint CI gate + lychee broken-link CI gate (PR fail se broken-link; weekly external link verification cadence). Sin docs production-grade, GTM bloqueado + enterprise procurement (SBOM access + SOC 2 timeline + pentest summary required) impossível + dev adoption baixa (developer experience 1st impression).

Decomposição em 5 WIs: (1) **WI-S18-001** Docusaurus 3.x foundation em `apps/docs/` + Cloudflare Pages deploy custom domain `docs.corelink.humangr.com` + Diátaxis taxonomy sidebar (tutorial / how-to / reference / explanation) + Algolia DocSearch integration + i18n 3 locales config en-US/pt-BR/es-419 + versioning (latest + 1 prior major) + edit on GitHub link per page; (2) **WI-S18-002** Getting started 5-min quickstart Bazel/Buck2/Native CLI + REAPI v2 reference auto-gen via `protoc-gen-doc` (CI gate zero drift between `.proto` + rendered docs) + manual code examples per endpoint em 4 languages (Rust + Python + Go + JS); (3) **WI-S18-003** SDK guides 4 deliverables per spec contract §5.3 (Python pyO3 `corelink-py` + Go cgo `corelink-go` + JS/TS WASM `@corelink/client` + CLI `corelink` reference command per command) + client verify default-on per CTRL-CAS-002 documented; (4) **WI-S18-004** Compliance + security + pricing pages **cross-functional review gate** (Finance + Legal pricing 5 tiers + feature matrix + calculator 10 scenarios; Legal + Privacy Officer compliance LGPD/GDPR/CCPA statements + DPA + sub-processors + privacy notice; Security lead + Privacy Officer security SBOM downloadable + Cosign verify + SLSA L3 Rekor lookup + pentest exec summary + SLA + SOC 2 timeline) + zero customer data em screenshots fixture pipeline; (5) **WI-S18-005** Closing WI ship gate i18n native speaker review en-US/pt-BR/es-419 + WCAG 2.2 AA axe-core CI 0 violations + Lighthouse ≥ 95 em 5 routes + Vale lint CI gate + lychee broken-link CI gate + UX research session 5 dev sample finds answer ≤ 30s + PRR LOW_RISK 3 sign-offs canonical + cross-functional gate publish separate. Implementa **CAP-DOCS-001..008** + reforça **CTRL-PRIV-001** (zero PII em screenshots/examples; redaction allowlist enforced) + introduz **CTRL-DOC-AUTO-GEN** (REAPI reference auto-gen drift prevention CI gate). Não introduz novas INVs (sprint consumer; per spec contract §8 mantidas only).

**Por que SOTA:** competitors têm docs fragmentadas (NativeLink minimal getting started, BuildBuddy product-marketing-heavy sem Diátaxis discipline), missing pricing transparency (Bazel Remote Cache zero pricing), sem SBOM access para enterprise procurement (BuildBuddy email request com NDA only), com auto-gen API reference drift risk (manual maintenance). CoreLink S-18 entrega: (a) Diátaxis-organized navigability discoverability test 5 dev sample finds answer ≤ 30s (Stripe Docs + Linear API Docs parity); (b) auto-gen REAPI reference via `protoc-gen-doc` + CI gate zero drift entre `.proto` + rendered docs (drift impossible by construction; Linear API Docs auto-gen excellence parity); (c) Lighthouse ≥ 95 em 5 routes + WCAG 2.2 AA via axe-core 0 violations (Stripe Docs parity; vantagem WCAG 2.2 vs 2.1 baseline competitors); (d) Vale tone consistency lint CI gate (Stripe + Linear parity); (e) lychee broken-link CI gate weekly external link verification (Stripe + Linear parity); (f) SBOM downloadable + Cosign verify + SLSA L3 attestation via Rekor lookup public + pentest exec summary public-safe (rare; vantagem competitivo strict — competitors limit SBOM access NDA-gated email request only; CoreLink openly downloadable); (g) cross-functional review gate canonical mandatory pricing/security/compliance pages (Finance + Legal + Privacy Officer + Security lead) — process discipline strict não skippable. Reference: **Stripe Docs** (gold standard; ~9/10 dimensions parity), **Diátaxis Framework** <https://diataxis.fr/>, **Vale linter** <https://vale.sh/>, **lychee broken-link checker** <https://github.com/lycheeverse/lychee>, **Docusaurus v3** <https://docusaurus.io/>, **Linear API Docs** (auto-gen excellence), **GOV.UK** (WCAG 2.2 + i18n discipline), **The Good Docs Project** templates, **OpenAPI Specification 3.1**, **protoc-gen-doc** <https://github.com/pseudomuto/protoc-gen-doc>.

## 2. Escopo

### 2.1 In-scope

- **WI-S18-001**: Docusaurus 3.x foundation em `apps/docs/` deployed CF Pages custom domain `docs.corelink.humangr.com` + SSL (per spec contract §6 EVT-018); Diátaxis taxonomy sidebar (Tutorials learning-oriented "Build your first cached project in 5 min" + How-to task-oriented "How to configure BYOK", "How to integrate Bazel CI" + Reference information-oriented REAPI v2 + CLI + SDK reference + Explanation understanding-oriented "Why content-addressable cache?", "How dedup works" per spec contract §5.1); Algolia DocSearch integration; versioning latest + 1 prior major (Docusaurus native); edit on GitHub link per page; i18n config 3 locales en-US (default) + pt-BR (LGPD primary) + es-419 (matching S-11 + S-15 + S-16 alignment); PR review checks Diátaxis taxonomy fit per Quality Standard 14.s18.2.

- **WI-S18-002**: Getting started 5-min quickstart 3 build systems (Bazel + Buck2 + Native CLI per spec contract §5.1 R-S18-1); REAPI v2 reference **auto-gen via `protoc-gen-doc`** de `.proto` files (gRPC services + messages; REST endpoints Worker handlers); CI gate **zero drift between `.proto` + rendered docs** (CTRL-DOC-AUTO-GEN canonical per Quality Standard 14.s18.8 — auto-gen drift prevention); manual code examples per endpoint em 4 languages (Rust + Python + Go + JS per spec contract §5.2 R-S18-4); PAT format examples use placeholder `corelink_dev_t_xxx.xxx.xxx` format (S-03 decision (a) PAT format `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` — never confused; never real PAT em examples).

- **WI-S18-003**: SDK guides per-language production-ready 4 deliverables per spec contract §5.3 R-S18-5: **Python** `corelink-py` install + first cache hit + advanced (BYOK + DSR); **Go** `corelink-go` análogo; **JS/TS** `@corelink/client` análogo; **CLI** `corelink` reference command per command (7 subcommands ls/get/put/stat/bench/doctor/version + `--output=json` flag); client verify default-on per CTRL-CAS-002 documented (R-S18-6); per-language idiomatic API patterns (Python async/await; Go context-based; JS Promise-based); zero customer data em SDK examples (fixture pipeline; CTRL-PRIV-001 enforced).

- **WI-S18-004**: **Cross-functional review gate canonical mandatory** páginas sensíveis (per spec contract §9.6 + §10 anti-scope estrito):
  - **`/pricing`** com 5 tiers (Free 10 GB / Starter 100 GB / Team 1 TB / Pro 10 TB / Enterprise unlimited per spec contract §5.5 R-S18-9) + feature matrix + pricing calculator usage-based input GB/mo + tier → estimated cost validated 10 sample scenarios (per spec contract §7 Completeness Criteria 10.s18.5 EVT-044) + **Finance + Legal sign-off mandatory** antes publish (R-S18-11; non-skippable per Waiver policy §19).
  - **`/security`** com SBOM downloadable link release SBOM (S-12 CycloneDX 1.5+) + verification instructions Cosign verify + SLSA L3 attestation lookup via Rekor query + pentest exec summary public-safe (high-level findings; no specific CVE before disclosure per spec contract §15 row 8) + SLA published terms + SOC 2 timeline gap analysis status (S-20 deliverable) + **Security lead + Privacy Officer sign-off mandatory** antes publish.
  - **`/compliance`** com LGPD compliance statement + GDPR compliance statement (lawful basis per processing) + CCPA/CPRA compliance + DPA template link + sub-processors list link (S-16 auto-generated reuse) + privacy notice link (S-16 reuse) + **Legal + Privacy Officer sign-off mandatory** antes publish.
  - Zero customer data real em screenshots/examples per Quality Standard 14.s18.9 (fixture pipeline ensures no real PII; test fixtures only; CI lint check via grep for known PII patterns).

- **WI-S18-005**: Closing WI ship gate cumulative — i18n native speaker review per locale en-US + pt-BR + es-419 (Legal local review for legal terms in pt-BR/es-419 per spec contract §15 row 6); WCAG 2.2 AA axe-core CI gate 0 violations sustained per Quality Standard 14.s18.4 + manual screen reader test (per spec contract §6 EVT-018 a11y); Lighthouse CI gate ≥ 95 (Performance + A11y + Best Practices + SEO) em 5 routes (`/`, `/docs/getting-started`, `/docs/sdk/python`, `/security`, `/pricing` per spec contract §5.6 R-S18-13 + Quality Standard 14.s18.1); Vale tone consistency lint CI gate em `apps/docs/.vale/` style guide per Quality Standard 14.s18.3; lychee broken-link CI gate PR fail se broken-link + weekly external link verification cadence per Quality Standard 14.s18.4; UX research session 5 dev sample finds answer ≤ 30s per Quality Standard 14.s18.2 + completes getting started ≤ 5 min per spec contract §6 (EVT-018); PRR LOW_RISK 3 sign-offs canonical (Owner + Final Approver + Docs lead) + cross-functional publish gate separate (Finance pricing + Legal terms + Privacy Officer compliance + Security lead security per spec contract §6) — esses sign-offs **count toward separate publish gate** (não main PRR per spec contract §10 anti-scope; non-skippable).

### 2.2 Anti-scope

- ❌ Video tutorials (backlog pós-GA; manuscript-only at GA per spec contract §10).
- ❌ Enterprise-specific docs com confidential business logic — S-19 customer onboarding owns enterprise material; `docs.corelink.humangr.com` é GA-public-only.
- ❌ Marketing landing page (separate `corelink.humangr.com` site; S-20 marketing prep).
- ❌ Blog (pós-GA Q1).
- ❌ Customer case studies pre-GA (waiting for lighthouse customers — S-20).
- ❌ **Pricing/security claims sem cross-functional review** — anti-scope estrito (per spec contract §10 + §19 Waiver policy non-skippable; qualquer page tocando pricing requires Finance + Legal review; security/compliance requires Privacy Officer + Security lead; sem review = não merge).
- ❌ ChatGPT-style chat support widget (pós-GA).
- ❌ Real-time pricing API (pricing changes via PR + cross-functional review only).
- ❌ Customer environments docs (chaos-as-a-service style; pós-GA enterprise).
- ❌ AI-powered docs search além Algolia DocSearch (pós-GA Q1+).

## 3. Customer Impact & Journey

**JTBD:** "Como External Developer / Build Engineer em prospect, preciso confiança production-grade pré-onboarding: (a) getting started 5-min quickstart Bazel/Buck2/Native (≤ 5 min time-to-first-cache-hit measured; ≤ 30s find answer Diátaxis taxonomy discipline); (b) SDK guide my-language idiomatic Python/Go/JS/CLI; (c) REAPI v2 reference fresh from `.proto` (drift impossible by construction; auto-gen CI gate). Como Procurement / Compliance auditor (enterprise prospect), preciso evidence pack: SBOM downloadable (S-12 CycloneDX 1.5+) + Cosign verify + SLSA L3 attestation lookup via Rekor + pentest exec summary public-safe + SLA + SOC 2 timeline gap analysis status (S-20 deliverable) + LGPD/GDPR/CCPA statements + DPA template + sub-processors + privacy notice. Como Finance / Legal counsel, preciso pricing transparency (5 tiers + feature matrix + calculator validated 10 scenarios) + Finance + Legal sign-off before publish (cross-functional gate canonical non-skippable). Como Developer Advocate / Customer Success, preciso Diátaxis taxonomy discipline (every page categorized tutorial/how-to/reference/explanation) + i18n 3 locales en-US/pt-BR/es-419 native speaker reviewed + WCAG 2.2 AA + Lighthouse ≥ 95 sustained = developer experience 1st impression accountability."

**CAPs entregues:** CAP-DOCS-001 (getting started 5-min quickstart) + CAP-DOCS-002 (REAPI v2 reference auto-gen) + CAP-DOCS-003 (SDK guides Python/Go/JS/CLI) + CAP-DOCS-004 (compliance & security page) + CAP-DOCS-005 (pricing page) + CAP-DOCS-006 (Diátaxis taxonomy) + CAP-DOCS-007 (i18n + a11y) + CAP-DOCS-008 (CI lint + broken-link check).

**Persona 1 — External Developer / Build Engineer (prospect)**:
- Getting started 5-min quickstart Bazel/Buck2/Native; UX research 5 dev sample finds answer ≤ 30s + completes getting started ≤ 5 min.
- Diátaxis taxonomy discoverability: tutorial learning-oriented + how-to task-oriented + reference information-oriented + explanation understanding-oriented.
- REAPI v2 reference fresh from `.proto` files (auto-gen drift impossible by construction).
- Diferenciador competitivo vs NativeLink/BuildBuddy: Diátaxis discipline + auto-gen reference + i18n 3 locales native speaker reviewed + WCAG 2.2 AA + Lighthouse ≥ 95.

**Persona 2 — Procurement / Compliance auditor (enterprise prospect)**:
- SBOM downloadable (S-12 CycloneDX 1.5+) + Cosign verify instructions + SLSA L3 attestation lookup via Rekor query (rare em competitors; vantagem strict).
- Pentest exec summary public-safe (high-level findings; no specific CVE before disclosure).
- SOC 2 timeline gap analysis status (S-20 deliverable; transparent communication).
- LGPD + GDPR (lawful basis per processing) + CCPA/CPRA compliance statements aligned canonical sources (não contradizer privacy_model.md ou compliance_matrix.md).
- DPA template link + sub-processors list link (S-16 auto-generated reuse) + privacy notice link.

**Persona 3 — Finance / Legal counsel**:
- Pricing transparency 5 tiers (Free/Starter/Team/Pro/Enterprise) + feature matrix + calculator validated 10 scenarios.
- Pricing changes via PR + cross-functional review only (not real-time API; intentional).
- Cross-functional review gate canonical: Finance + Legal sign-off pricing; Legal + Privacy Officer compliance; Security lead + Privacy Officer security — non-skippable per Waiver policy §19.

**Persona 4 — Developer Advocate / Customer Success**:
- i18n 3 locales en-US (default) + pt-BR (LGPD primary) + es-419 (matching S-11 + S-15 + S-16 alignment); native speaker review + Legal local review for legal terms.
- WCAG 2.2 AA axe-core CI 0 violations sustained + manual screen reader test (a11y regulatory baseline).
- Lighthouse ≥ 95 em 5 routes (UX baseline for dev tools); CI gate < 95 fails PR; perf review weekly.
- Vale tone consistency lint CI + lychee broken-link CI weekly external verification.

**SLA addendum**:
- Time-to-first-cache-hit getting started: ≤ 5 min (measured via UX research 5 dev sample; tracked monthly).
- Diátaxis discoverability: 5 dev sample finds answer ≤ 30s.
- Pricing calculator: 10 sample scenarios validated + Finance sign-off.
- Lighthouse score: ≥ 95 sustained 30d (per Completeness Criteria 10.s18.7).
- WCAG 2.2 AA: 0 axe-core violations sustained.
- i18n: en-US (default) + pt-BR (LGPD primary) + es-419 native speaker reviewed + Legal local review.
- Cross-functional review gate publish: non-skippable Finance/Legal/Privacy/Security per relevant page.

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Foundation: `remote_cache_product_profile.md` (REAPI v2 API semantics; auto-gen REAPI reference) + `compliance_matrix.md` (SOC 2 timeline + LGPD/GDPR/CCPA statements + supply chain claims) + `privacy_model.md` (privacy notice link + sub-processors list S-16 auto-generated reuse) + `security_model.md` (SBOM access S-12 CycloneDX 1.5+ + pentest exec summary public-safe + Cosign verify + SLSA L3 Rekor lookup) + `auth_model.md` (PAT auth flow para API examples; PAT format `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` per S-03 decision (a) — placeholder `corelink_dev_t_xxx.xxx.xxx` em examples; never real PAT).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S18-D1 | Docusaurus 3.x foundation + Diátaxis taxonomy + Algolia DocSearch + i18n 3 locales + custom domain | `apps/docs/` + CF Pages `docs.corelink.humangr.com` | Docusaurus deployed; Diátaxis sidebar 4 categories; Algolia DocSearch integrated; i18n config 3 locales; custom domain SSL; edit on GitHub per page |
| S18-D2 | Getting started 5-min quickstart + REAPI v2 reference auto-gen + 4-language code examples | `apps/docs/docs/getting-started/` + `apps/docs/docs/reference/api/` | Quickstart Bazel/Buck2/Native; REAPI auto-gen via `protoc-gen-doc` zero drift CI gate; manual code examples 4 languages |
| S18-D3 | SDK guides Python pyO3 + Go cgo + JS/TS WASM + CLI per command reference | `apps/docs/docs/sdk/` × 4 | 4 SDK guides production-ready; client verify default-on documented; per-language idiomatic API; zero customer data em examples |
| S18-D4 | Compliance + security + pricing pages com cross-functional gate | `apps/docs/docs/compliance.md` + `apps/docs/docs/security.md` + `apps/docs/docs/pricing.md` | Finance+Legal sign-off pricing; Legal+Privacy Officer sign-off compliance; Security lead+Privacy Officer sign-off security; SBOM downloadable; pricing calculator 10 scenarios |
| S18-D5 | Closing WI ship gate i18n native speaker + WCAG 2.2 AA + Lighthouse ≥ 95 + Vale + lychee + UX research + PRR LOW_RISK | `apps/docs/i18n/<locale>/` + `apps/docs/.vale/` + `.github/workflows/docs-ci.yml` + `specs/04_sprints/_sealed/S18/PRR-S18.md` | i18n native speaker en/pt-BR/es-419; WCAG 2.2 AA axe-core 0 violations; Lighthouse ≥ 95 5 routes; Vale CI green; lychee CI green; UX research 5 devs ≤ 30s; PRR 3 sign-offs + cross-functional publish gate |

## 6. Sign-off (LOW_RISK 3 canonical + cross-functional publish gate)

3 roles canonical para LOW_RISK lane (per framework §33.5.4 + spec contract §6 sign-off line):

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Docs lead | _TBD; emphatic — Diátaxis discipline + Vale tone consistency + i18n native speaker + WCAG 2.2 AA + Lighthouse ≥ 95_ | _pending_ | _pending_ |

**Cross-functional review gate canonical (separate publish gate; não main PRR; per spec contract §9.6 + §10 anti-scope estrito; non-skippable per Waiver policy §19)**:

| # | Page | Required cross-functional sign-off | Status |
|---|---|---|---|
| CF-1 | `/pricing` | **Finance** (5 tiers + calculator validated 10 scenarios + EVT-044) + **Legal** (terms + DPA reference) | _pending_ |
| CF-2 | `/security` | **Security lead** (SBOM access + pentest exec summary public-safe + Cosign verify + SLSA L3 Rekor instructions) + **Privacy Officer** (CTRL-PRIV-001 enforced + zero PII em examples) | _pending_ |
| CF-3 | `/compliance` | **Legal** (LGPD + GDPR + CCPA statements aligned canonical compliance_matrix.md) + **Privacy Officer** (privacy notice + sub-processors S-16 auto-generated reuse) | _pending_ |

**Lane rationale (LOW_RISK vs STANDARD 5-8 / HIGH_RISK 11)**:
- Public docs sprint não toca tenant data flow path; consume existing REAPI v2 + AUTH-MODEL + SECURITY-MODEL + COMPLIANCE-MATRIX + PRIVACY-MODEL canonical sources.
- Não introduz cripto-load-bearing controles novos.
- Não há regulatory residency exposure direct (compliance page LINKS canonical sources — não introduz claims novos).
- Auto-gen REAPI reference reuses single source of truth (`.proto` files from S-04).
- Cross-functional review gate (Finance/Legal/Privacy/Security) é process discipline para pages relevant non-skippable mas separate publish gate (não main PRR per spec contract §10 anti-scope).
- 3 sign-offs canonical + cross-functional gate publish separate cobre risco regulatory (compliance/pricing/security pages cannot be published sem cross-functional review).

> Engineer + QA + Product + Compliance officer + Privacy officer canonical em STANDARD lane são **NÃO mandatory canonical em LOW_RISK** (folded em PR review se applicable; Docs lead canonical em S-18 dado scope Diátaxis discipline + Vale lint + i18n native speaker + WCAG 2.2 AA + Lighthouse). Cross-functional publish gate captures Finance/Legal/Privacy/Security needs em separate gate.

## 7. Definition of Done (lane LOW_RISK; **single-phase SEAL D+10**)

> **Single-phase SEAL D+10 rationale**: LOW_RISK lane base + DoD §6 não requer 30d+ observation criteria. Todos os critérios são instant-verifiable at sprint close (Lighthouse score em 5 routes; WCAG axe-core 0 violations; Vale lint green; lychee broken-link green; cross-functional sign-offs coletados; UX research 5 devs em ≤ 30s; pricing calculator 10 scenarios validated). 2-week sprint LOW_RISK sem post-sprint observation window required.

### 7.1 Implementation SEAL D+10 (Sprint Review ceremony)

- [ ] **WIs SEALED**: 5/5 (EVT-031).
- [ ] **Docs URL live** + SSL CF Pages custom domain `docs.corelink.humangr.com` (EVT-018).
- [ ] **Docusaurus 3.x** deployed + Diátaxis taxonomy sidebar 4 categories + Algolia DocSearch + versioning + edit on GitHub.
- [ ] **Getting started 5-min quickstart** Bazel/Buck2/Native; UX research 5 dev sample completes ≤ 5 min (EVT-018).
- [ ] **Diátaxis discoverability** test: 5 dev sample finds answer ≤ 30s (per Completeness Criteria 10.s18.3 EVT-018).
- [ ] **REAPI v2 reference auto-gen** via `protoc-gen-doc` working + CI gate zero drift between `.proto` + rendered docs (per Completeness Criteria 10.s18.6).
- [ ] **Manual code examples** 4 languages (Rust + Python + Go + JS) per endpoint.
- [ ] **SDK guides** 4 deliverables (Python pyO3 + Go cgo + JS/TS WASM + CLI per command reference); client verify default-on documented per CTRL-CAS-002.
- [ ] **Pricing page** 5 tiers + feature matrix + pricing calculator validated 10 sample scenarios (EVT-044) + Finance sign-off (cross-functional gate CF-1).
- [ ] **Compliance page** LGPD + GDPR + CCPA statements + DPA template link + sub-processors list link + privacy notice link + Legal + Privacy Officer sign-off (cross-functional gate CF-3).
- [ ] **Security page** SBOM downloadable + Cosign verify instructions + SLSA L3 attestation lookup via Rekor + pentest exec summary public-safe + SLA + SOC 2 timeline + Security lead + Privacy Officer sign-off (cross-functional gate CF-2).
- [ ] **SBOM downloadable** + verification instructions tested (per Completeness Criteria 10.s18.4 EVT-010).
- [ ] **Lighthouse score** ≥ 95 todos pillars em 5 routes `/`, `/docs/getting-started`, `/docs/sdk/python`, `/security`, `/pricing` (EVT-002).
- [ ] **WCAG 2.2 AA** axe-core CI 0 violations + manual screen reader test (EVT-018 a11y).
- [ ] **Vale lint** tone consistency CI gate verde (EVT-002).
- [ ] **lychee broken-link** CI gate verde + weekly external link verification scheduled (EVT-002).
- [ ] **i18n 3 locales** native speaker reviewed + Legal local review for legal terms (en-US + pt-BR + es-419 EVT-018).
- [ ] **Zero customer data** em screenshots/examples + CI lint check via grep for known PII patterns (CTRL-PRIV-001 enforced).
- [ ] **PRR LOW_RISK**: 3 sign-offs canonical (Owner + Final Approver + Docs lead) + cross-functional publish gate separate (Finance + Legal + Privacy Officer + Security lead per relevant page).
- [ ] **Não introduz INVs novas** (sprint consumer; per spec contract §8 mantidas only).
- [ ] **Cost regression gate**: docs infra ≤ $50/mês (CF Pages free tier + Algolia DocSearch free tier + Lighthouse CI free + axe-core OSS + Vale OSS + lychee OSS).

### 7.2 Completeness Criteria (delta local; per spec contract §7)

- [ ] **10.s18.1** Zero broken links em CI (lychee) (EVT-002).
- [ ] **10.s18.2** i18n: en-US + pt-BR + es-419 (basic) native speaker reviewed (EVT-018).
- [ ] **10.s18.3** Diátaxis taxonomy discoverability test: 5 dev sample finds answer ≤ 30s (EVT-018).
- [ ] **10.s18.4** SBOM downloadable + verification instructions tested (EVT-010).
- [ ] **10.s18.5** Pricing calculator validated 10 scenarios + Finance sign-off (EVT-044).
- [ ] **10.s18.6** REAPI auto-gen drift zero entre `.proto` + rendered docs (CI gate).
- [ ] **10.s18.7** Lighthouse ≥ 95 sustained (instant verify at sprint close; tracking monthly post-sprint).
- [ ] **10.s18.8** WCAG 2.2 AA zero violations sustained (instant verify; tracking monthly post-sprint).

## 8. Dependencies

### Hard blockers

- **S-15 SEALED** (CLI + SDK existem para docs reference; CTRL-CAS-002 client verify default-on; PAT format).

### Soft blockers

- **S-12 SEALED** (SBOM published S-12 CycloneDX 1.5+ — referenced em security page; Cosign verify; SLSA L3 attestation Rekor).
- **S-16 SEALED** (privacy notice + sub-processors list auto-generated; docs link reuse).

### Outbound

- S-19 (enterprise customer onboarding may reuse public docs sections).
- S-20 (GA exige docs live em `docs.corelink.humangr.com` + 5-dev UX research passed + Lighthouse ≥ 95 sustained + cross-functional publish gate sign-offs).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-15 SEALED hard blocker; ideally após S-12 + S-16 SEALED).
- **D+3**: WI-S18-001 SEALED (Docusaurus + Diátaxis + i18n base + custom domain).
- **D+6**: WI-S18-002 SEALED (REAPI auto-gen + 4-language examples) + WI-S18-003 SEALED (SDK guides 4 deliverables).
- **D+8**: WI-S18-004 SEALED (compliance + security + pricing pages + cross-functional gate sign-offs).
- **D+10**: WI-S18-005 SEALED (i18n native speaker + WCAG 2.2 AA + Lighthouse + Vale + lychee + UX research + PRR LOW_RISK).
- **D+10**: **Single-phase SEAL ceremony** (Sprint Review; PRR LOW_RISK 3 sign-offs + cross-functional publish gate; sprint review).
- **Total**: 2 semanas (10 dias úteis) + buffer 2 dias (per spec contract §13).

## 10. Risk Register

Ver `_spec_contract.md §15` (8 riscos: docs stale vs realidade do código mitigado por auto-gen; pricing mudança post-launch confunde customers mitigado por versioned pricing pages + changelog + email broadcast 30d; compliance claim out of sync com canonical source mitigado por cross-functional review gate; pricing leak pre-GA NDA breach mitigado por Finance approve + GA-day deploy gated; Lighthouse score regression mitigado por CI gate < 95 fails PR; translation quality issues legal terms pt-BR/es-419 mitigado por Legal local review; SBOM disclosure inadvertent mitigado por Security lead review antes publish; pentest exec summary leaks attack details mitigado por Security lead review high-level only).

## 11. Observability Plan

DASH-DOCS-MATURITY (novo dashboard; opcional baseline):
- Docs deployment status (CF Pages last deploy ts; alert se > 7d sem deploy).
- Lighthouse score per route (5 routes; alert se < 95 sustained 7d).
- WCAG 2.2 AA axe-core violations (alert se > 0).
- Vale lint CI status (PR fails se Vale violations).
- lychee broken-link CI status (PR fails se broken-link).
- Auto-gen REAPI reference drift (CI gate; alert se drift detected).

**Métricas docs telemetry opt-in**:
- Não introduz métricas Prometheus per-tenant em docs sprint (docs telemetry separate; bounce rate / time-on-page via Algolia DocSearch analytics opcional opt-in se applicable; LINDDUN review for any analytics).
- Per observability_model §3.1 Prometheus snake_case (no docs métricas applicable; sprint consumer).

## 12. Security & Privacy

**STRIDE delta** (vs S-13/S-14/S-15/S-16/S-17 baseline):
- **Spoofing**: Algolia DocSearch auth via API key in build-time; CF Pages SSL (custom domain `docs.corelink.humangr.com` Let's Encrypt automated).
- **Tampering**: docs source git-tracked + reviewer Docs lead; PR review checks Diátaxis taxonomy fit; auto-gen REAPI reference fresh from `.proto` (no manual maintenance) — drift impossible by construction.
- **Repudiation**: docs published via CF Pages deploy log + git commit history + edit on GitHub link per page; cross-functional sign-offs documented em PRR.
- **Information disclosure**: zero customer data real em screenshots/examples (fixture pipeline; CTRL-PRIV-001 enforced; CI lint check via grep for known PII patterns); SBOM scrubbed of internal-only deps (review por Security lead antes publish per spec contract §15 row 7); pentest exec summary high-level only (no specific CVE before disclosure per row 8).
- **DoS**: CF Pages free tier rate-limited; Algolia DocSearch free tier query limited.
- **Elevation of privilege**: docs publish requires PR review + Docs lead sign-off + cross-functional gate per relevant page (Finance + Legal + Privacy Officer + Security lead per page).

**LINDDUN delta**:
- **Linkability**: docs telemetry opt-in if applicable (data anonymized: page view + referrer + browser; nunca user-identifiable; LINDDUN review for any analytics).
- **Identifiability**: zero customer data em screenshots/examples (fixture pipeline); PAT examples placeholder format `corelink_dev_t_xxx.xxx.xxx` (S-03 decision (a) — never real PAT).
- **Non-repudiation**: docs git-tracked + cross-functional sign-offs documented em PRR.
- **Detectability**: lychee broken-link CI weekly + auto-gen drift detection CI gate.
- **Disclosure**: SBOM Security lead review; pentest exec summary high-level only; compliance claims aligned canonical sources (não contradizer privacy_model.md ou compliance_matrix.md).
- **Unawareness**: cross-functional review gate canonical (Finance + Legal + Privacy Officer + Security lead) per page sensible non-skippable.
- **Non-compliance**: LGPD + GDPR (lawful basis per processing) + CCPA/CPRA statements aligned canonical compliance_matrix.md; LINDDUN review committed em `specs/_audits/2026-XX-XX-linddun-public-docs.md` se applicable.

**Controls**:
- **CTRL-PRIV-001** (zero PII em logs/screenshots/examples) — IMPLEMENTA primary reflection em fixture pipeline + CI lint check.
- **CTRL-DOC-AUTO-GEN** (auto-gen drift prevention CI gate) — INTRODUZ em S-18 cumulative; REAPI reference auto-gen via `protoc-gen-doc` from `.proto` files; CI gate zero drift between `.proto` + rendered docs.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc** (per `_spec_contract.md §18`):

- Pricing claim out-of-sync com Stripe billing → CRITICAL post-mortem (legal + customer trust).
- Compliance claim incorrect → 5-Why + Legal review + immediate fix.
- Broken-link em prod (post-deploy missed) → 5-Why + lychee CI strengthen.
- Lighthouse score < 90 sustained > 7d → post-mortem (DX regression).
- Cross-functional review bypassed em pricing/compliance/security PR → post-mortem + process reinforce (CRITICAL; non-skippable gate violated).
- Customer feedback: "I couldn't find X in docs" recurrent → discoverability post-mortem + IA review.
- Auto-gen REAPI reference drift detected em prod → post-mortem + CI gate reinforce.
- Customer data leak em screenshots/examples → CRITICAL post-mortem + CTRL-PRIV-001 reinforce.

## 14. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-18 (cycle 12.S18.0; spec contract v1.1.0 LOW_RISK lane base; Public Docs + API Reference + Pricing + Security Page Docusaurus 3.x + Diátaxis + WCAG 2.2 AA + i18n 3 locales + cross-functional review gate canonical Finance/Legal/Privacy Officer/Security lead; 5 WIs; single-phase SEAL D+10). |

---

**Fim de S-18 sprint contract.**
