<!-- DRAFT — pending Legal + Marketing + CEO sign-off. Do not publish. -->

# The Economics of a Fast Cache Hit: How CoreLink Changes Inner-Loop Math

> **DRAFT — pending Marketing + Finance sign-off (pricing references) + Engineering sign-off (bench numbers).**
> Economics deep-dive.
> Trace: spec contract S-20 §5.2 R-S20-8 · WI-S20-008 §2.1.2 (Post 5) · REMOTE-CACHE-PRODUCT-PROFILE · SLO-LAT-CAS-GET.

---

If you do nothing else after reading this post, do this: open your CI dashboard, find the median time spent downloading prebuilt artifacts from your remote cache during a green build, and multiply it by your weekly green-build count, your engineer count, and a fully-loaded engineer-hour cost. That is your annual cache-tax bill. It is almost always larger than the line-item you currently allocate to your remote cache.

The cache is a tax on every build, every retry, every CI job — and most teams pay it without measuring it. CoreLink is built to change two terms in that equation: the per-hit latency, and the hit rate. The economics underneath the latency-and-hit-rate conversation are equally interesting, and largely under-discussed, so this post walks the full picture: what a remote cache actually costs to run, where the dollars go, where CoreLink's pricing model differs from the obvious alternatives, and what the calculator at `corelink.humangr.com/calculator` does.

## The economics of remote build cache

A remote build cache has three cost axes that matter.

**Storage.** Cache blobs accumulate, get evicted, and (under content-addressable storage) deduplicate against each other. Cost per GB stored per month is the headline number, but the operationally relevant number is `cost-per-GB × steady-state-working-set`, which is a function of the customer's build graph and the cache's eviction policy.

**Egress.** Every cache-hit read pulls bytes from the cache to a CI runner. Cache-hits are the desirable case, which means egress scales with success rather than failure. If the cache is hosted on a cloud whose egress pricing is non-trivial — most are — egress is typically the single largest line item by a wide margin.

**Operations.** Self-hosting a cache has its own line item that does not show up on the AWS invoice: engineer time to operate it. Tuning eviction. Diagnosing GC correctness bugs. Building the dashboards. Responding to the page when the cache fills up at 3am. Reconciling blob sprawl between regions. The operational tax is real, persistent, and often the dominant cost for teams that started with "just spin up a self-hosted cache" and ended up with a part-time SRE job.

A managed remote cache replaces operations with a vendor relationship and trades egress for the vendor's pricing model. The interesting question is what the vendor's pricing model is.

## Zero-egress pricing on Cloudflare R2

CoreLink is built on Cloudflare's global edge platform. Storage lives on R2, Cloudflare's S3-compatible object store. R2's pricing model is — and this is the relevant detail for cache economics — **zero egress for cache reads to Cloudflare's network and to the public internet**. Customers pay for stored capacity and for operations against the store; they do not pay for the bytes flowing out on cache hits.

The economic shape of this is hard to overstate. On a cache hosted in S3 with a 70% hit ratio and a multi-terabyte working set, egress can dominate the bill so heavily that the storage line item is rounding error. On CoreLink, that line item is zero. Customers pay for the cache they keep, not for the cache they use.

This is not a CoreLink-only property — it is a property of Cloudflare R2's pricing model, and any cache vendor building on R2 inherits it. The reason it matters here is that the rest of the cache market is anchored to a different baseline (cloud-egress-heavy hyperscaler stores), and customers comparing CoreLink against incumbents will see the difference primarily as a unit-economics gap rather than as a feature.

## Cache hit ratio: a model

A useful, simplified annualized model:

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

### Why latency is a vendor problem

A "fast" remote cache is one where the cache-hit path goes from "the build needs blob X" to "blob X is in the local filesystem" in tens of milliseconds, not hundreds. That is, almost entirely, a product of where the cache is and how its data path is shaped. CoreLink runs on Cloudflare's global edge platform; the typical CAS GET from a CI runner in any of our four supported regions completes inside the latency budget published in the SLO catalog (`SLO-LAT-CAS-GET`).

We will publish bench numbers at GA-day. Today, in the embargoed launch documents, those numbers are placeholders pending final confirmation against the 30-day sustained staging data. They will tell the same story the staging data tells, which is: cache hits are fast, predictable, and not bottlenecked by region selection.

### Why hit rate is also (partly) a vendor problem

Many teams accept their existing hit rate as a property of their own build graph. Some of it is. But hit rate is also affected by:

- **Cache key stability.** A cache that is unstable across trivial input variation forces re-builds that should have hit. CoreLink's BLAKE3-keyed content addressing is byte-stable; action-cache keys are REAPI-canonical and deterministic.
- **Negative caching policy.** A cache that aggressively negative-caches transient failures forces re-builds for failures that should have hit on retry. CoreLink's negative caching is bounded and explicit.
- **Eviction policy.** A cache that evicts hot artifacts under load forces re-builds for artifacts that should have stayed warm. CoreLink's eviction is content-aware and tunable per tenant within the published bounds.
- **Deduplication.** A cache that fails to dedupe identical artifacts across actions wastes storage and read bandwidth, indirectly hurting hit performance. Content addressing makes dedup a structural property.

REMOTE-CACHE-PRODUCT-PROFILE pins each of those policies explicitly. The hit-rate curve a customer measures against CoreLink is, by construction, not bottlenecked by the policy choices that erode hit rate at other caches.

### A worked example (placeholder)

`DRAFT — bench numbers pending CAP-GA-007 staging confirmation.`

