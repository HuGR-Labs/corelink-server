export const meta = {
  name: 'corelink-dd-engineering-brutal',
  description: 'BRUTAL acquirer-grade engineering due diligence of CoreLink: M&A hard-questions + code quality / maintainability / efficiency-COGS / correctness / security-liability / compliance teardown across parallel DD lanes. Findings adversarially verified; output is an acquisition verdict + ranked must-fix-before-close. (The opinionated Musk/Thiel/House panel is a SEPARATE workflow: corelink-engineering-panel.)',
  whenToUse: 'Pre-acquisition / pre-launch engineering DD — when you need the brutal hard questions an acquirer asks, plus a real code-quality/efficiency/maintainability teardown (not a style pass).',
  phases: [
    { title: 'Recon', detail: 'map architecture, debt hotspots, test reality, money path + COGS' },
    { title: 'Audit', detail: 'acquirer DD lanes: scalability, maintainability, correctness, efficiency-COGS, security-liability, compliance' },
    { title: 'Verify', detail: 'skeptics separate proven material findings from opinion' },
    { title: 'Synthesize', detail: 'acquisition verdict + ranked must-fix-before-buy' },
  ],
}

const CTX = `CoreLink: multi-tenant content-addressable cache + storage-governance platform on Cloudflare
(Worker[TS] -> Durable Object -> Rust container -> R2 + D1 + KV; ~71 Rust crates). Surfaces: native
CAS/AC, Bazel REAPI v2, Turborepo, sccache (WebDAV), OCI registry, cargo/brew/npm/pip adapters. Auth =
Clerk -> PAT (HMAC + Argon2id on adapter routes only). Billing = Stripe + per-tenant $-ceiling + tier
quotas. Sold self-serve to SMBs (~$15-149/mo). The pitch: a content-addressed multi-tenant cache has a
network-effect moat (more tenants -> fuller cache -> faster+cheaper for everyone) and ~80% margin
(price $30, COGS ~$5). Expansion: CI/build-acceleration on cheap third-party runners + the cache.`

const FINDINGS_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['findings'],
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['title', 'category', 'severity', 'the_hard_question', 'what_i_found', 'evidence', 'impact', 'recommendation'],
        properties: {
          title: { type: 'string' },
          category: { type: 'string', description: 'scalability / maintainability / correctness / efficiency-COGS / security-liability / compliance / architecture / test-reality / bus-factor / moat-risk / over-engineering / dishonesty(code-lies)' },
          severity: { type: 'string', enum: ['deal-breaker', 'major', 'moderate', 'minor'] },
          the_hard_question: { type: 'string', description: 'the brutal acquirer/engineer question this answers' },
          what_i_found: { type: 'string', description: 'the actual finding — specific, technical' },
          evidence: { type: 'string', description: 'file:line / metric / repro — proof, not vibes' },
          impact: { type: 'string', description: 'on valuation, scale, cost, reliability, or team velocity' },
          recommendation: { type: 'string', description: 'the concrete SOTA fix or de-risk' },
        },
      },
    },
  },
}

const VERIFY_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['material', 'is_factual', 'corrected_severity', 'verdict_note'],
  properties: {
    material: { type: 'boolean', description: 'true if a real, evidence-backed engineering issue (not taste/opinion)' },
    is_factual: { type: 'boolean', description: 'true if the cited evidence actually checks out in the code' },
    corrected_severity: { type: 'string', enum: ['deal-breaker', 'major', 'moderate', 'minor', 'non-issue'] },
    verdict_note: { type: 'string', description: 'why it stands or falls; correct any overstatement' },
  },
}

