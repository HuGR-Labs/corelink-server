<!-- INTERNAL — DO NOT DISTRIBUTE TO CUSTOMERS. Sales + Finance only. -->

# CoreLink pricing worksheet (internal)

**Audience.** Sales, Finance, Product. Not customer-visible. This is
the Excel-equivalent of the public calculator at
`/explanation/pricing/calculator`, plus the vendor-cost / gross-margin
side of the ledger that the public version does not expose.

**Status.** DRAFT — pending Finance sign-off on COGS rows.
Last updated 2026-06-09. Pinned to `crates/corelink-tier-selection`.

**Source of truth.**
- Customer-facing tier definitions: `apps/docs/docs/explanation/pricing/index.mdx`
- Customer-facing calculator math: `apps/docs/docs/explanation/pricing/calculator.mdx`
- Tier taxonomy in code: `crates/corelink-tier-selection/src/tier.rs`

The public site, this worksheet, and the code all share one frozen
6-tier taxonomy: **Free / Solo / Starter / Pro / Max / Enterprise**. The
four paid tiers (Solo / Starter / Pro / Max) self-serve via Stripe
Checkout; Free is instant-activation; Enterprise routes through the
inquiry form. There is no held-back internal SKU.

---

## Rate card (canonical)

The constants below are the source of truth for the public calculator,
the public pricing page, and the internal worksheet. If you change a
number here, update the public pages and the calculator in the same
commit.

At v0.1 every priced tier is **hard-capped** (no metered overage — over
quota returns `429` with an upgrade CTA). Metered overage is deferred to
v0.2.

| Tier | Base $/mo | Included storage (GB) | Included reads (GB) | Overage | BYOK add-on ($/mo) | Seats cap | Regions cap |
|---|---|---|---|---|---|---|---|
| Free | 0 | 10 | 50 | hard-cap | n/a | 3 | shared (1) |
| Solo | 15 | 50 | 500 | hard-cap | n/a | 5 | 1 |
| Starter | 35 | 150 | 1,536 | hard-cap | n/a | 10 | 1 |
| Pro | 50 | 500 | 5,120 | hard-cap | n/a | 50 | 2 |
| Max | 149 | 2,048 | 20,480 | hard-cap | 99 | 100 | 2 |
| Enterprise | quote | quote | quote | negotiated | included | unlimited | 4 |

---

## COGS model (per-tier)

Vendor cost is dominated by R2 storage and operations amortized
across the fleet. We do not pay R2 egress on cache reads. We do pay
R2 Class A (writes, $4.50 / million) and Class B (reads, $0.36 /
million) operations.

| COGS component | Unit | Unit cost | Source |
|---|---|---|---|
| R2 storage | $/GB/mo | 0.015 | Cloudflare R2 list, 2026-05 |
| R2 Class A operations (writes) | $/million | 4.50 | Cloudflare R2 list, 2026-05 |
| R2 Class B operations (reads) | $/million | 0.36 | Cloudflare R2 list, 2026-05 |
| Compute (Workers, control plane) | $/tenant/mo | 0.50 | internal load model |
| Support amortization (Free) | $/tenant/mo | 0.00 | community-only |
| Support amortization (Starter) | $/tenant/mo | 1.50 | mid-tier email queue load |
| Support amortization (Team) | $/tenant/mo | 8.00 | 1-biz-day SLO |
| Support amortization (Pro) | $/tenant/mo | 25.00 | escalation pool |
| Support amortization (Enterprise) | $/tenant/mo | 200.00 | named TAM + 1-hr SLO |

Assumptions:
- Average blob size: 256 KB (representative Bazel artifact). This
  determines the ratio between GB-stored and operations-billed.
- Per-GB of storage stored for 1 month ≈ 4,096 Class A operations
  (writes once, amortized over retention).
- Per-GB of reads ≈ 4,096 Class B operations.

Derived per-GB COGS:

| Per-GB COGS | Formula | $/GB |
|---|---|---|
| Storage (1 mo retained) | $0.015 + (4,096 × $4.50 / 1e6) | 0.033 |
| Read (1 GB served) | 4,096 × $0.36 / 1e6 | 0.0015 |

(These numbers are conservative; real per-blob averages skew larger
than 256 KB, which reduces operations-per-GB and tightens COGS by
10–25%. Finance: please re-validate against 30-day prod data when
available.)

---

## Worked tier P&L

> **Pending Finance re-model (2026-06-09).** The canonical rate card
> above is the new 6-tier launch ladder (Free $0 / Solo $15 / Starter
> $35 / Pro $50 / Max $149 / Enterprise). The worked P&L, margin
> summary, sales playbook, and Excel block **below** are still pinned to
> the superseded rate card (Starter $29 / Team $199 / internal-Pro
> $599) and overage model. They are retained as the COGS methodology
> reference only; Finance must re-run the per-tier revenue/margin
> against the new ladder before these numbers are quoted. Do not cite
> the dollar figures below as current.

