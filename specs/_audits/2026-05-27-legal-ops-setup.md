---
id: "AUDIT-2026-05-27-LEGAL-OPS-SETUP"
type: "audit"
doc_status: "ACTIVE"
audit_status: "DRAFT"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "strategy", "legal", "ops", "solo-founder", "brazilian-founder", "pre-revenue"]
---

# Legal + ops minimum-viable setup (solo Brazilian founder, pre-revenue SaaS)

> **Scope.** Concrete, $-priced legal and operational stack for CoreLink — solo Brazilian founder (Gustavo Schneiter), pre-revenue, B2B SaaS targeting US/EU buyers, multi-tenant data (GDPR + LGPD obligations). All pricing cited inline; sources at the bottom of each section. Dates of fetch: 2026-05-27.
>
> **Bias.** Cheapest path that does not block (a) signing a US/EU enterprise pilot, (b) accepting first paid customer, or (c) passing a basic InfoSec questionnaire. Defer everything else.
>
> **Out of scope.** SOC 2 attestation (separate Wave-32 track), security questionnaires (separate audit), Drata setup (already wired per Wave-32 sign-off).

---

## §1 Legal docs comparison (Privacy / Terms / Cookie / DPA / Sub-processors / AUP)

### §1.1 Provider matrix

| Provider | Tier | Price (2026) | Docs covered | Auto-update on law change | Notes for solo founder |
|---|---|---|---|---|---|
| **Termly** | Free | $0 | Privacy Policy only | No (free tier) | Branding watermark; insufficient for B2B |
| Termly | Starter | $10/mo annual | PP, ToS, Cookie, Disclaimer (limited generators) | Yes | Single-site license |
| Termly | Pro+ | $15/mo annual | All 10 generators, unlimited edits | Yes | Best fit if Termly chosen |
| Termly | Monthly | $30/mo | Same as Pro+ | Yes | Avoid — annual is 50% cheaper |
| **Iubenda** | Free / Starter | €0 | PP only (basic) | No | Branding watermark |
| Iubenda | Pro / Advanced | from €27/yr per site (~$30/yr) | PP + Cookie + ToS, multi-language, 2,400+ clauses, API | Yes | EU-rooted, strongest GDPR pedigree |
| Iubenda | Ultimate | $99.99/mo | Unlimited services, no branding, analytics | Yes | Overkill pre-revenue |
| **Termageddon** | Single license | $12/mo OR $119/yr | PP, ToS, Cookie, Disclaimer, EULA | Yes (founder is a privacy attorney; tracks US/CA/EU/UK/AU) | Best price-to-coverage; no DPA |
| **Common Paper** | Free (CC BY 4.0) | $0 | CSA (MSA replacement), DPA, SLA, Software License Agreement | N/A (static templates) | Lawyer-drafted, plain-language, widely adopted by B2B SaaS — the **right primitive for DPA** |
| Lawyer-drafted | One-shot | $300–$2,000 | Custom Privacy + ToS + DPA bundle | No (you re-pay on law change) | Cheapest at launch, most expensive over time |

### §1.2 What CoreLink actually needs (and where to source it)

| Document | Why required | Recommended source | Cost |
|---|---|---|---|
| **Privacy Policy** | GDPR Art. 13/14, LGPD Art. 9, CalOPPA | Termageddon ($119/yr) — auto-updates on law change | $119/yr |
| **Terms of Service** | Limit liability, define payment, dispute resolution | Termageddon (bundled in $119/yr) | $0 incremental |
| **Cookie Policy** | GDPR / ePrivacy; EU visitors trigger this | Termageddon (bundled) + consent banner via own code or Iubenda Cookie Solution free tier | $0 incremental |
| **DPA (Data Processing Agreement)** | GDPR Art. 28 (customer-facing — you are processor for tenant data) | **Common Paper standard DPA** (free, CC BY 4.0) — sign as PDF or click-through | $0 |
| **Sub-processor list** | DPA §3 requires public list; convention is `/sub-processors` page | Self-hosted page on docs site; mirror Stripe / Clerk / Cloudflare / Neon / etc. format | $0 |
| **Acceptable Use Policy** | Limit liability for tenant misuse (spam, abuse, malware) | Cloudflare-style template adapted from Termly / iubenda free templates | $0 |
| **MSA / Cloud Service Agreement** | For pilot customers signing a real contract (not click-through) | **Common Paper CSA** (free) | $0 |
| **Order Form / SLA** | When pilot wants uptime commitment | **Common Paper SLA** (free) | $0 |

