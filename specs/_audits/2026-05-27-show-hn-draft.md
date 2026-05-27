---
id: "AUDIT-2026-05-27-SHOW-HN-DRAFT"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
tags: ["audit", "show-hn", "launch", "marketing", "channels"]
references:
  - "specs/_audits/2026-05-27-distribution-channels.md"
  - "specs/_audits/2026-05-27-competitive-landscape.md"
  - "specs/_audits/2026-05-27-icp-customer-discovery.md"
  - "specs/_audits/2026-05-27-solo-saas-playbook.md"
  - "specs/_audits/2026-05-27-pricing-benchmarks.md"
  - "ROADMAP-TO-LAUNCH.md"
  - "README.md"
---

# Show HN draft — ready to paste

> Verbatim copy intended for `news.ycombinator.com/submit`. Voice is
> technical, specific, honest about limitations. Anti-patterns from
> distribution-channels §4 enforced: no superlatives, no "revolutionize",
> no "we're excited", no emoji, no marketing nouns CoreLink does not
> own (per competitive §7 DO-NOT list).
>
> Length budget for `text` field is HN's hard ~2,000 char limit (the
> form silently truncates above that). All drafts below are under it.

---

## §1 Post title

HN title field is plain-text, max ~80 chars. Format mandated by HN:
`Show HN: <Name> - <one-line value prop>`.

**Primary (use this one):**

```
Show HN: CoreLink - REAPI v2 build cache with BYOK and a verifiable audit log
```

77 chars. Names the product, names the protocol (REAPI = the
SEO/topical anchor; disqualifies wrong audience early per competitive
§4), names the two differentiators that no §1 competitor leads with.
No verbs, no superlatives, no "the".

**Alternate 1 (sharper compliance angle, narrower audience):**

```
Show HN: CoreLink - content-addressable build cache with a re-derivable audit chain
```

83 chars - on the edge of HN's limit; trim "build" if it clips.
Leads with the audit-chain hook; loses the REAPI keyword that
pre-qualifies the Bazel/sccache reader.

**Alternate 2 (polyglot angle, broadest):**

```
Show HN: CoreLink - one cache for Bazel, sccache, Cargo, npm, pip, OCI
```

72 chars. Strongest for the polyglot persona (icp §3.1 P1 sub-segment
that runs > 1 build system). Loses the trust/audit hook. Use this one
**only** if the audit-chain post fails to gain traction on a first
attempt 8 weeks earlier - we will not re-Show-HN with the same angle
inside 8 weeks (distribution-channels §7).

Recommendation: **submit Primary**. It pre-qualifies the reader, names
the protocol the buyer searches for, and the two-feature pair (`BYOK`
+ `audit log`) signals "this is for someone who has a compliance
problem, not someone who wants faster builds".

---

## §2 URL field

```
https://corelink-docs.humangr.com/
```

Per launch-readiness §1 this returns 200 today. The `/sandbox` route
on `corelink-app.humangr.com` is the no-auth-wall demo (sandbox
tenants are free, 24h-TTL, no card per README L28) but HN expects the
canonical site in the URL field and the demo link in `text`. Do not
deep-link the URL field to `/sandbox` directly - HN readers want a
landing page first, sandbox second.

---

## §3 Post body (text field)

Paste verbatim. 1,847 chars including line breaks.

```
CoreLink is a content-addressable cache that speaks the Bazel Remote
Execution API (REAPI v2). Same wire protocol as BuildBuddy / EngFlow /
bazel-remote, so Bazel, Buck2, BuildStream, Pants, Please, and recc
clients work without a plugin.

The reason I built it: at the last three places I worked, the build
cache lived as either (a) self-hosted bazel-remote on EC2 with an S3
bucket and no audit log, or (b) sccache pointing at an S3 bucket
that the security team flagged at the next audit because nobody
could prove what was in it or who put it there. The standard answer
is "trust the cloud provider," which stops working the first time
your SOC 2 auditor asks for an inclusion proof.

Three things that are mechanically different here:

1. Tenant isolation is a TLA+ invariant (INV-TenantIsolation), and
   the model checker runs in CI. Counterexamples block merge. I do
   not know of another managed REAPI cache that publishes its
   isolation model.

2. BYOK is real across four KMS providers (AWS KMS, GCP KMS, Azure
   Key Vault, HashiCorp Vault). Customer-held KEK, 5-minute in-memory
   DEK cap, AAD-bound ciphertext. Disable your KEK and the service
   cannot read your data - by construction, not by promise.

3. Audit log is RFC 6962 Merkle-chained, BLAKE3 leaves, Ed25519-signed
   hourly roots, RFC 3161 timestamps. You can `corelink audit
   verify-ndjson` an export offline against a 64-hex chain anchor and
   re-derive the root with no network call.

Free 24h sandbox at corelink-docs.humangr.com (no card). Paid tier
$25/mo. Open source: client SDK + verifier primitives dual MIT/Apache;
server stays closed.

What I deliberately did not build: remote execution (cache only),
build-events UI, JS-monorepo zero-config story. Cache only.

Solo founder. Would love feedback on the audit-chain UX and on the
BYOK config surface - both are the parts I have lowest confidence in.
```

