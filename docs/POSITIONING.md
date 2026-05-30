# CoreLink — Product Positioning & Anti-Reductions

> **For any future contributor, agent, or context-fresh assistant: read
> this BEFORE you propose a market angle, a pivot, a feature, or a
> "tldr" of what CoreLink is. The founder has had to re-teach the
> product framing multiple times across sessions. This doc exists to
> stop that.**

## What CoreLink IS (in one sentence)

**A multi-tenant content-addressable cache + storage governance platform
running on Cloudflare's edge stack, sold self-serve at SMB prices,
using R2's zero-egress as a structural cost moat against enterprise
incumbents.**

## What CoreLink IS (in one paragraph)

71 Rust crates implementing CAS + Action Cache + multi-region replication
+ HMAC tenant isolation + BYOK with key rotation + Argon2id PAT auth +
Merkle RFC 6962 audit chain + GDPR DSR + DPA enforcement + dual-approval
+ billing aggregator + Stripe lifecycle + Bazel REAPI v2 + Turborepo v8
protocol bridges + chaos scheduler + SLO + Slack/Terraform/status-page
integrations — all hosted on the founder's Cloudflare account
(Workers + Durable Objects + Containers + R2 + D1), exposed as a single
SaaS product with tiered self-serve pricing. The same code that ships
the cache also ships every governance control a regulated buyer would
ask for. 47 TLA+ specs + 264 proptest blocks. Verified live in prod:
CAS + AC persist to real R2; PAT auth + tenant isolation work E2E.

## Anti-reductions (DO NOT use these framings)

| ❌ Wrong framing | ✅ Correct framing |
|---|---|
| "It's a build cache" | Bazel/Turbo are ONE protocol bridge each on top of the CAS+AC primitive. The product is the platform. |
| "It competes with R2 / S3 / B2" | R2 is the storage substrate. CoreLink is the layer ON TOP. Customers don't ever touch raw R2. |
| "It's self-hostable" | It's a SaaS on the founder's Cloudflare account. Customers sign up, get a PAT, point their tool at the endpoint. |
| "It's an enterprise sales product" | The founder's deliberate strategy is self-serve SMB at low ACV with high margin. Enterprise tier exists but is not the motion. |
| "It's an audit log SaaS" | The Merkle audit chain is one feature. EU AI Act / ISO 42001 compliance is a vertical wedge, not the product. |
| "It's an agent state store" | Mem0/Zep/Letta/LangMem own that market. CoreLink is not for agent runtime state. |
| "It's a Dropbox for builds" | Reductive by 70%. Dropbox has no CAS, no protocol bridges, no audit chain, no tenant isolation, no replication coordinator. |

## Strategic positioning

### The chosen play: margem pequena × escala grande

- **Self-serve SMB pricing, undercut enterprise vendors 4-6x.**
- **Structural cost moat: R2 zero-egress.** Competitors on S3 backend
  pay $0.09/GB egress; CoreLink pays $0. This is architectural, not
  marginal. It is the reason undercut + good margin is mathematically
  possible.
- **Pricing tiers (illustrative — not yet shipped):**
  - Free: 10GB, 1 user, public projects, OSS use
  - Solo: $5/mo, 100GB, unlimited transfer
  - Team: $30/mo, 1TB, multi-tenant, audit
  - Org: $150/mo, 10TB, BYOK, multi-region, SLA
  - Enterprise: custom (DPA, residency, compliance custom)
- **Distribution: dev community + content marketing, NOT sales calls.**
  GitHub, HN, dev.to, podcast appearances, OSS strategy. Self-serve
  signup. Impeccable DX. A $30/mo customer churns if onboarding
  takes > 5 minutes.

### Why this play works HERE specifically

| Lever | CoreLink | Typical competitor |
|---|---|---|
| Storage backend | R2 ($0.015/GB-mo, **$0 egress**) | AWS S3 ($0.023/GB-mo, **$0.09/GB egress**) |
| Compute | CF Workers + Containers (cents/hour) | Lambda / EC2 / Fargate |
| State | DO + D1 ($/million requests) | DynamoDB / RDS |
| Infra cost per 10-dev customer with 100GB cache + 500GB/mo egress | ~$5-10/mo | ~$30-100/mo |
| Possible price at 80-90% gross margin | $30-50/mo | $200/mo (Depot Startup) |
| Required margin | 80-90% | Lower (egress eats it) |

The same customer hits 80-90% gross margin at $30 with CoreLink, or at
$200 with Depot. That is not luck — that is a different infra stack.

### Distribution risks (the real hard part)

The product is built; the GTM is the hill. Solo founder + SMB self-serve
demands:

1. **Brand + content presence.** No GitHub stars / HN posts / dev.to
   articles = invisible to the SMB buyer.
2. **DX as a differentiator.** Vercel/Stripe-grade docs, dashboards,
   onboarding.
3. **OSS strategy.** Probably open-source the client SDK or a Bazel
   adapter to seed attention without giving away the platform.
4. **Free tier generous enough to be useful.** Vercel hobby tier as
   the reference.
5. **Self-serve everything.** Signup, checkout, upgrade, downgrade.
   Zero sales calls for the SMB tiers.

## Anti-pivot guidance

When proposing a market or strategy change, validate against:

1. Does it preserve the **structural cost advantage** (R2 zero-egress)?
2. Does it stay in **self-serve SMB** pricing range (or have a clear path)?
3. Does it leverage **what's already built** (the 71 crates), not require
   wholesale rewrites?
4. Is the founder's chosen play (margem pequena × escala grande)
   compatible with the new framing?

If any answer is "no", surface that explicitly. Don't quietly drift
toward enterprise pivots or different product categories.

## Reading the codebase to recalibrate

If your understanding feels off, run:

```bash
ls crates/                  # 71 crates, full inventory
ls crates/corelink-container/src/routes/   # protocol surfaces
cat README.md               # founder's framing
```

The crate names tell the truth. `corelink-dpa-acceptance`,
`corelink-erasure-attestation`, `corelink-dual-approval`,
`corelink-replication-coordinator`, `corelink-byok` — these are not
features a "cache" has. They are an enterprise governance platform's
spine, shipped on day one.

## History of this doc

Written 2026-05-30 after the founder said *"Atualize a porra da sua
memoria, e a documentacao do corelink pra nunca mais eu ter que te
reensinar o que e essa porra"* — at the end of a session where the
assistant had reduced CoreLink to "build cache", then "agent state
cache", then "audit log SaaS", then incorrectly assumed it was
self-hostable / competed with R2. Each reduction was corrected by the
founder. This doc + the matching `corelink_product_truth.md` memory
entry are the durable record so the lesson does not have to be
re-taught.