### §1.3 Recommendation

- **Termageddon $119/yr** for Privacy + ToS + Cookie + Disclaimer (auto-updates on law change is the killer feature; founder is an ABA ePrivacy Committee chair). [Termageddon Pricing](https://termageddon.com/pricing/)
- **Common Paper free standards** for DPA + CSA (MSA) + SLA. These are lawyer-drafted, plain-language, CC BY 4.0, and already adopted by 1000s of B2B SaaS vendors — your customers will recognize them. [Common Paper DPA](https://commonpaper.com/standards/data-processing-agreement/) | [Common Paper CSA](https://commonpaper.com/standards/cloud-service-agreement/)
- **Self-host sub-processor list** at `corelink-docs.humangr.com/sub-processors`.
- **Skip a paid lawyer review until first $50k pilot contract.** Common Paper + Termageddon both pass typical B2B procurement; a $500-$1,500 lawyer review only adds value when terms get negotiated.

**Total legal-docs cost at launch: $119/yr.** (Iubenda at €27/yr is competitive, but does not bundle ToS at that tier and lacks the auto-update breadth of Termageddon.)

---

## §2 Entity + banking (Brazilian solo founder selling US/EU SaaS)

### §2.1 Critical constraint: no US-Brazil income tax treaty

There is **no income tax treaty** between the US and Brazil. Both jurisdictions can tax the same income. Brazil **reintroduced a 10% withholding tax on dividends to non-residents effective 2026-01-01** (Law 15,270/2025), eliminating the historical 0%-dividend advantage. Mitigation is via the US Foreign Tax Credit (Form 1116) for IRRF paid in Brazil. [Brazil-USA Tax Treaty — ZS Advogados](https://zsassociados.com/blog/en-2026-02-05-tax-treaty-brazil-usa/) | [countrytaxcalc — No Treaty Guide 2026](https://www.countrytaxcalc.com/tax-guides/usa/brazil-us-tax-treaty-guide-2026/)

This means **entity choice has real money implications** — it is not just ceremony.

### §2.2 Entity options compared

| Option | Setup cost | Ongoing | Tax exposure (Brazilian founder) | Investor-ready? | Mercury / Stripe friendly? |
|---|---|---|---|---|---|
| **Brazilian LTDA only** | R$1-2k | Low (Simples Nacional possible up to ~R$4.8M revenue) | Single layer (Brazil), but 15% withholding on profit remittance to US partners + 15%+10% on royalties | No — US VCs require Delaware C-Corp | No (Stripe Brasil possible; no USD account) |
| **Delaware LLC** (pass-through) | $399–$597 via Doola / $500 via Stripe Atlas | $100/yr registered agent + state franchise tax (~$300/yr DE) | Pass-through to founder personally; Brazilian RFB treats as transparent → personal IRPF (up to 27.5%) on worldwide income; Form 5472 + 1120 filing required ($800-2k/yr CPA) | Limited — convertible to C-Corp before priced round but ugly | Yes |
| **Delaware C-Corp** (via Stripe Atlas) | $500 one-time + state fees included; $100/yr registered agent renewal | $300/yr DE franchise + $500-2k/yr CPA | Two layers: 21% US federal corp tax, then withholding on dividends to BR (currently 30% non-treaty rate, brutal) — but **retain earnings in C-Corp** until raise / acquisition and the double-tax is deferred. QSBS eligibility a real upside on exit | Yes — required for YC, US VCs, SAFE notes | Yes — Stripe Atlas IS Stripe, native integration |
| **Brazilian LTDA + Delaware C-Corp** (parent-sub or sister) | $500 + R$2k + intercompany agreement legal ($1-3k) | Both maintenance costs | Most complex; transfer pricing required; best if you have Brazilian employees / costs to push into BR sub | Yes | Yes |

### §2.3 Recommendation for CoreLink

**Phase 1 (now → first $10k MRR or pilot signed): Delaware C-Corp via Stripe Atlas.**

Reasoning:
1. **$500 one-time, $100/yr renewal** is the cheapest path with US banking + Stripe in one motion. [Stripe Atlas](https://stripe.com/atlas)
2. C-Corp is investor-default. You will not have to re-paper if you raise even a small SAFE round. (LLC → C-Corp conversion costs $5-15k legal and triggers tax events.)
3. **2,500 USD in Stripe credits** included covers ~1 year of Stripe fees at low MRR.
4. Atlas handles EIN + 83(b) + bylaws + founders' stock — solo founder cannot screw up the corporate governance fundamentals.
5. C-Corp retained earnings strategy: do **not** pay yourself dividends. Pay yourself a salary as a US-source contractor invoice from Brazil (deducted as expense in C-Corp, taxed in BR as personal income, no withholding on services with proper documentation). Defer the double-tax question until exit/acquisition.
6. **No LTDA needed yet** — there are no Brazilian employees, no Brazilian customers, no local revenue. Re-evaluate if you hire a BR employee or sign a BR enterprise customer (R$ revenue → CNPJ usually required for invoice).

**Defer:**
- Brazilian LTDA (only if hiring BR or invoicing BR customers in R$).
- LLC alternative via Doola/Firstbase — only attractive if explicitly bootstrapping forever (no raise). [Stripe Atlas vs Firstbase vs Doola comparison](https://www.globalsolo.global/blog/stripe-atlas-vs-firstbase-vs-doola-pricing-comparison-2026)

### §2.4 Banking

| Bank | Brazilian founder eligible? | Fees | Notes |
|---|---|---|---|
| **Mercury** | Yes (Brazil not on prohibited list as of 2026-05); requires US entity, EIN, government ID, US business intent | $0/mo; free USD wires; physical address required (not P.O. box) | **Default choice.** Native Stripe Atlas handoff. [Mercury eligibility](https://support.mercury.com/hc/en-us/articles/28770467511060-Eligibility) |
| Wise Business | Yes but KYC tightened in 2026; harder approval for foreign-owned LLCs since debit card launch; forces $31/mo plan for routing+account numbers | $31/mo for full features; ~0.5% FX | Good as **secondary** for cheap USD→BRL transfers to fund yourself |
| Brazilian bank (Nubank PJ, Itaú PJ) | Yes (requires CNPJ → requires LTDA first) | Variable | Only if LTDA exists |
| Relay | Yes, similar to Mercury | $0/mo, multi-account | Mercury alternative if Mercury rejects |

**Recommendation:** Open Mercury during the Stripe Atlas flow (Atlas pre-fills the Mercury application). Add Wise Business 1-3 months later for cheap USD → BRL personal withdrawals.

---

## §3 Payment processing (Stripe direct vs MoR)

### §3.1 Decision tree

| Scenario | Recommended | Why |
|---|---|---|
| **Pre-revenue → $10k MRR**, US/EU SMB customers self-serve | **Lemon Squeezy (Stripe Managed Payments)** — 5% + $0.50 flat, MoR handles sales tax/VAT/GST globally | Avoid the **EU VAT MOSS filing** trap and the US state sales-tax-nexus trap; with MoR you do not register/file anywhere |
| **$10k → $100k MRR**, growing enterprise mix | **Stripe direct + Stripe Tax** at 0.5% on top of 2.9%+$0.30 | Lower fees + better dispute control; you now have the revenue to afford a Sales Tax CPA / Anrok for the messy states |
| **Enterprise pilots invoiced in USD** (>$10k ACV) | **Stripe Invoicing** + wire/ACH; no MoR needed (B2B usually self-accrues) | MoR fees stupid on big invoices |

### §3.2 Cost comparison ($10k MRR scenario)

| Approach | Effective fee | Annual cost (@$120k revenue) | What you also do yourself |
|---|---|---|---|
| Lemon Squeezy / Stripe Managed Payments (MoR) | 5% + $0.50/txn ≈ 5.4% | ~$6,500 | Almost nothing — they file all tax |
| Stripe direct + Stripe Tax + ad-hoc CPA | 2.9% + $0.30 + 0.5% Tax = ~3.7% | ~$4,500 + ~$2,000 CPA = ~$6,500 | Register in nexus states, monitor thresholds, respond to notices |
| Stripe direct, no tax automation | 2.9% + $0.30 = ~3.2% | ~$3,900 | You eventually owe back-taxes + penalties to ≥1 state. Don't. |

At sub-$10k MRR the difference is dollars — **the operational simplicity of MoR is worth it.** Switch to Stripe direct at the same time you hire your first contractor / can afford an hour of CPA time.

[Lemon Squeezy 2026 update](https://www.lemonsqueezy.com/blog/2026-update) | [Stripe vs Lemon Squeezy](https://designrevision.com/blog/stripe-vs-lemonsqueezy) | [Stripe Tax pricing](https://stripe.com/tax/pricing)

### §3.3 Recommendation

- **Launch with Lemon Squeezy** (Stripe Managed Payments under the hood since the 2024 acquisition).
- Cut over to **Stripe direct + Stripe Tax (0.5%)** at $10k MRR or first enterprise pilot, whichever first.
- For pilots > $5k ACV: send Stripe Invoices (no MoR) directly.

---

## §4 Support stack (solo-founder tier)

| Layer | Tool | Price | Notes |
|---|---|---|---|
| Demo booking | **Cal.com Free** | $0 | Unlimited bookings, Stripe payments, Cal Video. Branding on booking page (acceptable pre-revenue). [Cal.com pricing](https://cal.com/pricing) |
| Inbound support (first 6 months) | `support@corelink.humangr.com` → Gmail or Fastmail | $6/mo (Fastmail) or $0 (Gmail+Cloudflare Email Routing) | Plain email scales to ~20 active users / day |
| Inbound support (post-launch, 5+ paying) | **Plain Foundation** | $35/seat/mo (solo = $35/mo) | Best B2B fit; Ari AI included; clean threading; integrates with Slack / Linear. [Plain pricing](https://www.plain.com/pricing) |
| Status page | **Better Stack Free** | $0 (3 GB logs, 1 status page, 10 monitors) | Already in stack per memory; status page is `/status.corelink.humangr.com` style |
| Issue tracker (public bug reports) | **GitHub Issues** in `humangr-labs/corelink-server` | $0 | Already exists |
| Roadmap (public) | Same GitHub Issues + a labeled view | $0 | Don't pay for Productboard / Canny pre-revenue |
| Docs / help center | **Docusaurus** at `corelink-docs.humangr.com` | $0 | Already deployed per Wave-32 |

### §4.1 What to defer

- **Intercom / HelpScout / Zendesk**: $89/mo entry, $33k/yr typical SMB spend. Not justified <$10k MRR. [Intercom pricing](https://www.intercom.com/pricing)
- **Customer.io / Loops / Resend**: defer email automation until lifecycle nurture is a real bottleneck. Use a single transactional sender (Resend free tier: 100/day, 3k/mo) for now.
- **Calendly Premium / SavvyCal Pro**: Cal.com Free is feature-equivalent for solo.

---

## §5 Analytics + monitoring stack

| Layer | Tool | Price | Tier |
|---|---|---|---|
| **Web analytics** (marketing site) | **Plausible hosted Starter** | $9/mo (annual) | 10k pageviews, 1 site, GDPR-friendly. [Plausible pricing](https://plausible.io/) |
| Alternative (cheaper) | Self-host Plausible CE on existing Cloudflare/VPS | ~$0 incremental | Skip if no DevOps capacity |
| **Product analytics** | **PostHog Cloud Free** | $0 | 1M events/mo, 5k recordings, feature flags, surveys. [PostHog pricing](https://posthog.com/pricing) |
| **Error monitoring** | **Sentry Developer** | $0 | 5k errors/mo, 1 user, 30d retention. [Sentry pricing](https://sentry.io/pricing/) |
| **Logs** | **Axiom Free** | $0 | 500 GB/mo ingest, 30d retention — most generous free tier in 2026. Ship Cloudflare Workers logs here. |
| Alternative | Better Stack Free | $0 | Only 3 GB logs / 3d retention — too small for prod |
| **Uptime + status page** | Better Stack Free | $0 | Already in stack |
| **APM / traces** | Defer | $0 | Sentry has traces; add Honeycomb / Datadog post-revenue |

**Total monthly analytics+monitoring at launch: $9/mo (Plausible).** Everything else on free tier.

When you outgrow:
- **PostHog free → paid:** at ~1M events/mo. Pricing is $0.00005/event after. A 5M event month ≈ $200.
- **Sentry free → Team ($26/mo):** when you need >1 user dashboard access or >5k errors/mo.
- **Axiom free → paid ($25/mo):** at 500 GB/mo ingest. Realistically not for >12 months.

---

## §6 Monthly cost ladder

All prices in USD, monthly equivalent (annual plans amortized).

### §6.1 Pre-launch (Month -1 → 0): everything you pay before first customer

| Item | Cost |
|---|---|
| Stripe Atlas (one-time) | $500 (treat as Month-0 lump) |
| Termageddon | $9.92/mo ($119/yr) |
| Domain (humangr.com already owned; corelink.humangr.com subdomain) | $0 incremental |
| Cloudflare Workers Paid plan (already on per Wave-32) | $5/mo |
| Plausible Starter (hosted) | $9/mo |
| Mercury banking | $0 |
| Cal.com Free | $0 |
| PostHog / Sentry / Axiom (all free tiers) | $0 |
| Better Stack Free (status + uptime) | $0 |
| **Monthly recurring (steady-state)** | **~$24/mo** |
| **One-time (Month 0 only)** | $500 + DE franchise tax ~$300/yr prorated |

### §6.2 Month 1 (launched, first checkout live)

| Add | Cost |
|---|---|
| Lemon Squeezy / Stripe Managed Payments | $0 base; 5% + $0.50 per txn (only on revenue) |
| Resend transactional email (free tier 3k/mo) | $0 |
| **Monthly recurring** | **~$24/mo** + 5.4% of revenue |

### §6.3 Month 3 (some paying customers, ~$2-5k MRR)

| Add | Cost |
|---|---|
| Resend Pro (50k emails) | $20/mo |
| Plain Foundation (single seat) — if support volume warrants | $35/mo |
| Notion Plus (if Linear/GitHub Issues not enough) | $10/mo |
| **Monthly recurring** | **~$70-90/mo** + 5.4% of revenue |

### §6.4 Month 6 ($10k MRR target)

| Add / Swap | Delta |
|---|---|
| Swap Lemon Squeezy → Stripe direct + Stripe Tax | Fees drop 5.4% → 3.7%; saves ~$200/mo at $10k MRR |
| Sentry Team plan (more users, longer retention) | +$26/mo |
| First CPA engagement (Form 5472, 1120, BR personal IRPF) | ~$167/mo amortized ($2k/yr) |
| E&O + Cyber liability (see §8 — Founder Shield, Embroker, Vouch) | ~$125/mo ($1,500/yr starter policy) |
| Anrok or Stripe Tax Complete (if outgrowing 0.5%) | Conditional |
| **Monthly recurring** | **~$400-500/mo** + 3.7% of revenue |

### §6.5 12-month total burn estimate (excluding salary)

- Months 1-3: ~$24/mo × 3 = $72 + Atlas $500 + DE franchise $300 = **~$870**
- Months 4-6: ~$85/mo × 3 = $255
- Months 7-12: ~$450/mo × 6 = $2,700
- **Year-1 ops/legal/tooling burn: ~$3,800** (excluding payment processing fees, salary, infra at scale)

For comparison: a single hour of a US tech-startup lawyer (~$400-650/hr) costs more than your Year-1 legal stack at launch.

---

## §7 RECOMMENDED setup checklist (1-page actionable)

Order matters. Do top-down.

### §7.1 Week 1: legal entity
- [ ] Sign up at [stripe.com/atlas](https://stripe.com/atlas), select **Delaware C-Corp**, pay $500.
- [ ] Provide passport, BR CPF, US business intent narrative (CoreLink description), home address (residential, no P.O. box).
- [ ] Sign incorporation docs via Atlas portal. Wait 1-2 days for EIN.
- [ ] **File 83(b) election within 30 days** — Atlas walks you through it. Missing this is the most expensive solo-founder mistake.
- [ ] Open Mercury account during Atlas flow (pre-filled application).

### §7.2 Week 2: banking + payments
- [ ] Verify Mercury account, request debit card.
- [ ] Sign up for Stripe (use Atlas-created entity); enable Lemon Squeezy product (now under Stripe Managed Payments umbrella).
- [ ] Open Wise Business (secondary, for cheap USD→BRL transfers to yourself).
- [ ] Configure Stripe → Mercury payouts (default daily).

### §7.3 Week 2: legal docs
- [ ] Buy Termageddon $119/yr; generate Privacy Policy, Terms of Service, Cookie Policy, Disclaimer. Embed in `apps/docs` footer (already wired per Wave-32 — replace 404 placeholders).
- [ ] Adopt Common Paper standards: download CSA + DPA + SLA PDFs; host at `corelink-docs.humangr.com/legal/`.
- [ ] Build `/sub-processors` page. Initial list (per Wave-32 prod): Cloudflare (CDN, Workers, R2, KV, D1, DO), Clerk (auth), Stripe (payments), Mercury (banking — not customer data so optional), Resend (email), PostHog (analytics), Sentry (errors), Axiom (logs).
- [ ] Adopt AUP from Cloudflare-style template; link from ToS.

### §7.4 Week 2: stack
- [ ] Plausible Starter $9/mo, add tracking script to docs + app + admin.
- [ ] PostHog Cloud Free, snippet in `apps/app` only.
- [ ] Sentry Developer Free, DSN in all 3 apps + workers.
- [ ] Axiom Free, ship Cloudflare Workers logs via Logpush.
- [ ] Cal.com Free, single 30-min "Demo / Sandbox Tour" event linked from landing + footer.
- [ ] Resend Free, configure transactional template, point auth + billing emails through it.

### §7.5 Week 3: launch
- [ ] Status page on Better Stack at `corelink-status.humangr.com` (already wired per Wave-32 baseline).
- [ ] Public roadmap = GitHub Project view, link from docs nav.
- [ ] `support@corelink.humangr.com` mailbox (Cloudflare Email Routing → Gmail).
- [ ] Pilot intake form at `corelink-docs.humangr.com/pilot/apply` (already exists per W32 sign-off).

### §7.6 Trigger-based (not date-based)
- [ ] **First pilot signature:** swap click-through ToS for Common Paper CSA + DPA; 1-shot lawyer review ($300-$800 on UpCounsel / Atrium / Lexion) only if customer redlines.
- [ ] **$5k MRR:** switch to Plain Foundation ($35/mo) for support.
- [ ] **$10k MRR:** cut over Lemon Squeezy → Stripe direct + Stripe Tax. Engage US CPA ($2k/yr) for Form 5472 + 1120. Engage BR contador for personal IRPF declaration of US contractor income. Buy E&O + Cyber ($1,500/yr starter).
- [ ] **$50k ARR or first enterprise:** SOC 2 Type I via Drata (already configured per W32); consider Vanta as alternative.
- [ ] **First SAFE / priced round:** upgrade legal to Cooley / Goodwin / Orrick free startup tier; review cap table; D&O insurance.

---

## §8 What to defer (and the trigger to re-evaluate)

| Item | Why defer pre-revenue | Trigger to revisit |
|---|---|---|
| E&O / Cyber insurance | $1,500-$5,000/yr; no claims surface area without paying customers + signed contracts | First enterprise pilot OR $50k ARR OR contract requires it |
| D&O insurance | Protects board; you have no board | First priced round / first independent director |
| SOC 2 audit (Type II) | $20-40k; useless without trust-fabric of customers | First enterprise asking for it in a questionnaire AND committed to paid pilot |
| Brazilian LTDA | Adds R$2-4k/yr ongoing accounting + tax filings; no benefit pre-BR-revenue | First BR employee OR first BR-invoiced customer in R$ |
| Trademark registration (USPTO, INPI) | ~$350/class US, R$355 BR; defensive only | After product-market fit signal; cybersquatting attempt; competitor name conflict |
| Patent | Nearly never worth it for SaaS | Specific defensive case driven by VC |
| Dedicated lawyer retainer | $5-15k/yr | Post-Series-A |
| Bookkeeping software (Pilot, Bench, Quickbooks $25-200/mo) | Mercury has built-in basic categorization; CPA can do annual cleanup | Multiple payment sources OR >50 txns/mo OR investor reporting |
| HRIS / payroll (Deel, Gusto) | No employees; you are a contractor invoicing your own C-Corp | First W-2 hire OR first BR CLT hire |
| Customer.io / Loops (email lifecycle) | Resend free tier + manual campaigns work <500 users | >500 active users OR >2 lifecycle workflows |
| Premium status page | Better Stack Free covers it | Need SLA dashboards or multi-region public latency |

---

## §9 Open risks / caveats

1. **Brazilian dividend withholding (10%) is new (effective 2026-01-01).** The Foreign Tax Credit will cover most of it US-side but cash-flow timing matters. Re-engage a BR contador in Q4 2026 to align distribution strategy before Year-1 close.
2. **Mercury KYC has tightened in 2026.** If Mercury rejects, fall back to Relay ($0/mo, similar feature set) or Wise Business ($31/mo paid plan for routing+account numbers).
3. **Termageddon does not include a DPA.** Common Paper covers the gap, but if a customer demands a Termageddon-style auto-update guarantee on the DPA, you will need to either (a) commit to a quarterly review yourself, or (b) bump to Iubenda Ultimate which does include DPA tooling. Not expected pre-enterprise.
4. **Stripe acquired Lemon Squeezy in 2024.** "Lemon Squeezy" and "Stripe Managed Payments" are now overlapping products. Onboarding path is via Lemon Squeezy → migration prompt as the product matures. Watch for breaking pricing changes; lock annual where possible.
5. **Cloudflare DPA bookkeeping.** Cloudflare is the heaviest sub-processor (Workers, R2, KV, DO, D1). Their customer DPA at [cloudflare.com/cloudflare-customer-dpa](https://www.cloudflare.com/cloudflare-customer-dpa/) is auto-incorporated when you accept TOS. Re-confirm before signing your first enterprise pilot.
6. **AI exclusions in cyber policies.** When buying E&O+Cyber at $50k ARR, explicitly check the policy does not carve out AI-driven claims — CoreLink's roadmap includes ML cache workloads.

---

## §10 Source index (with fetch dates)

All URLs fetched 2026-05-27.

**Legal docs:**
- [Termly Pricing](https://termly.io/pricing/)
- [Iubenda Pricing](https://www.iubenda.com/en/pricing/)
- [Termageddon Pricing](https://termageddon.com/pricing/)
- [Common Paper — DPA](https://commonpaper.com/standards/data-processing-agreement/)
- [Common Paper — CSA](https://commonpaper.com/standards/cloud-service-agreement/)
- [Common Paper — SLA](https://commonpaper.com/standards/service-level-agreement/)
- [Cloudflare DPA](https://www.cloudflare.com/cloudflare-customer-dpa/)
- [Cybernews — Termly vs Iubenda 2026](https://cybernews.com/privacy-compliance-tools/termly-vs-iubenda/)

**Entity + banking:**
- [Stripe Atlas](https://stripe.com/atlas)
- [Stripe Atlas Pricing 2026 — sparklaun](https://sparklaun.ch/compare/stripe-atlas)
- [Stripe Atlas vs Firstbase vs Doola comparison 2026](https://www.globalsolo.global/blog/stripe-atlas-vs-firstbase-vs-doola-pricing-comparison-2026)
- [Mercury Eligibility](https://support.mercury.com/hc/en-us/articles/28770467511060-Eligibility)
- [Brazilian vs US LLC Comparison — ZS Advogados](https://zsassociados.com/blog/en-2026-04-06-brazilian-company-vs-us-llc-comparison/)
- [Brazil-USA Tax Treaty (none) — ZS Advogados](https://zsassociados.com/blog/en-2026-02-05-tax-treaty-brazil-usa/)
- [Brazil-US Tax Treaty 2026: Why There Isn't One](https://www.countrytaxcalc.com/tax-guides/usa/brazil-us-tax-treaty-guide-2026/)
- [PWC — Brazil Foreign Tax Relief](https://taxsummaries.pwc.com/brazil/individual/foreign-tax-relief-and-tax-treaties)

**Payments:**
- [Lemon Squeezy 2026 update](https://www.lemonsqueezy.com/blog/2026-update)
- [Stripe vs Lemon Squeezy 2026](https://designrevision.com/blog/stripe-vs-lemonsqueezy)
- [Stripe Tax Pricing](https://stripe.com/tax/pricing)
- [Paddle Pricing](https://www.paddle.com/pricing)
- [Merchant of Record Decision for B2B SaaS](https://fintechspecs.com/blog/stripe-vs-paddle-vs-lemon-squeezy-vs-polar-merchant-of-record-b2b-saas/)

**Support / scheduling:**
- [Cal.com Pricing](https://cal.com/pricing)
- [Plain Pricing](https://www.plain.com/pricing)
- [Intercom Pricing](https://www.intercom.com/pricing)

**Analytics / monitoring:**
- [Plausible — self-hosted vs hosted 2026](https://analytics-alternatives.com/tools/plausible/)
- [PostHog Pricing](https://posthog.com/pricing)
- [Sentry Pricing](https://sentry.io/pricing/)
- [Better Stack Pricing](https://www.g2.com/products/better-stack/pricing)
- [Axiom Free Tier — 401 Clicks 2026 roundup](https://401clicks.com/blog/best-log-management-tools-with-generous-free-tiers-2026)

**Insurance:**
- [SaaS Insurance — Founder Shield](https://foundershield.com/business-insurance/saas/)
- [Cyber Insurance Cost — Seedpod 2026](https://seedpodcyber.com/cyber-insurance-for-tech-companies/)
- [E&O Cost — MoneyGeek 2026](https://www.moneygeek.com/insurance/business/professional-liability/errors-and-omissions/cost/)
