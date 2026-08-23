# Expansion Campaign #3 — CoreLink Streams (durable event streaming)

> **Status:** 🗄️ **SHELVED 2026-08-23 — deliberately, on sequencing, not on merit.**
> The idea survives scrutiny; the timing does not. CoreLink has no paying Cache
> customer yet, Runners is at v0.1, and this would be a third product with a
> categorically higher reliability bar (losing a customer's event log is an
> extinction event, not an inconvenience) and no durable moat. Cost of keeping it
> shelved is zero; the two cheap kill-checks below can be run opportunistically at
> any time.
>
> **Revisit when:** Cache has paying customers AND Runners has proven retention —
> or sooner if a customer asks for streaming by name.
>
> **Kill early if:** Cloudflare extends Queues into this space, or the step-1
> metering-basis check shows we are not materially cheaper.
>
> **CONCEPT BRIEF — idea stage, nothing committed, no code written.**
> A candidate expansion, written down early so it can be attacked before anyone
> invests in it. **Not** a change to the launch route: CoreLink launches as the
> cache + storage-governance product.
>
> **Owner:** Gustavo Schneiter · **Drafted:** 2026-08-23 · **Rev 2** (three-lens
> review applied: truth/verifiability, adversarial, editorial)
>
> **Confidence markers are load-bearing.** Claims grounded in the codebase carry a
> file cite. Estimates, recollections, and untested assumptions are labelled
> **UNVERIFIED** inline. Do not promote any UNVERIFIED line into a customer-facing
> claim without measuring it first.

---

## The thesis — two things the incumbents left on the table

**1. Nobody removed the expertise requirement.** Managed Kafka removed the
*servers*. It did not remove the *conceptual surface*: brokers, partitions,
consumer groups, offsets, in-sync replicas, retention policy, serialization
formats, schema registries. The implementation has moved on considerably since
2011 — KRaft replaced ZooKeeper, KIP-848 reworked group coordination — but what the
*user* must understand has barely shrunk. Confluent shipped a documentation
library, not a rail. Rails did not make servers cheaper; it collapsed the
conceptual cost. Somebody did that for databases, for payments, for deploys.
Nobody has finished doing it for event streaming. *(See "The competition" — this
claim needs qualifying against Pub/Sub, which is the strongest counterexample.)*

**2. Nobody removed the retention penalty.** Managed Kafka prices retention as a
penalty — you pay to keep history and you pay again to read it back — so in
practice most deployments expire the log after ~7 days. Our storage substrate has
no read penalty and a cheaper keep penalty, so our customers stop deleting.

The two claims do different work, and both are needed:

> **The rail opens a market that could not buy at any price. The economics change
> what the product does for the market that already buys.**

Neither is a price-war pitch. "The same thing, cheaper" is a position anyone with a
spreadsheet can undercut. What holds is the capability the price unlocks — the
customer changes their behaviour, and the new behaviour is worth more than the
discount — and the audience the rail admits, who were previously excluded twice
over: by the licence cost, and by the platform engineer they would have had to hire.

### Why now

The population of people building software is expanding rapidly because of AI
assistance, and much of that population cannot use Kafka — not for lack of effort,
but because the conceptual surface is larger than the problem they are solving. A
market that is growing, and that the incumbent's design does not serve, is timing
rather than luck. **The counter-argument to this is in the agent-completable
section and it is serious; read it before repeating this claim.**

---

## What the product is, in plain terms

An event log. One system writes events; many systems read them independently, in
order, each at its own pace. Nothing blocks anything else, and a system added six
months later can read the whole history from the beginning and catch itself up.

This is the backbone pattern of essentially every technology company past a certain
size, and it is why Kafka is entrenched.

**The dysfunction we are attacking:** because retention and replay both cost real
money on every managed offering, the common production setting is to expire the log
after ~7 days. Customers own a time machine and throw the tape away every week.

---

## Why the customer cares (what "never delete" unlocks)

Concrete workflows that a 7-day log forbids and a permanent log allows:

- **Backfill a new consumer.** A service added today reads three years of history
  and bootstraps itself. No migration project, no separate warehouse export.
- **Rebuild derived state.** A bug corrupted a table? Recompute it from the log.
  The log is the truth; the database is a cached opinion derived from it.
- **Retrain and evaluate on real data.** Run a new fraud model, ranking model, or
  eval suite against years of true production events before shipping. Impossible
  today, because the data no longer exists.
- **Audit without a compliance project.** The log *is* the trail.

The AI/ML angle deserves emphasis: for a modern ML team, **replay is the core
workflow**, and replay is precisely what the incumbents meter. That is an ICP whose
main activity is the thing our competitors tax.

---

## Who buys it — two distinct ICPs, two doors, one log

The operational-pain argument and the cost argument land on different customers,
and conflating them produces a muddled product.

