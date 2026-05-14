# The Economics of a Fast Cache Hit: How CoreLink Changes Inner-Loop Math

> **DRAFT — pending Marketing + Finance sign-off (pricing references) + Engineering sign-off (bench numbers).**
> Target length: 1,500–3,000 words. Economics deep-dive.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 (Post 5 reframed → cache hit economics) · REMOTE-CACHE-PRODUCT-PROFILE · SLO-LAT-CAS-GET.

---

If you do nothing else after reading this post, do this: open your CI dashboard, find the median time spent downloading prebuilt artifacts from your remote cache during a green build, and multiply it by your weekly green-build count, your engineer count, and a fully-loaded engineer-hour cost. That is your annual cache-tax bill. It is almost always larger than the line-item you currently allocate to your remote cache. The cache is a tax on every build, every retry, every CI job — and most teams pay it without measuring it.

CoreLink is built to change two terms in that equation: the per-hit latency, and the hit rate. This post explains how, with placeholder bench numbers (because GA-day benches are still being finalized) and a pointer to the cost-savings calculator.

## The model

The annualized cost of build-cache latency for a team can be modeled as:

```
annual_cost ≈ engineers
            × builds_per_engineer_per_day
            × jobs_per_build
            × cache_lookups_per_job
            × (hit_rate × avg_hit_latency + (1 − hit_rate) × avg_miss_latency)
            × loaded_engineer_hourly_rate
            × working_days_per_year
```

Three terms are policy choices made by the cache vendor: `avg_hit_latency`, `avg_miss_latency`, and the curve of `hit_rate` against build-graph similarity. The other terms are properties of the customer's team.

CoreLink optimizes the three vendor-side terms hard, and exposes the customer-side terms in dashboards so the customer can see, in dollars, the impact of each.

## Why latency is a vendor problem, not a customer problem

A "fast" remote cache is one where the cache-hit path goes from "the build needs blob X" to "blob X is in the local filesystem" in tens of milliseconds, not hundreds. That is, almost entirely, a product of where the cache is and how its data path is shaped. CoreLink runs on Cloudflare's global edge platform; the typical CAS GET from a CI runner in any of our four supported regions completes inside the latency budget published in the SLO catalog (`SLO-LAT-CAS-GET`).

We will publish bench numbers at GA-day. Today, in the embargoed launch documents, those numbers are placeholders pending final confirmation against the 30-day sustained staging data. They will tell the same story the staging data tells, which is: cache hits are fast, predictable, and not bottlenecked by region selection.

## Why hit rate is also (partly) a vendor problem

Many teams accept their existing hit rate as a property of their own build graph. Some of it is. But hit rate is also affected by:

- **Cache key stability.** A cache that is unstable across trivial input variation forces re-builds that should have hit.
- **Negative caching policy.** A cache that aggressively negative-caches transient failures forces re-builds for failures that should have hit on retry.
- **Eviction policy.** A cache that evicts hot artifacts under load forces re-builds for artifacts that should have stayed warm.
- **Deduplication.** A cache that fails to dedupe identical artifacts across actions wastes storage and read bandwidth, indirectly hurting hit performance.

CoreLink's REMOTE-CACHE-PRODUCT-PROFILE pins each of those policies explicitly: BLAKE3-keyed content addressing for strong dedup, eviction policy documented and content-aware, negative caching bounded and explicit, action-cache key canonicalization REAPI-conformant.

## What the calculator does

The CoreLink cost-savings calculator (at `corelink.dev/calculator`, pending GA-day publication) takes the customer-side inputs from the model above, asks for current observed hit rate and current average latency, and returns:

- Current annualized cache-tax estimate.
- Projected annualized cache-tax with CoreLink's published `SLO-LAT-CAS-GET` budgets.
- The break-even point at which CoreLink Team / Enterprise tier pricing becomes a net cost reduction.

We deliberately do not publish a single headline ROI number. The honest number depends on the customer's build profile, and the calculator is the right artifact for that conversation, not a press release pull-quote.

## Pricing

CoreLink ships with Free, Team, and Enterprise tiers. Specific list prices live at `corelink.dev/pricing`. In this and other launch documents, dollar values are denoted as `$X` placeholders pending Finance sign-off — not because pricing is undecided, but because every customer-facing dollar value will be reviewed and approved against the Finance audit trail before it appears in print.

## What CoreLink will not promise

A few things we will not promise in this post:

- We will not promise a specific percentage hit-rate improvement, because that depends on the customer's build.
- We will not promise a specific dollar savings, because that depends on the customer's team economics.
- We will not promise sub-millisecond cache-hit latency, because that is a claim no honest remote cache can make from a CI runner anywhere on Earth.

We will promise the latency budget in the SLO catalog, sustained against 30 days of staging, and we will let the customer's measurements do the rest.

## A pragmatic suggestion

If you are evaluating CoreLink, the most useful experiment to run is:

1. Pick a single CI pipeline that you know is bottlenecked on remote cache latency.
2. Configure that pipeline to use CoreLink alongside your existing cache for a two-week shadow period.
3. Compare green-build wall-clock time between the two.

That experiment will tell you, with measurement rather than marketing, whether the cache-tax line item in your annual model is one CoreLink can move.

## Where to go next

- **Calculator:** `corelink.dev/calculator`
- **Pricing:** `corelink.dev/pricing`
- **SLO catalog:** `docs.corelink.dev/slo`
- **Product profile (remote cache semantics):** `docs.corelink.dev/architecture/remote-cache-product-profile`

— Product at CoreLink

---

## Internal notes (strip before publish)

- Word count: ~1,200. Slightly short of the 1,500 floor — expand once GA-day bench numbers land. Currently within DRAFT acceptable range pending bench confirmation.
- Pending review: Finance (sign-off slot 13) + Engineer (sign-off slot 6) + Product (slot 8).
- All bench numbers held as placeholders pending 30d sustained staging data per WI-S20-007.
- No specific dollar amounts — all pricing references use $X placeholders per WI-S20-008 constraints.