Notes on the draft:

- "I built it" (not "we") - solo-founder honesty (solo-saas §5 P2;
  HN punishes ghost-team posturing).
- Three numbered technical mechanisms, not three feature bullets.
  HN rewards mechanisms over verbs.
- Explicit "what I did not build" paragraph - pre-empts the inevitable
  "how is this different from BuildBuddy" comment by leading with
  what is missing rather than what is present.
- Explicit feedback ask, narrow and specific (audit-chain UX +
  BYOK config). Open-ended "would love feedback" gets generic
  drive-bys; named surfaces get useful replies.
- No mention of "Cloudflare Workers" in the body. It is interesting
  but it draws "is this just a Cloudflare wrapper" into the top
  thread; better answered in §3 first-comment when asked, not as
  the lede.

---

## §4 First comment (post immediately after submission, same account)

Paste within 60 seconds of submission. ~1,420 chars.

```
Author here. A few details that did not fit in the post:

Architecture. Data plane is Rust on Cloudflare's edge. R2 holds the
content-addressed blobs (BLAKE3 digest is the storage key), D1 holds
hot metadata, Durable Objects scope per-tenant state (the actor
substrate that makes the TLA+ isolation invariant tractable). The
REAPI gRPC surface is implemented in a single crate (corelink-reapi)
and the architecture is mod-monolith + hex + EDA(audit) + actor(DO) +
µkernel(BYOK). One deployable, hexagonal at the boundaries.

Contention/idempotency tradeoffs. Writes are content-addressed, so
duplicate-write is a no-op - same hash, same blob. The contention
question moves up the stack to the Action Cache (AC), which is
key/value not content-addressed. AC writes use compare-and-swap on
the Durable Object owning that action key; concurrent writers race
and the loser observes the winner's value. We do not promise
last-writer-wins; we promise some-writer-wins and a consistent read
of whichever it was.

What I learned. (1) TLA+ is worth the activation cost only if you
let the counterexample block merge - "ran the model once" is theater.
(2) Cloudflare R2 zero-egress flips the unit economics of multi-region
cache vs. S3 hard enough that I would not build this on AWS today.
(3) BYOK as a real product surface (not a checkbox) is at least 4x
the engineering work of "encrypt with our key" - the DEK lifecycle
is the hard part, not the KEK.

Open-source boundary: crates/corelink-hash, crates/corelink-cas
(client), crates/corelink-byok client primitives, full OpenAPI v1
spec. Server stays closed. Repo link in profile.

Happy to answer anything.
```

Notes:

- Voice = engineer talking to engineers about tradeoffs. No "we are
  building" / "we believe" / "we are pleased".
- Three concrete "what I learned" bullets is the HN currency.
  Generic learnings get downvoted; specific tradeoffs get upvoted.
- "Repo link in profile" - HN policy prefers external links in
  comments to be in the user profile rather than re-linked under
  the post (which can read as upvote-bait).
- Pricing detail intentionally omitted from this comment - already
  in the body. Repeating it reads as a pitch.

---

## §5 Replies to top-5 anticipated questions

Pre-drafted. Each is short (HN rewards brevity in replies) and
opinionated. Edit names/handles when actually replying, but the
substance is locked.

### Q1: "Why not just self-host bazel-remote?"

Frequency prediction: certain. Will appear within the first 10
comments.

```
Honestly, you should, if your situation is "one cluster, one bucket,
one SRE who already knows S3". bazel-remote is genuinely good and
free under Apache-2.0.

Where self-host stops being free: (a) when your security team asks
for an audit log and you find bazel-remote does not publish one,
(b) when you operate it cross-region and your egress bill gets
interesting, (c) when your SOC 2 prep needs an answer to "prove the
cache wasn't tampered with" and "trust S3" stops being acceptable.

CoreLink's pitch is "you can keep bazel-remote on day 1, but when
those three things start mattering, here is a managed option that
treats them as primary features instead of paperwork".

Not trying to win against bazel-remote on price. Trying to be the
upgrade path when the audit/BYOK/cross-region story breaks.
```