**ICP 1 — the company priced out of event streaming entirely.** Not a Confluent
refugee: someone who never had streaming at all, excluded twice over — by the
licence cost and by the platform engineer they would have had to hire. The larger
and less contested market, and it matches CoreLink's stated DNA: self-serve for
SMBs, not enterprise. The same move Stripe made with payments and Supabase with
Postgres — take something that required a specialist team and sell it to everyone
who could never have that team.

**ICP 2 — the company already paying a managed-Kafka bill.** Here the operational
argument lands weakly, because Confluent and MSK already solved the on-call pain;
that is what the premium buys. **UNVERIFIED as a characterisation of their current
offering.** What lands is the cost structure and the retention penalty.

Being honest about the difference matters: **against self-hosted Kafka we remove an
enormous operational burden, but so does Confluent — we are not unique there.**

**This resolves the protocol decision in Hard part 1.** The two ICPs want opposite
things from the interface, so they get different doors into the same log:

| Door | Audience | Price |
|---|---|---|
| **Native API, zero-config ("on rails")** | ICP 1 — never had streaming | consumption only |
| **Wire-compatible gateway** | ICP 2 — migrating existing Kafka code | consumption + per-tenant gateway |

Zero-config and wire compatibility would fight each other through a single door: a
real Kafka client *wants* to declare partitions and negotiate metadata. Split, each
door does what it is good at, and option (c) below stops being a compromise and
becomes the design.

### What "on rails" means concretely

Kafka is the anti-Rails: everything is a decision. The inverse:

- **No cluster, no provisioning.** A topic exists on first write — this falls out
  of the substrate rather than being built.
- **No partition count to choose.** Start at one, split automatically when
  throughput demands it. *(See Hard part 4 — this is the hardest promise here.)*
- **No schema registry to stand up.** Registered on first write.
- **No connection configuration.** The PAT the customer already holds, plus a topic
  name.
- **Opinionated defaults:** compression on, batching on, retention permanent,
  compaction off.
- **A one-line SDK:** `streams.publish("orders", event)` /
  `streams.consume("orders", group="billing")`.

### "Agent-completable" — the rail as a testable criterion

"Easy enough for a fourteen-year-old with an AI assistant" sounds like marketing.
It can be made into an engineering gate instead.

An LLM writing code against an API succeeds or fails on four properties:

- **Small surface.** Few concepts, few calls. A large surface makes the model
  hallucinate plausible-but-wrong configuration keys.
- **Convention over configuration.** One obvious way and the model gets it right;
  forty options and it invents one.
- **Legible failures.** `UNKNOWN_TOPIC_OR_PARTITION` does not tell anyone what to
  do. An error must state the correction.
- **No out-of-band setup.** The decisive one. If getting started requires clicking
  through a console to provision a cluster, **an agent cannot do it** — it writes
  code, it does not provision infrastructure. If a topic exists on first write and
  auth is a PAT the customer already holds, an agent goes from zero to a working
  stream with no human touching a console.

That yields a metric that belongs in CI rather than a pitch deck:

> **Hand the model the documentation. Does it produce working code on the first
> attempt, with no human intervention?**

As a recurring eval it becomes a guard-rail against the product's own entropy,
failing the moment someone adds a step requiring a human.

**⚠️ The serious counter-argument, which must not be discovered by a sceptic
first.** An LLM's fluency with an API depends heavily on its training corpus. The
model knows Kafka intimately and knows CoreLink Streams not at all. **A small,
clean, unknown API can lose to a large, messy, deeply-represented one** — in exactly
the AI-assisted scenario this section targets.

Mitigations exist and must be designed in rather than assumed: a surface small
enough to fit entirely in context, a maintained `llms.txt`, an MCP server, and
documentation written for retrieval. But the advantage here is **not** automatic,
and "AI makes our simplicity win" is a claim that has to be earned by evaluation.

**UNVERIFIED:** the eval is unbuilt and the pass rate of any real design against it
is unknown. It is proposed as a gate, not reported as a result.

### Auditable and transparent — in both senses

**Transparent pricing.** Managed-Kafka billing is widely described as opaque:
capacity units, partition counts, and egress interacting. **UNVERIFIED as a
characterisation of current offerings.** Our answer is to bill in the buyer's own
unit, with no platform fee — see *Pricing*.

**Auditable data.** A permanent log, plus the signed-attestation machinery already
in production, plus per-subject erasure, means the log *is* the audit trail and its
contents can be proven.

---

## Why us — the structural advantage

The advantage is one specific line on a bill.

| | Storage ($/TB-month stored) | Read-back |
|---|---|---|
| Confluent Cloud (infinite retention) | ~$95 | charged (egress) |
| AWS MSK (tiered storage) | ~$100 | charged (inter-AZ) |
| **CoreLink on R2** | **$15 (our cost)** | **$0** |

**⚠️ UNVERIFIED, and this is the single most decision-critical unknown in the
document:** the competitor figures are recalled list pricing, **and their metering
basis is unknown**. If their per-TB rate is denominated in stored (compressed)
bytes and ours were denominated in logical bytes, an apparently cheaper rate would
be several times more expensive in practice. Pricing cannot be set before this is
answered. See *De-risking*, step 1.

