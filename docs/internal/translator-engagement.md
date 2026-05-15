# Translator engagement playbook — R-4 / H-16

> Scope: procure native-speaker translation for the 200 `<!-- i18n:TODO -->`
> stub pages in `apps/docs/i18n/{pt-BR,es-419}/`. Owner: docs/i18n team.
> Last revised: 2026-05-14.

The CoreLink docs i18n delivery is on the critical path to R-4 (GA i18n
parity). This playbook covers vendor selection, RFP, contract, ingest,
delivery, and acceptance.

---

## 1. Why we hire humans, not MT

We deliberately do **not** ship machine-translated docs. Reasons:

1. **Brand**: terminology drift across pages reads "AI-generated" and erodes
   trust. Stripe-quality docs need a consistent voice.
2. **Technical accuracy**: developer documentation has high density of
   product-specific terminology (PAT, BYOK, CAS, BLAKE3) that LLMs
   consistently mistranslate or over-translate.
3. **Legal**: LGPD/GDPR consent and DSR pages must be reviewable by qualified
   bilingual reviewers per the org's legal-review-process.md. MT alone fails
   that gate.
4. **Cost-benefit**: 200 pages × 250 words × 2 locales ≈ 100k words; the
   delta between MT-then-revise and from-scratch native translation is small
   relative to total project cost.

## 2. Vendor candidates

Three managed-service vendors and one freelance route, ranked by fit:

### Option A — Smartling (managed service)

- **Pros**: enterprise SOC 2 + LGPD compliance posture; in-tool CAT with
  translation memory; client-portal review workflow; native XLIFF 2.1
  ingest/export; established CoreLink-adjacent customer base (Stripe,
  Cloudflare, Shopify use them).
- **Cons**: most expensive (~$0.12-0.15/word for FIGS-class pairs); minimum
  monthly platform fee (~$1.5k/mo) — only worth it if recurring i18n.
- **Best fit**: post-GA when docs become a continuous ingest.

### Option B — Transifex (managed service)

- **Pros**: GitHub integration; XLIFF 2.1; mid-tier pricing
  (~$0.10-0.12/word); good for open-source-flavoured docs; free tier for
  validation.
- **Cons**: smaller native-speaker pool for es-419 specifically (heavier on
  es-ES); turnaround variable.
- **Best fit**: hybrid managed + freelance model.

### Option C — Smartcat (managed marketplace)

- **Pros**: marketplace model — vet individual translators; pay per project;
  no platform fee; ~$0.08-0.11/word; XLIFF 2.1 native; integrates TMX
  seeded translation memory.
- **Cons**: quality variance is translator-dependent — must vet samples.
- **Best fit**: one-shot delivery for R-4 (this engagement).

### Option D — ProZ.com freelancers (direct)

- **Pros**: cheapest (~$0.06-0.10/word per locale); direct relationship with
  named translators; you keep the TMs.
- **Cons**: project management overhead is on us; QA/redundancy not built-in;
  invoicing/contracts handled per-vendor.
- **Best fit**: backup option if managed-service quotes blow budget.

**Recommendation**: lead with **Smartcat** (Option C) for this one-shot
delivery. Fall back to Option D if quotes exceed $15k all-in.

## 3. Cost estimate

| Item | Value |
|---|---|
| Source corpus | 200 pages |
| Avg words/page | ~250 (per `export-xliff.ts` dry-run; refine before RFP) |
| Locales | 2 (pt-BR, es-419) |
| Billable word-count | ~100,000 (50k × 2 locales) |
| Rate (Smartcat) | $0.08–0.11/word |
| Rate (Smartling) | $0.12–0.15/word |
| **Smartcat midpoint** | **~$9.5k** |
| **Smartling midpoint** | **~$13.5k** |
| Project mgmt buffer (10%) | $0.9–1.4k |
| **All-in band** | **$8–15k** |

Variance drivers: actual avg words/page (run the exporter), translator tier
(senior vs. mid-level), urgency premium for D+14 ingest.

## 4. RFP template

Send to all 3 managed vendors + 5 ProZ freelancers per locale (10 total
freelancer queries; expect ~3 viable responses per locale).