### Free tier

| Line | Amount |
|---|---|
| Revenue | $0 |
| Storage COGS | 5 GB × $0.033 = $0.165 |
| Read COGS | 50 GB × $0.0015 = $0.075 |
| Compute | $0.50 |
| Support | $0.00 |
| **Total COGS** | **$0.74** |
| **Gross margin** | **−$0.74** |

Free is a customer acquisition cost. We accept ~$0.74/tenant/mo of
negative margin. Hard caps at 5 GB / 50 GB reads keep this bounded.
At 10,000 Free tenants, the line item is ~$7,400/mo — material but
manageable.

### Starter tier

Representative customer at included quotas (50 GB stored, 500 GB
reads, no overage):

| Line | Amount |
|---|---|
| Revenue | $29.00 |
| Storage COGS | 50 × $0.033 = $1.65 |
| Read COGS | 500 × $0.0015 = $0.75 |
| Compute | $0.50 |
| Support | $1.50 |
| **Total COGS** | **$4.40** |
| **Gross profit** | **$24.60** |
| **Gross margin %** | **84.8%** |

Starter at full quota is a high-margin SKU. Below-quota customers are
slightly higher margin (the included quotas are a soft ceiling, not a
floor on COGS). Customers in overage are slightly *higher* margin per
GB than included quota (overage is priced 6× COGS on storage and 27×
COGS on reads).

### Team tier

Representative customer at included quotas (500 GB stored, 5,120 GB
reads, no BYOK):

| Line | Amount |
|---|---|
| Revenue | $199.00 |
| Storage COGS | 500 × $0.033 = $16.50 |
| Read COGS | 5,120 × $0.0015 = $7.68 |
| Compute | $0.50 |
| Support | $8.00 |
| **Total COGS** | **$32.68** |
| **Gross profit** | **$166.32** |
| **Gross margin %** | **83.6%** |

With BYOK add-on at $99/mo, COGS adds ~$5/mo of KMS-call overhead and
TAM time; net add to gross margin ≈ +$94/mo. BYOK is a high-margin
attach.

### Pro tier (internal SKU, not yet launched)

Representative customer at included quotas (2,048 GB stored, 20,480 GB
reads, BYOK included):

| Line | Amount |
|---|---|
| Revenue | $599.00 |
| Storage COGS | 2,048 × $0.033 = $67.58 |
| Read COGS | 20,480 × $0.0015 = $30.72 |
| Compute | $0.50 |
| Support | $25.00 |
| BYOK overhead | $5.00 |
| **Total COGS** | **$128.80** |
| **Gross profit** | **$470.20** |
| **Gross margin %** | **78.5%** |

Pro is the contemplated bridge SKU between Team and Enterprise. The
question for Finance + Product is whether the customer-discovery data
supports a $599 SKU between Team's $199 and Enterprise's negotiated
floor (~$2,000–$5,000/mo observed in pilots). Hold internal until
that question is answered.

### Enterprise tier

Negotiated, but the worked floor for a multi-region BYOK customer at
~1 TB/mo writes and ~20 TB/mo reads with all 4 regions enabled:

| Line | Amount |
|---|---|
| Revenue (floor, observed) | $3,000.00 |
| Storage COGS (3 TB stored, 4-region replication = 12 TB) | 12,288 × $0.033 = $405.50 |
| Read COGS | 20,480 × $0.0015 = $30.72 |
| Compute | $5.00 |
| Support (TAM) | $200.00 |
| BYOK overhead | $20.00 |
| **Total COGS (floor)** | **$661.22** |
| **Gross profit (floor)** | **$2,338.78** |
| **Gross margin % (floor)** | **77.9%** |

Enterprise margin is structurally lower than Team because of
multi-region replication COGS and TAM allocation, but absolute gross
profit per account is 10–20× a Team account. The floor price is the
relevant negotiation anchor; deals above the floor are pure
incremental margin.

---

## Margin summary

| Tier | Representative monthly revenue | Representative monthly COGS | Gross margin % |
|---|---|---|---|
| Free | $0 | $0.74 | −∞ (loss leader) |
| Starter | $29 | $4.40 | **84.8%** |
| Team | $199 | $32.68 | **83.6%** |
| Pro (internal) | $599 | $128.80 | **78.5%** |
| Enterprise (floor) | $3,000 | $661.22 | **77.9%** |

Target gross margin: **75% blended** across the paid book. We are
well above target on paid SKUs; the Free-tier drag pulls blended down
proportional to Free-tenant volume.

---

## Sales playbook

### Who lands on Starter