R2's zero-egress policy is not a discount we choose to offer; it is a cost line
that **does not exist for us**. Nobody building on S3-class storage can undercut it,
because for them it is a real cost being marked up. *(It is also a vendor policy
rather than a law of physics — see "Risks that could kill this".)*

Two further advantages that are about quality, not price:

**Consumer-group coordination becomes trivially correct.** In Kafka, group
membership and partition assignment are a distributed protocol, and rebalance
storms are a well-known operational failure mode. A Durable Object is a
single-threaded serialization point by construction — no election, no gossip — so
that class of failure does not arise. Here we are *better*, not merely cheaper.
**This is the one structural quality claim in the document; it is deliberately
stated once.**

**Idle consumers cost approximately nothing.** WebSocket hibernation is designed for
exactly the long-lived, mostly-idle connection a stream consumer holds. **UNVERIFIED:
the billing consequences of hibernation at our expected connection counts.**

---

## The competition — the full landscape, not just Kafka

An earlier revision of this brief discussed only Kafka vendors. That was the
document's largest blind spot: a hostile evaluator opens with a different question.

### Kafka-compatible vendors

**Confluent Cloud, AWS MSK, Redpanda, Aiven.** The retention-penalty argument lands
here, and the ecosystem argument (why wire compatibility matters) comes from here.

**WarpStream** is the closest architectural precedent: Kafka protocol compatibility
with object storage as the only storage layer, no local disks. It was acquired by
Confluent. **UNVERIFIED: recalled from memory, including the price. Verify before
citing.** The upside is that the thesis has been market-validated; the downside is
that the space now has a well-funded incumbent. What survives the comparison is
specific: WarpStream runs on S3, where egress and inter-AZ transfer are real costs.
On R2 that line is zero. An acquisition also tends to vacate the cheap-disruptor
position it was bought out of.

**Redpanda** is often cited alongside WarpStream and should not be — it is a full
broker rewritten in C++ with its own storage engine (thread-per-core, local NVMe),
not a translation layer over object storage. It proves that reimplementing the Kafka
protocol is tractable; it is not the same architecture.

### ⚠️ Cloud-native streaming — the strongest counterexample to thesis 1

**Google Pub/Sub, AWS Kinesis, Azure Event Hubs, AWS EventBridge.**

**Pub/Sub in particular is already serverless, already has no partition count to
choose, and is already pay-per-use.** It is a direct counterexample to "nobody
removed the expertise requirement," and the differentiation against it is
substantially weaker than against Confluent. It must be argued explicitly:

- **Retention.** Pub/Sub's retention window is bounded and replay is limited;
  permanent retention with free replay is a different product. **UNVERIFIED —
  current limits and pricing must be checked.**
- **Portability.** Pub/Sub is a proprietary API with no Kafka ecosystem; our
  wire-compatible door means the customer is not locked into our API.
- **Cost at retention scale.** Our advantage is specifically in keeping and
  re-reading history, not in per-message delivery.

**If those three do not hold up under scrutiny, thesis 1 needs rewriting.** This is
a real risk to the positioning, not a footnote.

### ⚠️ The platform owner

**Cloudflare Queues already exists** and is the direct neighbour. Cloudflare could
extend it into this product and would start with every structural advantage we
claim, because they own the substrate. This is not hypothetical competition — it is
the most likely way this product dies. See *Risks that could kill this*.

---

## What already exists in the codebase

The partition primitive is **already built and already bound in production** —
though it carries no traffic.

- **`EventLogDO`** ([worker/src/event_log_do.ts](../../worker/src/event_log_do.ts))
  — ratified by ADR-0065. A per-tenant, append-only, totally ordered log with a
  strictly monotonic, gap-free `seq` assigned under the Durable Object's
  single-writer serialization. One DO instance per tenant via `idFromName`. This is
  a Kafka partition, minus the tiering. 337 lines, surface `handleAppend` /
  `handleRead`.
- **It is deployed and bound in production (migration v3) but wired to no request
  path.** Imported at `worker/src/index.ts:27`, exported at `:4044`, binding
  declared optional at `:90` — no handler calls it. That is good (it exists, the
  migration is applied, there is no debut risk) and honest (it is unproven under any
  load whatsoever).
- **The shard + coordinator topology is proven.** `RequestMeterShardDO` /
  `RequestMeterCoordinatorDO`
  ([worker/src/request_meter_shard_do.ts](../../worker/src/request_meter_shard_do.ts))
  run one DO per `(tenant, region)` with refill/epoch arithmetic and over-serve=0
  held under concurrency — the same shape as "topic with N partitions plus a group
  coordinator."
- **The multi-tenant spine is done:** PAT auth, per-tenant quotas, billing, regional
  residency, and the GDPR erasure path — all live, all reusable.

