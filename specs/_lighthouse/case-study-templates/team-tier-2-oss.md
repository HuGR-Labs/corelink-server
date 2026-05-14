---
id: "CASE-STUDY-TEMPLATE-TEAM-OSS"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-004"
tags: ["lighthouse", "s20", "case-study", "template", "team-tier", "oss", "bazel", "buck2", "governance"]
---

# Case Study Template — Team Tier #2 (External OSS, Bazel/Buck2 ecosystem)

> **Slot:** `LH-OSS-01` · **Tier:** Team · **External validation of REMOTE-CACHE-PRODUCT-PROFILE**
> **Audience:** OSS community first; CoreLink marketing co-publish; public testimonial.

---

## 1. Problem statement (≤ 200 words)

_Describe the OSS project's build/CI pain pre-CoreLink. Anchor to public artifacts where possible (e.g. GitHub Actions logs, issues filed about build time, mailing-list complaints about cache invalidation)._

Suggested skeleton: "The {Project} OSS project has N contributors across M timezones. The Bazel/Buck2 build of the full tree takes T minutes on a cold cache and Y minutes on a warm cache; CI minutes consumed per merged PR averaged X..."

---

## 2. Before-state metrics

| Metric | Pre-CoreLink baseline |
|---|---|
| Build tool | _Bazel X.Y_ / _Buck2 Z.Y_ |
| Build time (cold cache) | _XX min_ |
| Build time (warm cache) | _XX min_ |
| Cache hit ratio (CI) | _XX%_ |
| Cache backend | _AWS S3 + bazel-remote-cache_ / _Buck2 cas client (self-hosted)_ / _none (cold every time)_ |
| Monthly CI minutes consumed | _XX,XXX min_ |
| Contributors active in last 90d | _N_ |

---

## 3. Integration story (≤ 400 words)

_Narrate the OSS maintainer's experience migrating to CoreLink as a managed shared cache backend. Emphasize:_

- **Configuration delta** — what changed in `WORKSPACE` / `.bazelrc` / `.buckconfig`.
- **Onboarding friction** — was the docs path sufficient for a non-employee?
- **CI integration** — how was the PAT issued + scoped + rotated; any CI provider quirks (GitHub Actions, BuildBuddy, CircleCI).
- **Community comms** — how did the maintainer communicate the change to other contributors?
- **First-week bugs** — captured publicly in GitHub issues / discussions.

This case study is **public OSS ecosystem validation** — the narrative should resonate with another OSS maintainer evaluating CoreLink for their project. Avoid corporate marketing tone.

---

## 4. Results (30d observation)

| Metric | Pre-CoreLink | Post-CoreLink (30d) | Delta |
|---|---|---|---|
| Build time (cold cache) | _XX min_ | _YY min_ | _-Z%_ |
| Build time (warm cache) | _XX min_ | _YY min_ | _-Z%_ |
| Cache hit ratio (CI) | _XX%_ | _YY%_ | _+Z pp_ |
| Monthly CI minutes consumed | _XX,XXX_ | _YY,YYY_ | _-Z%_ |
| Mean PR-to-merge wall time | _XX h_ | _YY h_ | _-Z%_ |

### 4.1 SLA evidence

| SLO | Target | OSS actual (30d) |
|---|---|---|
| SLO-AVAIL-CAS-PUT | ≥ 99.9% | _99.9X%_ |
| SLO-AVAIL-CAS-GET | ≥ 99.9% | _99.9X%_ |
| SLO-LAT-CAS-GET p99 | < 300 ms | _XXX ms_ |
| SLO-FRESH-BILLING | < 0.1% drift / 24h | _0.0X%_ |

---

## 5. Customer quote (slot)

> _"[Quote from OSS maintainer — 2-3 sentences. Authenticity > polish; let the maintainer's voice come through.]"_
>
> — _Name, Maintainer_, _{Project}_

Quote MUST be approved in writing (GitHub issue thread or email) by the maintainer. PR-style review of quote draft is acceptable.

---

## 6. Technical architecture diagram (slot)

```
[ insert Mermaid / SVG diagram here ]

OSS contributors ──► Bazel/Buck2 CLI ──► CoreLink CAS (multi-region)
                                              │
   GitHub Actions / CI ─────────────────────────┤
                                              ├─► R2 storage (encrypted at rest)
                                              ├─► Dedup + chunk store
                                              └─► Audit chain (public-good transparency)
```

OSS diagram emphasizes **multi-region** + **transparency** (open audit endpoints if applicable). No proprietary architecture details.

---

## 7. Future plans (≤ 100 words)

_OSS maintainer's roadmap with CoreLink (e.g. adoption by sister projects, contribution back to CoreLink OSS adapters, evolution of CI minute spend)._

---

## 8. Publication metadata

| Field | Value |
|---|---|
| Customer-approved | _yes / no_ |
| Legal review status | _approved (light — no NDA, public OSS)_ |
| Sanitization required | _no (public OSS)_ |
| Publication venue | _CoreLink website + project's blog/GitHub README link_ |
| Embargo | _none_ |
| Co-authors | _CoreLink Marketing + OSS maintainer co-authored_ |

---

**Template for OSS case study; ALWAYS instantiate per real customer attestation. Public OSS context allows more open architecture diagrams + public metric disclosure.**