### Q2: "How is this different from BuildBuddy / EngFlow?"

Frequency prediction: certain. Within the first 5 comments. Lead
with what those products do better, then differentiate.

```
Both BuildBuddy and EngFlow are great, and both do things CoreLink
does not. BuildBuddy has the build-events UI and full RBE (remote
execution); EngFlow has the Bazel-creator pedigree and operates RBE
at very large scale. If you need RBE - the actual remote-execution
layer where bazel ships your compile jobs to a fleet - go talk to
them, not me.

Where CoreLink picks a different point:

1. Cache only, by design. No RBE. The "we just want cache" buyer
   does not have to buy or operate a build farm.

2. BYOK across four KMS providers (AWS / GCP / Azure / Vault). Last
   time I checked, neither BuildBuddy nor EngFlow publishes
   four-KMS BYOK at the cache tier.

3. Self-serve pricing with a published number ($25/mo), not "Contact
   Sales".

4. Polyglot beyond Bazel: REAPI clients plus adapter paths for
   Cargo, npm, pip, brew, OCI under the same content-addressable
   substrate.

If you want a build-events UI today, BuildBuddy is the answer. I
am not trying to ship a worse version of it.
```

### Q3: "What about RBE (remote execution)?"

Frequency prediction: high. Within the first 15 comments.

```
Deliberately not on the roadmap for v1.

Two reasons. (1) Cache and RBE are different operational beasts. RBE
is a compute scheduler with a worker fleet, autoscaling, sandboxing,
cross-platform toolchains - it is a build farm that happens to read
from a cache. Shipping it well is more work than the cache itself,
and shipping it badly is worse than not shipping it. (2) A lot of
buyers I have talked to want the cache without inheriting the
operational burden of a build farm. RBE-only competitors exist;
cache-only competitors with a published audit chain do not.

If a customer asks for RBE with a real budget attached, I will
revisit. Until then, the answer is "go run buildfarm or pay
BuildBuddy or EngFlow, and point your AC at CoreLink".
```

### Q4: "Pricing seems high/low - how did you arrive at $25/mo?"

Frequency prediction: medium. Within the first 25 comments.

```
$25/mo lands inside what looks like a real anchor band for
solo-founder dev-infra tools: Resend $20, Garnix $25, BetterStack
$25 annual, Sentry Team $26 annual, Plausible $19 Business. Above
that band starts to require a procurement conversation; below it
starts to feel like a side-project tool.

Concretely: Pro tier is $25/mo for 500 GB cache + 20M cache reads/mo
+ 1M writes/mo, BYOK included, audit log included, four regions. At
that footprint the underlying Cloudflare cost is single-digit
dollars/mo per tenant, so margin is healthy enough to fund support
without venture pressure.

Free sandbox is 24h-TTL, no card. The intent is to let you run a
real shadow-cache comparison against your current setup in an
afternoon. Annual pricing is $250 (two months free), matching the
Plausible/BetterStack convention.

I will be A/B testing $19 / $25 / $29 over the first four weeks.
Happy to be wrong about the middle number if the data says so.
```

### Q5: "Is this just a wrapper around Cloudflare R2?"

Frequency prediction: medium-high. Within the first 20 comments. The
honest answer is "no, R2 is the blob layer, not the product" but
the framing matters - do not be defensive.

```
R2 is the blob backend, yes - same way Postgres is the row store for
half of HN. The product is everything around it.

What is not in R2: the content-addressable indexing layer (BLAKE3
digest is the storage key, dedup is automatic, GETs are re-hashed
client-side before bytes leave the verifier), the REAPI v2 gRPC
surface, the Durable-Objects per-tenant state actors, the BYOK
envelope (DEKs wrapped under your KEK in your KMS, never CoreLink's),
the RFC 6962 Merkle audit chain with re-derivable roots, the tenant
isolation invariant proven in TLA+, the four-region residency
policy enforced by structural invariant.

If your question is "could I do all of that on raw R2 myself?" -
yes, in the same way you could write a Postgres on top of a
filesystem. The product is the time you do not spend wiring it.

If your question is "are you going to break when R2 has an outage?"
- yes, that is a real shared-fate. The honest answer is: R2's
durability story is what we inherit, and any customer whose threat
model forbids Cloudflare should not pick CoreLink. We do not BS that
one in either direction.
```

---