**The gap is already named in our own code.** The `EventLogDO` doc comment states it
*"does not itself archive to R2; rolloff/archive is a documented policy layered by
the operator."* R2 tiering — the exact mechanism this product depends on — is a
deliberately deferred item, not an unknown one.

---

## Semantics we must define before promising anything

A Kafka user asks these within five minutes, and the brief previously had no
answers. They are design obligations, not open questions to leave dangling.

- **Acknowledgement.** Is a write acknowledged when the DO's storage commits, or
  only after the R2 flush? What is the `acks=all` equivalent, and what is the
  durability of the hot tail between commit and flush?
- **Ordering scope.** `EventLogDO` is totally ordered *per tenant*. Under
  multi-partition topics, ordering is per-partition — as in Kafka. This must be
  stated explicitly, because "totally ordered" invites a stronger reading than we
  can deliver.
- **Delivery semantics.** At-least-once is the v1 target; exactly-once is a
  non-goal. Consumers must be told plainly.
- **Region scope.** Regions are separate physical buckets under residency rules. Is
  a topic single-region? A global customer will ask for the MirrorMaker equivalent,
  and the answer is probably "not in v1" — which needs saying.
- **Failure behaviour.** What happens if a DO is evicted mid-flush, or a segment
  write to R2 fails after the append was acknowledged?

**UNVERIFIED across the board — none of these have been designed.**

---

## The hard parts

### Hard part 1 — the wire protocol forces a product decision, early

Kafka clients speak a **binary protocol over raw TCP**. Cloudflare Workers accept
HTTP and WebSocket; **UNVERIFIED but believed firm: they cannot accept inbound raw
TCP** (outbound `connect()` exists; inbound listening does not). **This is the most
load-bearing platform claim in the document — if it is wrong, the entire (a)/(b)/(c)
decision dissolves and the gateway may be unnecessary. Verify first.**

Three options:

**(a) Native HTTP/WebSocket API.** Fastest to build, perfect substrate fit, scales
to zero. But it is then *not Kafka* — it forfeits the client libraries, Flink,
Debezium, and the connector ecosystem. That ecosystem is the reason Kafka wins.

**(b) Protocol gateway in a container.** We already run containers. One terminates
TCP, speaks the Kafka protocol, and writes through to DOs and R2. A genuine drop-in:
the customer changes a connection string and nothing else.

**(c) Hybrid — recommended.** Native HTTP for greenfield and serverless consumers;
the container gateway as a billed, per-tenant add-on for wire compatibility. As
argued under *Who buys it*, this is not a compromise between (a) and (b) — it is
what falls out of serving two ICPs that want opposite interfaces.

**On who pays for the gateway.** Provisioned per tenant and billed as a line item,
its cost passes through — a customer migrating from Confluent or MSK is already
paying for an always-on cluster, so this is parity, not a regression. Per-tenant
provisioning also makes isolation trivial, whereas a shared gateway would terminate
many tenants' auth in one process. Billing and security point at the same design.

**The genuine trap is the container pool floor, not the gateway.** Our historical
container cost problem was not "containers cost money" — it was a per-region pool
floor charged regardless of use. **UNVERIFIED: this is recalled from the internal
cost incident and should be re-confirmed against current billing.** If the gateway
inherits that mechanism, we pay a floor in five regions with zero streaming
customers. **Whether the gateway can be its own scale-to-zero deployment rather than
riding the shared pool must be answered before (b) or (c) is committed.**

Secondary trade-off: scaling to zero means the first connection pays a cold boot,
which a producer with a tight timeout may not tolerate. Either accept it, or keep it
warm and charge for warmth.

#### Are we rewriting Kafka? No — we are writing a translator

"Kafka" is several things bundled together, and the substrate supplies most:

| Component | Supplied by | Do we build it? |
|---|---|---|
| Storage engine (on-disk segments) | R2 | No |
| Replication / durability | Cloudflare, built in | No |
| Controller, leader election, KRaft/ZooKeeper | A DO is a single-writer by construction | **No — the problem disappears** |
| Ordering and offsets | `EventLogDO`, already deployed | No |
| Consumer-group coordination | DO | Yes, but the easy version |
| **The wire protocol** | — | **Yes. This is the work.** |

The parts that make Kafka miserable to operate — replication, the controller,
distributed rebalance — are precisely the parts we do **not** build. What remains is
a translation layer whose hard state lives elsewhere.

That layer is itself smaller than it sounds:

- **Encoding may come for free.** The `kafka-protocol` Rust crate provides message
  types generated from Kafka's official schema, making the work *semantics* rather
  than byte parsing. **UNVERIFIED: the crate's current state and completeness.**
- **A working client needs a subset.** Approximately `ApiVersions`, `Metadata`,
  `Produce`, `Fetch`, `ListOffsets`, `OffsetCommit`, `OffsetFetch`,
  `FindCoordinator`, `JoinGroup`, `SyncGroup`, `Heartbeat`, `LeaveGroup` — roughly a
  dozen rather than the full catalogue. **UNVERIFIED: the real minimum set must be
  derived from an actual client handshake, not from this list.** WarpStream is the
  precedent for this shape.