```text
Subject: RFP — technical documentation translation (en-US → pt-BR + es-419)

Hi [vendor],

CoreLink (developer infrastructure, content-addressable cache on Cloudflare)
is seeking native-speaker translation for ~200 pages of developer
documentation. Details:

- Source language: en-US (US English, technical, Stripe Docs voice)
- Target languages: pt-BR (Brazilian Portuguese) + es-419 (Latin American
  Spanish, neutral)
- Volume: ~50,000 source words per locale (~100k total)
- Format: XLIFF 2.1 ingest + delivery; bundle includes TMX 1.4 translation
  memory seed and a 9-section style guide (terminology, voice, banned
  constructions, do-not-translate list)
- Subject matter: developer documentation, REST/gRPC API references,
  tutorials. Familiarity with build systems (Bazel, Buck2), caching,
  cryptographic primitives required.
- Deliverable: same XLIFF bundle structure with `<target>` filled.
- Quality gate: scripted check for terminology adherence, MDX structure
  integrity, link integrity. Failures returned for revision under
  contract SLA (max 2 revision cycles included).
- Timeline:
    - D+0:   RFP sent
    - D+7:   Vendor selected, contract signed
    - D+14:  Bundle delivered to vendor
    - D+28:  Translations returned (14-day SLA)
    - D+30:  Imported, quality-checked, accepted
- Confidentiality: standard NDA, no public attribution required.

Please reply with:
1. $/word rate for each locale.
2. Lead time once bundle delivered.
3. CAT tool and TMX/XLIFF version supported.
4. Senior translator named per locale (we want the same person across all
   pages of a locale for voice consistency).
5. Sample translation of the attached 300-word excerpt (no obligation).

Thanks,
[procurement lead]
```

## 5. Engagement timeline (D-30 to D+0 delivery)

| Day | Event | Owner |
|---|---|---|
| D-30 | RFP sent to 3 managed + 10 freelance | docs/i18n lead |
| D-28 | Vendor responses due | vendors |
| D-26 | Sample translations reviewed by internal native speakers (one per locale, found via team) | i18n + native reviewers |
| D-23 | Vendor selected, contract drafted | docs/i18n lead + legal |
| D-20 | Contract signed; PO issued (Stripe Bill or Mercury invoice) | finance |
| D-19 | Export bundle generated (`pnpm tsx scripts/export-xliff.ts`) | docs/i18n lead |
| D-19 | Bundle SHA-256 published; sent to vendor with style guide | docs/i18n lead |
| D-5  | Vendor mid-checkpoint: 20% sample returned for early QA | vendor |
| D-4  | Mid-checkpoint reviewed; corrections flagged | i18n + reviewers |
| D+0  | Full delivery returned by vendor (D+14 from start) | vendor |
| D+1  | Imported via `pnpm tsx scripts/import-xliff.ts` | docs/i18n lead |
| D+1  | Quality check: `pnpm tsx scripts/translation-quality-check.ts --strict` | CI + docs/i18n lead |
| D+2  | Findings sent back for revision (if any) | vendor |
| D+5  | Revisions returned (3-business-day SLA) | vendor |
| D+6  | Re-import + quality check → green | CI |
| D+7  | Committed to `main` via PR + i18n review checklist | docs/i18n lead |

Total budget: **30 days from RFP to merge**. Critical path: vendor selection
(7d) + translation (14d) + revision (5d) + integration (4d).

## 6. Acceptance criteria

A delivery is accepted only when **all** of the following hold:

1. `pnpm tsx scripts/translation-quality-check.ts --strict` exits 0.
2. `pnpm i18n-coverage --threshold 1.0` exits 0 (100% — zero stubs remain).
3. Internal native-speaker reviewer (one per locale) signs off in writing on
   tone, terminology, and naturalness for a random 10% sample.
4. `pnpm typecheck` and `pnpm build` succeed in `apps/docs/` (catches broken
   MDX).
5. Lighthouse + Vale + lychee gates remain green post-merge.

## 7. Contract clauses (non-negotiable)

- **Confidentiality**: NDA; CoreLink internal terminology and roadmap are
  trade secrets.
- **Work-for-hire / assignment**: all rights to translations vest in
  HumanGR-Labs; vendor cannot reuse copy for other clients.
- **Quality SLA**: max 2 free revision cycles after initial delivery; vendor
  bears cost of structural defects (broken MDX, dropped placeholders).
- **TM ownership**: HumanGR-Labs retains all translation memories; vendor
  delivers TMX exports on completion.
- **No MT clause**: vendor warrants no raw machine-translation output is
  delivered; only post-edited human translation acceptable.
- **Attribution**: no public credit; translator names do NOT appear in
  committed docs (constraint per project charter).
- **Sub-contracting**: written approval required before vendor subcontracts.

## 8. Risk register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Vendor delivers late | M | H | Mid-checkpoint at D-5; alternate vendor on standby |
| Translation quality below bar | M | H | Internal reviewer sample at D-26; sample translation upfront |
| Terminology drift across pages | M | M | TMX seed + style guide enforced; quality-check script |
| Cost overrun | L | M | Cap RFP at $15k; freelance fallback (Option D) |
| MDX breakage on import | L | H | `translate="no"` on code/frontmatter; tag-balance check in QC |
| Translator subcontracts to MT | L | H | Explicit contract clause + spot-checks vs LLM detector |

## 9. Post-delivery

- Archive the accepted XLIFF bundle + TMX in `apps/docs/dist/xliff/` (not
  committed; .gitignored). Upload to internal S3-equivalent for retention.
- Update the TMX seed in next export with the now-committed translations
  (closes the loop for future incremental updates).
- Schedule quarterly i18n refresh against new EN content.

---

**Owner**: docs/i18n team (rotation TBD). **Escalation**: VP Eng, then CTO.
