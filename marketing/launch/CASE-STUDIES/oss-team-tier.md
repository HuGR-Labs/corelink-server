# Case Study — [OSS_PROJECT_NAME]: Bazel/Buck2 Ecosystem Lighthouse

> **DRAFT — pending external OSS lighthouse engagement confirmation. NOT FOR PUBLICATION until customer-approved.**
> Trace: spec contract S-20 §5.2 R-S20-4 · WI-S20-004 lighthouse customer attestations (2 team tier + 1 enterprise BYOK) · WI-S20-008 §2.1.3 (case study #2) · CAP-GA-004.
> Status: **DRAFT — pending OSS lighthouse engagement (candidates: rules_rust maintainers, buf, tilt-dev, or equivalent Bazel/Buck2 ecosystem project per REMOTE-CACHE-PRODUCT-PROFILE).**

---

## Customer

**[OSS_PROJECT_NAME]** — open-source project in the Bazel / Buck2 / RBE ecosystem with **[OSS_CONTRIBUTOR_COUNT]** contributors and **[OSS_BUILD_VOLUME]** in monthly CI build volume across **[OSS_LANGUAGES]**.

> Candidate lighthouse projects under engagement (final selection TBD):
>
> - `rules_rust` maintainers — Rust rules for Bazel; high cache-sensitivity workload.
> - `buf` — Protobuf ecosystem; Buck2 / Bazel hybrid usage.
> - `tilt-dev` — Kubernetes dev tooling; multi-language Bazel build.
> - Alternative candidate slot — TBD via Customer Success outreach per WI-S20-004.

## The problem

Open-source projects in the build-tools ecosystem face a structural disadvantage in remote-cache adoption:

- **Self-hosted is operationally infeasible** for a volunteer-maintained project without dedicated infrastructure ownership.
- **Single-tenant SaaS** caches are typically priced or scoped against commercial workloads; CI volumes from a large OSS project can be either over-served or expensive in those plans.
- **Trust posture matters**: an OSS project's contributors are not employees, and the cache vendor's security and residency story matters for contributor jurisdictions that vary widely.

## Why CoreLink

[OSS_PROJECT_NAME] selected CoreLink for the lighthouse OSS engagement based on three factors:

1. **REAPI conformance** meant no protocol divergence from their existing Bazel / Buck2 configuration.
2. **Tenant isolation as a TLA+ invariant** addressed a procurement concern — even though OSS data is typically less sensitive, the project's contributor base spans jurisdictions where structural isolation matters.
3. **Audit chain transparency** appealed to the project's existing supply-chain posture; many of the OSS projects in this ecosystem maintain strong SBOM / provenance hygiene and CoreLink's audit chain integrates naturally.

## The migration

Migration was scoped as a configuration change to the project's CI runner pool. **[OSS_PROJECT_NAME]** ran a shadow window of **[SHADOW_DURATION]** to compare green-build wall-clock against the prior cache. No code changes were required.

## 30-day observation window

> Placeholder pending engagement; metrics structure mirrors Forge case study.

- SLA claim met every day across `SLO-LAT-CAS-GET` and `SLO-AVAIL-CAS-GET`.
- **[INCIDENT_SUMMARY]**.
- Cache hit rate delta vs. baseline: **[HIT_RATE_DELTA]**.
- Median wall-clock build time delta: **[BUILD_TIME_DELTA]**.

## OSS lighthouse testimonial

> "We migrated from a self-hosted Bazel remote cache. CoreLink absorbed the operational burden — eviction, GC correctness, blob deduplication, region-aware residency — without changing the REAPI contract our CI already spoke. The migration was effectively a config change."
>
> — **[OSS_PROJECT_LEAD_NAME]**, Maintainer, [OSS_PROJECT_NAME]
> _Quote DRAFT — pending customer engagement confirmation per WI-S20-004._

## Future plans

[OSS_PROJECT_NAME] plans to **[FUTURE_PLAN_PLACEHOLDER]**.

---

## Internal notes (strip before publish)

- Engagement target window: per WI-S20-004 timeline (D+10..D+25).
- Customer Success co-leads engagement per spec contract §5.1 R-S20-4.
- Legal review required pre-publication per WI-S20-005.
- All metrics are placeholders. Status DRAFT pending engagement confirmation + attestation.
- No specific dollar amounts. No unverified compliance claims.