- **The fiddliest part gets easier here.** With a DO as coordinator we implement
  only the server side of group coordination, with no distribution.

**Honest sizing:** a real project of some focused weeks, not an afternoon — a
bounded translation layer, not a distributed-systems rewrite.

**Sequencing that avoids betting on it:** ship the native API first and use it to
test whether demand is real. Write the translator only when a customer asks. If
nobody asks, the project was never spent.

### Hard part 2 — per-partition throughput is the biggest unknown, and it is measurable this week

A Durable Object is single-threaded with durable storage writes. A Kafka partition
sustains on the order of 10–100 MB/s (**UNVERIFIED**); a DO will sustain far less.
**UNVERIFIED: the actual figure. The working estimate is low thousands of small
appends per second, and that is a guess with no measurement behind it.**

The mitigation is Kafka's own doctrine — add partitions — and Kafka users already
think in shards. The consequence is that we do not serve the single-topic firehose
tier, which is fine, because that is not the ICP. But note the second-order effect:
**more partitions means more DO instances, and DO request and duration billing
scales with partition count** — which feeds directly into the cost model gap noted
under *Execution*.

**`EventLogDO` is deployed today.** Benchmarking append throughput against a real
production DO answers the largest unknown in this brief and is roughly a day of
work, with no new product code. **This is the first action.**

### Hard part 3 — "never delete" creates a compliance problem we must solve

A permanent event log is in direct tension with the GDPR right to erasure.
Competitors avoid this tension by accident: a 7-day retention window erases
everything continuously. **By making retention permanent, we create the problem.**

Required, not optional. It is also an opportunity: "permanent log with per-subject
erasure" is a combination nobody serves well, and we have more of the machinery than
most — a live physical-erase seam, per-tenant key derivation, and a signed
erasure-attestation path already in production.

**UNVERIFIED design direction:** per-subject crypto-shredding — encrypt each data
subject's payloads under a per-subject key and delete the key on an erasure request,
so segments stay immutable while content becomes unrecoverable. The standard answer
for immutable stores, but not designed or costed here.

### Hard part 4 — automatic partitioning is the price of the zero-config promise

Splitting a partition without breaking per-key ordering requires consistent hashing
and careful resharding. Kafka forces the count to be chosen upfront precisely
because the alternative is hard — and Kafka cannot even reduce it afterwards.

This is where the architecture time should be expected to go. **Until it has a
design, "no partition count to choose" is a claim we cannot make**, and the rail is
correspondingly less magical.

---

## Design constraints already known

**Segment size is a hundred-fold cost decision.** R2 bills per operation as well as
per byte, so the cost scales inversely with segment size — a 100 MB segment costs
1/100th the operations of a 1 MB segment for the same volume.

Concretely, for 1 PB at 100 MB segments: ~10 M objects → **~$45** in Class A
(writes, $4.50/M) and **~$3.60** for a full replay in Class B (reads, $0.36/M). At
1 MB segments the same petabyte is ~1 B objects → **~$4,500** and **~$360**.

*(An earlier revision described this as thousand-fold by comparing a Class B read
figure against a Class A write figure — different operation classes. The real factor
is 100×, on either class.)*

**Flush in large segments; never per message.** An architectural invariant from day
one. Kafka clients already batch natively (`linger.ms` / `batch.size`), which works
in our favour, but the pricing model must be denominated in bytes, never messages.

**Segments are large; the frames inside them are small.** Compression must be
applied in ~1 MB indexed frames rather than one blob per segment, or serving a
single offset would require decompressing the whole segment. The two constraints
compose: **large segments, small indexed frames.**

---

## Pricing — bill in the buyer's unit

### The unit decision, and why it is the whole game

A challenger that is not directly comparable loses the comparison before it starts.
A buyer evaluating us against Confluent will put both numbers in one spreadsheet
cell; if the units differ, they either convert wrongly or default to the familiar
vendor.