Solo developers, two-person startups, three-person CI pilots. Annual
contract value: ~$350. Self-serve through Stripe Checkout. No Sales
touch.

### Who lands on Team

20–200 engineer teams, single-region. ACV: ~$2,400. Self-serve
through Stripe Checkout in the majority case; Sales touch on
inbounds with annual procurement.

### Who lands on Enterprise

200+ engineer orgs, multi-region, regulated industry, or BYOK
requirement. ACV: $36,000+. Sales-led with TAM assignment above ACV
$60,000.

### Common upsell paths

- **Free → Starter** when a customer hits the 5 GB / 50 GB cap (we
  send an in-product notification at 70% and 100% of cap).
- **Starter → Team** when read overage exceeds the Team flat fee
  delta (~$170/mo of overage on Starter ≈ Team flat). Calculator
  surfaces this break-even.
- **Team → Enterprise** when the customer triggers BYOK, multi-region
  (>2), or seat cap. Routes to inquiry form per WI-S19-005.

### Common downsell paths

- **Team → Starter** for a customer whose CI volume drops (e.g.,
  reduced team headcount). Allow once per 12 months; route through
  Customer Success.
- **Enterprise → Team** is rare and indicates a misclassified deal;
  route to deal review.

---

## Discount authority

| Discount | Authority required |
|---|---|
| 0–10% | AE |
| 11–20% | Sales manager |
| 21–30% | VP Sales |
| 31%+ | CFO + CEO |

Annual prepay: 10% list discount (no additional authority required).
Multi-year prepay: negotiated.

---

## Worksheet (Excel-equivalent)

Paste into your spreadsheet of choice. The named ranges are referenced
in the formula block below.

```
# Inputs
T                       = 1
S_w_gb                  = 100
S_r_tb                  = 4
byok                    = "none"   # "none" | "aws" | "gcp" | "azure"
h                       = 0.80
R                       = 1

# Rate card
base_free               = 0
base_starter            = 29
base_team               = 199
base_pro_internal       = 599
incl_storage_free       = 5
incl_storage_starter    = 50
incl_storage_team       = 500
incl_storage_pro        = 2048
incl_reads_free         = 50
incl_reads_starter      = 500
incl_reads_team         = 5120
incl_reads_pro          = 20480
rate_storage_starter    = 0.20
rate_storage_team       = 0.15
rate_storage_pro        = 0.10
rate_reads_starter      = 0.04
rate_reads_team         = 0.03
rate_reads_pro          = 0.02
byok_fee_team           = 99
byok_fee_pro            = 0      # included
byok_fee_enterprise     = 0      # included

# COGS unit prices
cogs_storage_per_gb     = 0.033
cogs_read_per_gb        = 0.0015
cogs_compute_per_tenant = 0.50
cogs_support_free       = 0.00
cogs_support_starter    = 1.50
cogs_support_team       = 8.00
cogs_support_pro        = 25.00
cogs_support_enterprise = 200.00
cogs_byok_overhead      = 5.00

# Derived
S_w_billable_gb         = S_w_gb * IF(R<=1, 1, IF(R=2, 2, IF(R=3, 3, 4)))
S_r_billable_gb         = S_r_tb * 1024

# Per-tier revenue (Free is $0; Enterprise is quote)
rev_starter = base_starter * T
            + MAX(0, S_w_billable_gb - incl_storage_starter * T) * rate_storage_starter
            + MAX(0, S_r_billable_gb - incl_reads_starter   * T) * rate_reads_starter

rev_team    = base_team * T
            + MAX(0, S_w_billable_gb - incl_storage_team * T) * rate_storage_team
            + MAX(0, S_r_billable_gb - incl_reads_team   * T) * rate_reads_team
            + IF(byok = "none", 0, byok_fee_team)

# Per-tier COGS
cogs_starter = S_w_billable_gb * cogs_storage_per_gb
             + S_r_billable_gb * cogs_read_per_gb
             + cogs_compute_per_tenant * T
             + cogs_support_starter * T

cogs_team    = S_w_billable_gb * cogs_storage_per_gb
             + S_r_billable_gb * cogs_read_per_gb
             + cogs_compute_per_tenant * T
             + cogs_support_team * T
             + IF(byok = "none", 0, cogs_byok_overhead)

# Margin
margin_starter = rev_starter - cogs_starter
margin_team    = rev_team - cogs_team
margin_pct_starter = margin_starter / rev_starter
margin_pct_team    = margin_team / rev_team
```

---

## Trace

- spec contract S-19 §6.1 (tier taxonomy)
- WI-S19-004 §6.1 (tier baseline)
- WI-S19-005 (Enterprise routing)
- WI-S20-008 §2.1.2 Post 5 (economics framing)
- `crates/corelink-tier-selection/src/tier.rs`
- Cloudflare R2 list pricing 2026-05