// ── Phase 1: Recon ────────────────────────────────────────────────────────────
phase('Recon')
const RECON = [
  { k: 'architecture', p: 'Map the real architecture: the Worker->DO->container->R2/D1/KV data flow, the 71-crate layout, the trust boundaries, and where complexity concentrates. Name the 5 most load-bearing modules.' },
  { k: 'debt-hotspots', p: 'Find the tech-debt + complexity hotspots: god files, TODO/FIXME/HACK/XXX density, #[allow] / eslint-disable / @ts-ignore suppressions, "deferred"/"Wave-NN"/"for now"/"stub" markers, dead code, duplication. Cite the worst offenders file:line.' },
  { k: 'test-reality', p: 'Assess the TEST REALITY (not coverage %): are tests meaningful or vacuous? mocks that assert nothing, ignored/skipped tests, pre-existing reds, gates that are off per-PR, proptest density, whether the money/auth paths are actually tested. Be skeptical — green != correct.' },
  { k: 'money-and-cogs', p: 'Map the money path + COGS reality: Stripe/quota/tier enforcement, what actually meters cost, per-request D1/R2 round-trips and Argon2id cost on hot paths, and whether the claimed ~80% margin survives contact with the real per-op cost. Find the cost leaks.' },
]
const recon = (await parallel(RECON.map((r) => () =>
  agent(`${CTX}\n\nRECON: ${r.p}\nRead the real code. Be concrete (file:line). Return findings (use category sensibly; severity = how much it matters to an acquirer/operator).`,
    { label: `recon:${r.k}`, phase: 'Recon', schema: FINDINGS_SCHEMA, agentType: 'Explore' }
  ).then((x) => (x?.findings || [])))
)).filter(Boolean).flat()
const reconBrief = recon.slice(0, 40).map((f) => `- [${f.category}/${f.severity}] ${f.title} — ${f.evidence}`).join('\n')
log(`Recon: ${recon.length} initial observations. Running acquirer DD lanes.`)

// ── Phase 2: Audit — acquirer DD lanes (parallel) ─────────────────────────────
const DD_LANES = [
  { k: 'scalability', p: 'Acquirer DD — SCALABILITY: where does this fall over at 100x tenants/traffic/blob-count? per-tenant DO hotspots, D1 limits, R2 listing costs, unbounded growth, the cache index, single-Mac CI as a dev-velocity cliff. What breaks first and at what number?' },
  { k: 'maintainability', p: 'Acquirer DD — MAINTAINABILITY & BUS-FACTOR: how expensive is this to own? coupling, can a new engineer ship safely, where is the tribal knowledge, how brittle is the deploy (CF secrets write-only, shared-Mac runners), 71 crates — is that modularity or fragmentation? What is the realistic onboarding + change cost?' },
  { k: 'correctness', p: 'Acquirer DD — CORRECTNESS & DATA INTEGRITY: concurrency/TOCTOU on money+quota+cache, idempotency, partial-failure handling, the multi-tenant isolation guarantees as actually implemented, error-handling that swallows. Where can it silently corrupt or double-charge or cross tenants?' },
  { k: 'efficiency-cogs', p: 'Acquirer DD — EFFICIENCY & UNIT ECONOMICS: does the ~80% margin survive? hot-path allocations/clones, per-request Argon2id + D1 round-trips, redundant verification (e.g. double Argon2id), R2/D1 op counts per cache hit, build/CI compute burn. Quantify the cost leaks that erode margin.' },
  { k: 'security-liability', p: 'Acquirer DD — SECURITY AS A LIABILITY (engineering, not pentest): secret management (shared keys, write-only CF secrets, .env backup risk), dependency/supply-chain exposure, the auth architecture concentration, audit-log integrity, blast radius of one leaked key. What would a security DD flag as a buy-blocker?' },
  { k: 'compliance', p: 'Acquirer DD — COMPLIANCE & LEGAL: GDPR/DSR erasure completeness (is deletion real & verifiable?), data residency (Schrems II), license compliance of the dep tree, PII-in-logs, retention. What compliance debt transfers to the acquirer?' },
]

const allFindings = (await parallel(
  DD_LANES.map((a) => () =>
    agent(`${CTX}\n\nRecon brief (other auditors' early observations — extend/challenge, don't just repeat):\n${reconBrief}\n\n${a.p}\n\nRead the actual code. Every finding needs file:line evidence and a concrete recommendation. Ask the HARD questions. Return your findings.`,
      { label: `audit:${a.k}`, phase: 'Audit', schema: FINDINGS_SCHEMA }
    ).then((x) => (x?.findings || []).map((f) => ({ ...f, auditor: a.k })))
  )
)).filter(Boolean).flat()