## §6 Bonus pre-drafted replies (likely but not top-5)

### Q6: "Bus factor of one is a hard sell."

```
Fair. Three things that try to make the bus-factor objection
survivable rather than dismiss it:

1. Your data is BYOK-encrypted under a KEK you control. If CoreLink
   disappears tomorrow, your blobs in R2 are useless without your
   KMS, and your audit-chain export is verifiable offline against a
   chain-head anchor you can capture today.

2. The client SDK and verifier primitives are open-source dual
   MIT/Apache. The wire protocol is REAPI v2, which is not
   CoreLink-proprietary - you can repoint any REAPI client at
   bazel-remote tomorrow.

3. Month-to-month pricing, no annual commitment required.

I am not going to pretend solo-founder is the same risk profile as
a Series-A team. But the escape hatches are real.
```

### Q7: "What about SOC 2?"

```
Honest answer: SOC 2 Type I is the next compliance milestone, target
GA + 6 months. Type II follows.

What is in place today: full STRIDE threat model, CTRL-* catalog
mapped to SOC 2 / GDPR / LGPD / ISO 27001 / PCI DSS, audit-chain
that an external auditor can re-derive, security.txt, coordinated
disclosure policy.

What is not: a Type I or Type II report. If you have a hard SOC 2
gate on your vendor list today, we are on the trust center as
"target GA + 6mo" and that is the truth. Reach out and I will keep
you posted on when the report lands.
```

### Q8: "What benchmarks do you have?"

```
None I am willing to publish yet. The two reasons:

1. Cache benchmark numbers are dominated by workload shape (hit
   ratio, blob-size distribution, region) and the published number
   tells you about my test, not about your repo. I would rather
   give you a 24h sandbox and let you run your real workload.

2. The metrics that matter for the buyer who picks CoreLink (audit-
   chain re-derivation latency, BYOK rewrap p95, region-failover
   RTO) are not what most cache benchmarks measure. I will publish
   the ones that match the positioning when the methodology is
   defensible enough to argue about in public.

If you have a workload you want to throw at it, sandbox is free for
24h and I will personally help you get a comparison running.
```

---

## §7 Timing recommendation

Per distribution-channels §1 row 1 + icp §6.1 #3 + HN community data
on submission timing:

| Setting | Recommendation | Rationale |
|---|---|---|
| **Best day** | **Tuesday or Wednesday** | Highest mid-week HN traffic; readers fresh; engineers at desks |
| **Best hour** | **07:00-09:00 Pacific Time** (10:00-12:00 ET, 15:00-17:00 UTC) | EU morning + US east-coast morning overlap; full 12-hour daylight window for front-page momentum |
| **Acceptable** | Thursday 07:00-09:00 PT | Slightly weaker than Tue/Wed but still solid |
| **DO NOT submit** | Monday (weekend submission backlog buries new posts), Friday (early checkout, weekend graveyard), US federal holidays (low engagement), the week of any major tech conference keynote (Apple/Google IO/AWS re:Invent days swallow attention) |
| **DO NOT submit** | Any time the founder cannot stay at a keyboard for the next 6 hours minimum |

The submission window dominates everything else. If Gustavo cannot
commit 6 uninterrupted keyboard hours starting at 07:00 PT on a
Tue/Wed/Thu, **delay the submission to the next valid window**.
Showing up to your own thread 3 hours late is worse than not
launching that week.

Specifically forbidden: launching on the same day as a Show HN by
any competitor (BuildBuddy, EngFlow, Depot, Garnix, Nx, Tinybird,
Plausible). Check the `show` tab the night before; if a competitor
just launched, defer 24-48 hours.

---

## §8 Post-submission protocol

Per solo-saas §5 P2 (Tailscale's Avery Pennarun stayed awake 24+
hours hand-replying to every email after the HN launch) + per
distribution-channels §4 "REPLIES (first 4 hours)":

1. **Reply to every comment within 30 minutes** for the first 6
   hours. Even one-line negative comments. Especially negative
   comments. Silence reads as defensiveness.
2. **First-comment posted within 60 seconds** of submission (§4
   above). Reply rate visibility on HN counts the author thread
   into ranking signal.
3. **Acknowledge valid criticism before adding nuance**. "You are
   right that X is a limitation - here is what I am thinking..." -
   never start with "Actually" or "Well, technically".
4. **Do not delete negative comments**. Engage technically or
   acknowledge. Deletion is detected and punished by both HN
   readers and HN moderators (`dang` will notice).