**Therefore: bill per TB of stored (compressed) bytes — the same unit the incumbent
uses.** An earlier revision proposed billing logical (uncompressed) bytes on the
grounds that it is more predictable for the customer. That was wrong: it introduces
a unit mismatch that made this document's own comparisons incoherent, and
predictability has a cheaper fix (show the ratio and a projected bill in the
dashboard; after the first week of real data the customer's uncertainty is small).

| | $/TB-month, stored |
|---|---|
| Confluent Cloud | ~$95 **(UNVERIFIED, basis unknown)** |
| AWS MSK | ~$100 **(UNVERIFIED, basis unknown)** |
| **CoreLink — illustrative** | **$30–40** |
| *Our cost* | *$15* |

At $35: **~57% margin**, and **~2.7× cheaper** on a directly comparable number — no
conversion, no asterisk.

### What this choice actually decides

The two possible bases differ in who receives the compression gain:

- **Billing stored bytes:** compression benefits the **customer** (their bill
  shrinks). Our margin is capped by the $15/TB floor.
- **Billing logical bytes:** compression benefits **us**; margin scales with the
  ratio.

**Legibility wins over margin**, because an incomparable challenger does not get
evaluated. Compression remains a customer-facing benefit we can market — their
effective cost per TB *written* falls to roughly $4 at an 8× ratio.

### The rail is free; consumption is billed

Setup, configuration, auto-partitioning, schema handling, the dashboard, and the SDK
carry **no platform fee**. There is no capacity unit, no cluster minimum, and no
charge for the product being easy.

**⚠️ Honest caveat:** "free because it is software written once" understates it.
Support is recurring headcount, and support is the real cost of serving self-serve
SMBs at volume. The rail is free to the customer; it is not costless to us.

### Why this is structurally hostile to the incumbent

Confluent's pricing is the inverse: a capacity unit bundles the managed-ness into a
large provisioned block, so the customer pays for "easy" whether or not they consume
capacity, producing a high effective minimum — precisely what excludes smaller
companies. **UNVERIFIED as a description of their current model.**

We have no minimum, because idle is close to free on this substrate. **Near-zero
cost until the first real volume** is the SMB unlock, and it costs us little to
offer. *(Precisely: a DO with data in storage still incurs storage billing, so
"idle is zero" is nearly true rather than exactly true.)*

### The pricing principle that matters more than the margin calculation

The thesis is behaviour change: the customer stops deleting. **So the price must sit
below the threshold at which deletion becomes worth thinking about.**

Test the number against the thesis: a customer holding 1 TB compressed pays $35/month
— far below the cost of a meeting about retention policy. A price high enough to
prompt that meeting has broken the thesis, whatever its margin.

### The guardrails

- **Storage must be metered.** Bundling unlimited retention into a flat price is the
  fastest way to lose money here. A free allowance as an on-ramp needs a published
  cap, never an unlimited promise.
- **Margin sits on consumption, which scales with the customer's own value.**
- **The free rail is the on-ramp to the billed behaviour.** A customer who accepts
  "stop deleting" grows into metered storage by design. The free tier is the funnel,
  not charity.
- **Compression ratio becomes price-critical at aggressive rates.** See below.

---

## Compression — where it applies, and what it does not buy

### Where it applies

**At rest in R2 (the segments).** Where the money is. Event payloads (JSON, Avro,
protobuf) are highly repetitive within a segment; zstd on a ~100 MB segment of
similar events plausibly yields **5–15×**. **UNVERIFIED for real workloads — this is
the single figure the margin model leans on hardest.**

**On the wire (producer → us).** Kafka clients already compress at batch level
(`compression.type`). Accepting an already-compressed batch and **storing it as
received** puts the CPU cost on the producer, which is what modern brokers do.

**At flush (DO → R2).** Only when the producer did not compress.

**⚠️ A Durable Object is the wrong place to compress.** DOs operate under a CPU
budget; compressing a 100 MB segment inside one will likely exceed it. **Push
compression to the producer wherever possible and do the remainder in a container
where CPU is cheap. Never in the DO.**

**On read, ideally we never decompress.** A Kafka client accepts compressed batches,
so the path is passthrough; CPU is spent only when a consumer asks for raw bytes.

### The advanced play — per-topic trained dictionaries

zstd supports trained dictionaries, and dictionaries help most on *small* inputs —
precisely the low-volume, small-batch case. An event stream is extremely repetitive
at the *schema* level. A per-topic dictionary could plausibly move small-record
ratios materially. **UNVERIFIED — must be measured on real event data.**

The rail already registers a schema on first write, so the dictionary falls out of
something being built anyway. The natural complement is **background recompaction of
cold segments** to zstd-with-dictionary, running in a container: hot data stored as
received (fast), cold data dense (cheap).

### ⚠️ What compression does NOT buy — do not put this in a pitch

**Do not claim a headline multiple over Confluent based on compression.**

Compression improves our **absolute** cost. It does not necessarily improve our
**relative** position, because the incumbents compress too and store the compressed
bytes. Since we now bill in the same unit they do, the comparison is already
apples-to-apples and compression does not move it.

What does *not* cancel, and remains the argument: **zero egress and near-zero idle.**
Those are structural.

### Compression is price-critical, not just margin-relevant

Because our floor is $15/TB stored, the ratio determines what price is survivable
when expressed per TB *written*:

| Ratio | Our cost per TB written |
|---|---|
| 8× | $1.88 |
| 5× | $3.00 |
| 3× | $5.00 |

At aggressive pricing a poorly-compressing customer can erode margin badly. This
needs either a published floor, a clause for incompressible payloads, or pricing set
against a conservative assumed ratio. **Measuring real ratios is therefore a
prerequisite for setting price, not a refinement of it.**

### Recommendation

1. **zstd as the storage format**, ~1 MB indexed frames, from day one.
2. **Accept gzip / snappy / lz4 / zstd on the wire**; store hot data as received.
3. **Recompact cold segments** to zstd-with-dictionary in the background, in a
   container.
4. **lz4 on the hot tail** if latency demands it.
5. **Bill stored bytes; display the ratio** as transparency, not as the basis.
6. **Never compress inside a Durable Object.**

---

## Execution — shape, placement, cost

### Feature, product, or service?

**A new product in the CoreLink family — the third pillar**, alongside Cache and
Runners. Not a feature of Cache (different audience, billing model, surface) and not
a service (it has its own pricing, ICP, and lifecycle). But it **shares the spine** —
auth, tenancy, residency, billing, erasure — which is what makes a third product
viable without a third team.

```
CoreLink
├─ Cache      — CAS + Action Cache          (live)
├─ Runners    — ephemeral compute            (v0.1 shipped)
└─ Streams    — durable event log            (concept)
```

### How it works

```
producer → Worker (PAT auth, quota) → partition DO (ordering + seq)
                                          ↓ flush in ~100 MB segments
                                         R2 (unbounded history, $0 egress)

consumer → Worker → coordinator DO (group, offsets)
                          ↓
                   DO tail + R2 segments, merged
```

The DO supplies ordering and single-writer serialization. R2 supplies cheap
retention and free reads.

### What is already built — and the distinction that matters

**Substrate: roughly 70% — an estimate, not a measurement.** DO runtime and
migrations, R2 with zero egress, PAT auth, multi-tenancy, quotas, billing, regional
residency, the physical-erasure seam, signed attestation, and a proven
shard+coordinator pattern.

**The product itself: roughly 3–5%.** The 337 dormant lines of `EventLogDO`, wired
to nothing.

It is easy to confuse "the platform is built" with "the product is built." The
platform is. The product barely exists.

### ⚠️ The cost model in this document is incomplete

Every figure here counts **R2 storage only**. It does not model Durable Object
requests, DO duration, DO storage, or Worker invocations.

For a **high-message-rate, low-byte** workload — a common event-streaming shape —
compute could plausibly exceed storage cost, and Hard part 2 makes it worse: more
partitions means more DO instances. **The stated ~57% margin is therefore optimistic
because it counts one line.** A full unit-cost model is required before any price is
published.

### Which repository

**Phase 1 lives in `corelink-server`. Do not create a repository before the
measurement.**

`EventLogDO` already lives there, the Worker is there, and the product needs the
auth / tenancy / quota / billing that are there. A separate repository would mean
duplicating the spine or transcribing contracts across repos — a pain this
organisation **already carries** with `corelink-runners`, whose "wire-contract law"
of types transcribed on both sides exists precisely because of that split.

**Later, if it graduates:** extract to `corelink-streams` the parts with their own
release cadence — the SDK and the Kafka gateway (a Rust container binary speaking
HTTP to the Worker, a clean seam). The data plane stays where it is.

Creating a repository before measuring throughput is ceremony ahead of knowing
whether the idea stands up.

### Complexity

| Component | Difficulty | Why |
|---|---|---|
| R2 tiering | medium | known mechanics; the trap is segment size, documented above |
| Multi-partition topics | medium | key routing plus fan-out |
| Consumer groups + offsets | medium | **easier than Kafka** — no election |
| Retention / compaction | medium-low | policy, not a distributed system |
| Native API + SDK | low | ordinary work |
| **Automatic partitioning** | **high** | Hard part 4 |
| **Protocol gateway** | **high** | ~12 APIs, conformance against real clients |
| Erasure / crypto-shredding | medium-high | Hard part 3 |
| Durability / ordering semantics | medium | undesigned; see *Semantics* |

Nothing here is research. Two pieces are genuinely hard engineering.

### Token estimate

⚠️ **A model, not a measurement.** Assumptions stated so they can be disputed:
~15–25k LOC of production code plus tests at this repository's ratio (proptest,
acceptance suites, OKF gates, changelog, ADRs), at ~1,500–3,000 tokens per delivered
LOC — expensive here because every change requires loading architecture, passing
gates, and iterating review.