// dedup by title — plain code
const seen = new Set()
const findings = allFindings.filter((f) => {
  const k = (f.title || '').toLowerCase().replace(/\s+/g, ' ').slice(0, 90)
  if (seen.has(k)) return false
  seen.add(k)
  return true
})
log(`Audit: ${allFindings.length} raw -> ${findings.length} deduped DD findings. Verifying materiality.`)

// ── Phase 3: Verify — skeptic separates proven from opinion (focus on the heavy ones) ──
phase('Verify')
const heavy = findings.filter((f) => f.severity === 'deal-breaker' || f.severity === 'major')
const verified = await parallel(heavy.map((f) => () =>
  agent(`You are a rigorous engineering DD lead validating a junior's finding before it goes in the acquisition memo. Confirm it against the ACTUAL code — is it material (a real engineering issue, not taste) and is the cited evidence true? Correct any overstatement.\n\nFINDING: ${f.title}\ncategory: ${f.category} | claimed severity: ${f.severity} | by: ${f.auditor}\nhard question: ${f.the_hard_question}\nfound: ${f.what_i_found}\nevidence: ${f.evidence}\nimpact: ${f.impact}\n\nVerify and return the verdict.`,
    { label: `verify:${(f.title || '').slice(0, 20)}`, phase: 'Verify', schema: VERIFY_SCHEMA }
  ).then((v) => ({ ...f, v }))
))
const confirmed = verified.filter(Boolean).filter((f) => f.v?.material && f.v?.is_factual && f.v?.corrected_severity !== 'non-issue')
// keep the lighter (moderate/minor) findings as-is (not individually verified, but recorded)
const lighter = findings.filter((f) => f.severity === 'moderate' || f.severity === 'minor')
log(`Verify: ${confirmed.length}/${heavy.length} heavy findings CONFIRMED material+factual. + ${lighter.length} lighter recorded.`)

// ── Phase 4: Synthesize — acquisition verdict ─────────────────────────────────
phase('Synthesize')
const rank = { 'deal-breaker': 0, major: 1, moderate: 2, minor: 3, 'non-issue': 9 }
const sev = (f) => f.v?.corrected_severity ?? f.severity
confirmed.sort((a, b) => (rank[sev(a)] ?? 5) - (rank[sev(b)] ?? 5))

const memoInput = confirmed.slice(0, 30).map((f) =>
  `- [${sev(f)} | ${f.category} | ${f.auditor}] ${f.title}\n    Q: ${f.the_hard_question}\n    found: ${f.what_i_found} (${f.evidence})\n    fix: ${f.recommendation}`
).join('\n')
const memo = await agent(
  `${CTX}\n\nYou are the lead partner writing the ENGINEERING DUE-DILIGENCE MEMO for a potential acquirer of CoreLink. Below are the confirmed material findings from Musk/Thiel/House + the DD lanes. Write a brutal, honest memo: (1) one-paragraph engineering verdict — would you acquire, and at what discount / under what conditions; (2) the DEAL-BREAKERS (must-fix-before-close); (3) the major risks; (4) what's genuinely GOOD (be fair where earned — the moat, the rigor); (5) the single most important hard question the team must answer. Be the adult in the room: specific, quantified, no flattery.\n\nCONFIRMED FINDINGS:\n${memoInput}`,
  { label: 'dd-memo', phase: 'Synthesize' }
)

return {
  summary: {
    recon_observations: recon.length,
    raw_findings: allFindings.length,
    deduped: findings.length,
    heavy_verified: confirmed.length,
    lighter_recorded: lighter.length,
    by_severity: confirmed.reduce((a, f) => { const s = sev(f); a[s] = (a[s] || 0) + 1; return a }, {}),
  },
  acquisition_memo: memo,
  confirmed_findings: confirmed.map((f) => ({
    title: f.title, severity: sev(f), category: f.category, auditor: f.auditor,
    hard_question: f.the_hard_question, finding: f.what_i_found, evidence: f.evidence,
    impact: f.impact, recommendation: f.recommendation, verify_note: f.v?.verdict_note,
  })),
  lighter_findings: lighter.map((f) => ({ title: f.title, severity: f.severity, category: f.category, auditor: f.auditor, evidence: f.evidence, recommendation: f.recommendation })),
}