5. **Do not brigade upvotes**. Do not ask friends, family, Twitter
   followers, Discord, Slack, mailing list, or LinkedIn for
   upvotes. HN's ring-detection is mature; ring-voted posts get
   dehydrated to oblivion and the account can be shadowbanned.
   This rule survives any short-term temptation.
6. **Do not link the same site from another account**. HN tracks
   the URL and any second-account submission to a story already
   posted is treated as ring activity.
7. **Reply to email follow-ups within the same day**. Comments
   that turn into "I am the platform lead at X, would love to
   trial this" go to the founder inbox - same-day reply for the
   first week.
8. **Monitor `news.ycombinator.com/from?site=corelink-docs.humangr.com`**
   for follow-on submissions of the docs site by other readers.
   These are higher-quality signal than the original post and
   often outperform it on ranking.
9. **Capture signups via Plausible UTM**. `?utm_source=hn` on the
   demo link in any subsequent comment that includes a URL.
   Distribution-channels §3 Day-13 retro depends on per-channel
   conversion data.
10. **Hard pause trigger**: if the post is `<20` upvotes and `<5`
    comments after 2 hours, **stop amplifying** (no Twitter
    thread, no Reddit, no Slack mention). Diagnose what went
    wrong, do not burn other channels off a dead Show HN
    (distribution-channels §3 hard-pause triggers).
11. **Hard pause trigger**: production incident lasting >2 hours
    during the launch window → pull all amplification, focus on
    stability, post a follow-up comment when service is restored.
    Do not hide an incident on a Show HN day - the same audience
    that votes you up will also watch the status page.

---

## §9 What we are NOT doing in this post

Explicit anti-list, per competitive §7 + distribution-channels §4
anti-patterns:

| Not doing | Why |
|---|---|
| "We are excited to announce" | HN punishes corporate-speak; reads as PR copy |
| "Revolutionary" / "game-changing" / "next-generation" | All marketing-buzzword anti-patterns; competitive §7 DO-NOT |
| "Faster builds" as the lede | Every §1 competitor says it; meaningless; pulls us into a category fight we cannot win |
| "RBE" / "remote execution" anywhere as a positive feature | BuildBuddy + EngFlow own that category; using the words pulls us into their frame |
| "Bazel" as the primary noun on title | Pulls EngFlow energy; we lead with REAPI |
| "Smart" anything | Nx Cloud's tagline space |
| Emoji | HN convention is no emoji in titles or bodies |
| Naming a customer who has not approved | We do not have signed lighthouse references yet (per ROADMAP-TO-LAUNCH); do not invent one |
| Claiming SOC 2 we do not have | Type I target is GA+6mo; current state is "in progress" - say that explicitly if asked (Q7) |
| Posting from a brand-new HN account | Use an existing aged account; new-account Show HN posts are auto-throttled |
| Asking for upvotes anywhere (Twitter, Discord, email, Slack) | Ring-detection; shadowban risk; community-trust burn |

---

## §10 Sources

- `specs/_audits/2026-05-27-distribution-channels.md` §1 row 1
  (Show HN ratings + rules), §3 (calendar Day-8 Show HN slot),
  §4 (post template skeleton), §7 (anti-channels).
- `specs/_audits/2026-05-27-competitive-landscape.md` §3 (positioning
  matrix), §4 (positioning statement), §7 (brand DO-NOT-copy list).
- `specs/_audits/2026-05-27-icp-customer-discovery.md` §3.1 (P1
  Priya persona - the reader this post must resonate with),
  §6.1 #3 (Show HN-specific guidance for the Bazel/sccache
  audience).
- `specs/_audits/2026-05-27-solo-saas-playbook.md` §3 P3 (Show HN
  pattern in 5/10 studied launches), §5 P3 (CoreLink-specific
  action), §6 hard rules #7, #8.
- `specs/_audits/2026-05-27-pricing-benchmarks.md` §1 + §5 ($25/mo
  derivation; Q4 reply numbers).
- `ROADMAP-TO-LAUNCH.md` §0 (30-second pitch verbatim source),
  brand DO-NOT list.
- `README.md` (canonical product description, current production
  status, audit-chain verify-ndjson UX).
- `specs/_audits/2026-05-27-launch-readiness-check.md` §1 (URL
  reachability proof for `corelink-docs.humangr.com`).

---

**End of draft. Action: Gustavo reviews, edits any names/handles,
opens `news.ycombinator.com/submit`, pastes §1 title + §2 URL + §3
body, hits submit, immediately pastes §4 as the first comment, and
follows the §8 protocol for the next 6 hours.**