| Phase | Tokens | Delivers |
|---|---|---|
| **Step 0 — measurement + de-risking** | **2–5M** | whether spending the rest is justified |
| Phase 1 — log core, tiering, native API | 20–40M | the product exists |
| Phase 2 — the rail (SDK, dashboard, auto-partitioning, eval) | 15–35M | the differentiator exists |
| Phase 3 — Kafka gateway | 15–40M | ICP 2 unlocked |
| Cross-cutting — erasure, semantics, ADRs, OKF, docs | 5–15M | |
| **Total** | **~57–135M** | |

Largest variance: automatic partitioning and the gateway. Deferring the gateway until
a customer asks removes 15–40M from the immediate figure.

**The number that matters is the first: 2–5M to learn whether the rest is worth
spending.**

---

## Risks that could kill this

Named explicitly, because the previous revision built the entire moat on
assumptions it never examined.

**1. R2's zero-egress policy is a commercial decision, not a law of physics.** It is
Cloudflare's strategic choice to attack AWS. The central pillar of this product's
economics rests on a single vendor's pricing decision that we do not control and
cannot hedge. If it changes, the thesis evaporates. **This is concentration risk and
it has no mitigation beyond acknowledging it.**

**2. Cloudflare could build this.** Queues already exists. The platform owner would
start with every structural advantage we claim. Our defence is speed, the rail, the
Kafka-compatible door, and the multi-tenant spine we have already paid for — none of
which is durable against a determined platform owner.