Consider a representative customer: an engineering team of around 200 developers, running a Bazel monorepo, producing a non-trivial volume of green builds per day. With the customer's measured hit rate stable in the high range and per-hit latency at the CoreLink SLO budget, the inner-loop time saved annually relative to a self-hosted baseline maps to engineer-hours that any team can price into their loaded-cost model. The calculator at `corelink.humangr.com/calculator` runs this arithmetic for the customer's actual inputs, not for a published average.

## Cost comparison vs. self-hosted

The honest comparison is: against a properly-engineered self-hosted Bazel remote cache, hosted on S3 with appropriate bandwidth provisioning and an SRE on-call rotation. That comparison has three components:

**Storage.** Roughly comparable in raw object-store pricing. CoreLink amortizes across multi-tenant storage but does not pass that amortization through as a discount line item; the customer pays for what the customer keeps.

**Bandwidth.** This is where the gap is largest. A self-hosted cache on S3 pays cloud egress on every cache hit. CoreLink (on R2) does not. Over a year of heavy CI traffic, this is the line item that most often surprises teams when they finally measure it.

**Operations.** A self-hosted remote cache, run properly, is a part-time job for at least one engineer and a full-time job for one during bad weeks. CoreLink replaces that with a vendor relationship and an SLO. Whether that trade is good depends on the customer's engineering economics, but the trade exists and is worth pricing.

We do not publish a headline ROI number, because the honest number depends on the customer's build profile. The calculator is the right artifact for that conversation, not a press release pull-quote.

## What the calculator does

The CoreLink cost-savings calculator (at `corelink.humangr.com/calculator`, pending GA-day publication) takes the customer-side inputs from the model above, asks for current observed hit rate and current average latency, and returns:

- Current annualized cache-tax estimate.
- Projected annualized cache-tax with CoreLink's published `SLO-LAT-CAS-GET` budgets.
- The break-even point at which CoreLink Team / Enterprise tier pricing becomes a net cost reduction.

The calculator's outputs are scoped to the customer's inputs. A team with strong hit rate and modest CI volume will see a smaller absolute number than a team with weak hit rate and heavy CI volume; both are legitimate, and both are worth measuring.

## Pricing

CoreLink ships with Free, Team, and Enterprise tiers. Specific list prices live at `corelink-docs.humangr.com/pricing`. In this and other launch documents, dollar values are denoted as `$X` placeholders pending Finance sign-off — not because pricing is undecided, but because every customer-facing dollar value will be reviewed and approved against the Finance audit trail before it appears in print.

## What CoreLink will not promise

A few things we will not promise:

- We will not promise a specific percentage hit-rate improvement, because that depends on the customer's build.
- We will not promise a specific dollar savings, because that depends on the customer's team economics.
- We will not promise sub-millisecond cache-hit latency, because that is a claim no honest remote cache can make from a CI runner anywhere on Earth.

We will promise the latency budget in the SLO catalog, sustained against 30 days of staging, and we will let the customer's measurements do the rest.

## Honest caveats

A few honest caveats about cache-hit economics, so this post is not just the optimistic side of the ledger.

**Cold start.** A fresh repository, or a fresh CI environment, will not have a populated cache and will pay miss latency on every lookup until the working set warms. The hit-rate curve we publish is the steady-state curve, and customers running short-lived ephemeral CI environments will see a curve that ramps. This is true of every remote cache, including the ones the customer is comparing CoreLink against, but it is worth naming.

**Region locality.** Cache hits are fast when the CI runner is geographically near the region serving the tenant. CI runners on the other side of the world from the tenant's residency region will pay round-trip latency. CoreLink's four regions are positioned to keep the typical customer within geographic reach; customers running global CI fleets should consider their runner geography deliberately.

**Large blobs.** The economics described above are dominated by lots of small-to-medium blobs (the typical Bazel artifact shape). Caches that store predominantly very-large blobs (multi-gigabyte artifacts, model weights, dataset shards) see different trade-offs — the egress savings still apply, but per-hit latency is dominated by transfer time rather than overhead. CoreLink handles these workloads, but the calculator's defaults are tuned for the typical build-cache profile.

**The cache is not the whole inner loop.** A faster cache makes the inner loop faster only to the extent that the inner loop was bottlenecked on the cache. Teams whose inner loop is bottlenecked on compute, on test flakiness, or on review latency will see a smaller absolute impact than the cache-tax model suggests. The calculator helps disentangle these by surfacing the bottleneck, not by claiming the cache fixes everything.

## A pragmatic suggestion

If you are evaluating CoreLink, the most useful experiment to run is:

1. Pick a single CI pipeline that you know is bottlenecked on remote cache latency.
2. Configure that pipeline to use CoreLink alongside your existing cache for a two-week shadow period.
3. Compare green-build wall-clock time between the two.

That experiment will tell you, with measurement rather than marketing, whether the cache-tax line item in your annual model is one CoreLink can move.

## Where to go next

- **Calculator:** `corelink.humangr.com/calculator`
- **Pricing:** `corelink-docs.humangr.com/pricing`
- **SLO catalog:** `docs.corelink.humangr.com/slo`
- **Product profile (remote cache semantics):** `docs.corelink.humangr.com/architecture/remote-cache-product-profile`

— Product at CoreLink

---

## Internal notes (strip before publish)

- Target length: 1,800–2,100 words.
- Pending review: Finance (sign-off slot 13) + Engineer (sign-off slot 6) + Product (slot 8).
- All bench numbers held as placeholders pending 30d sustained staging data per WI-S20-007.
- No specific dollar amounts — all pricing references use `$X` placeholders per WI-S20-008 constraints.