**3. Pub/Sub may already have taken the "easy" position.** If the three
differentiators listed under *The competition* do not survive scrutiny, thesis 1
needs rewriting and the product narrows to a cost play.

**4. The reliability bar is categorically different from anything we run today.**
CoreLink is a cache. Losing cache data is an inconvenience. **Losing a customer's
event log is an extinction event** — it is their source of truth. This changes the
required SLA, backup strategy, testing depth, on-call posture, and legal exposure.
The organisation has not operated a product with this risk profile.

**5. There is no exit story.** If the data is the stickiest asset, buyers will ask
how they get it out. An answer is needed before the first enterprise conversation,
and "you can't" is the wrong one.

**6. The AI-fluency advantage is not automatic.** See the counter-argument in the
agent-completable section.

---

## Explicit non-goals (v1)

- **Not** the single-topic firehose tier.
- **Not** exactly-once semantics or transactional producers.
- **Not** a stream-processing engine. We are the log, not the compute over it —
  though a Runners-driven "replay a range through a job" integration is an obvious
  later adjacency.
- **Not** multi-region topic replication.
- **Not** a launch-route item. Post-launch, after Cache and after Runners.

---

## De-risking plan — ordered, with kill criteria

A de-risking plan without kill criteria is theatre: any number can be rationalised
after the fact. **The thresholds below are stated in advance.**

| # | Investigation | Kills the project / forces redesign if |
|---|---|---|
| 1 | **Competitor metering basis and current pricing** — stored or logical bytes, and today's real rates | our cost-comparable price is not materially under theirs → the cost thesis is dead |
| 2 | **`EventLogDO` append throughput in production** (~1 day, no new code) | sustained rate is low hundreds/sec → partitioning design changes materially; very low → firehose-adjacent ICPs are out entirely |
| 3 | **Pub/Sub, Kinesis, Event Hubs** — retention limits, replay cost, real pricing | their retention and replay story is comparable → thesis 1 needs rewriting |
| 4 | **Inbound TCP on Workers** — confirm the platform limit | if inbound TCP is possible, the gateway may be unnecessary and (a)/(b)/(c) is moot |
| 5 | **Container pool floor** — can the gateway scale to zero independently | it cannot → the gateway is a standing liability, and (b)/(c) need repricing |
| 6 | **Full unit-cost model** including DO requests, duration, storage, and Worker invocations | compute dominates storage at realistic message rates → the pricing model is wrong |
| 7 | **Real compression ratios**, with and without a trained dictionary | below ~3× on representative data → aggressive pricing is unsurvivable |
| 8 | **`kafka-protocol` crate state** + minimum API set from a real client handshake | crate is unusable → gateway sizing moves from weeks to months |
| 9 | **Design: durability, ordering, delivery, region scope** | — obligation, not a gate |
| 10 | **Design: per-subject erasure** | no workable design → permanent retention cannot be promised |
| 11 | **Design: automatic partitioning** | no workable design → the zero-config claim is dropped, and the rail thesis weakens |

Steps 1–8 are investigation costing days, not weeks, and **1 and 2 alone can end the
project cheaply.** Nothing beyond step 11 should be built until they have answers.

---

## Decision recorded

**Nothing is committed.** This brief exists to be attacked, and rev 2 is the result
of attacking it: a three-lens review found an arithmetic error in the segment-cost
argument, an internal contradiction between the pricing and compression sections, a
mischaracterised competitor, an entire missing competitor class (Pub/Sub and the
cloud-native streaming vendors), an unexamined dependency on one vendor's pricing
policy, and a cost model that counted a single line.

What survives is narrower and more honest than rev 1:

- The partition primitive is genuinely in production, and genuinely carries no
  traffic.
- The cost advantage is structural rather than promotional — **conditional on the
  competitor metering basis, which is unknown.**
- The rail thesis is the larger of the two claims and the less proven, because
  Pub/Sub may already occupy that position.
- Two hard engineering problems (automatic partitioning, the protocol gateway) and
  four undesigned areas stand between concept and product.

**The recommended next action remains a measurement, not a line of code** — and
steps 1 and 2 of the de-risking plan can end this cheaply, which is the point of
doing them first.
