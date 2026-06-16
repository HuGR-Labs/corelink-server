export const meta = {
  name: 'corelink-redteam-brutal',
  description: 'BRUTAL black-hat red-team of CoreLink: attacker personas hunt real exploit CHAINS (auth forgery, tenant-isolation breaks, multi-tenant cache poisoning, billing/quota abuse, DoS, secret leak), each adversarially refuted by skeptics — only PROVEN-exploitable survive. Severity weighted by script-kiddie reachability.',
  whenToUse: 'When you need an attacker-grade security audit (not a code review) before/after launch — finds what hackers, malicious tenants, and script kiddies will actually do.',
  phases: [
    { title: 'Recon', detail: 'map attack surface, trust boundaries, auth gates, tenant-isolation + money paths' },
    { title: 'Exploit', detail: 'persona × attack-class hunters build WORKING exploit chains' },
    { title: 'Refute', detail: 'independent skeptics try to DISprove each exploit; only survivors kept' },
    { title: 'Synthesize', detail: 'dedupe, rank by impact×ease, emit fix per confirmed exploit' },
  ],
}

// ── Schemas ───────────────────────────────────────────────────────────────────
const SURFACE_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['boundary', 'entrypoints', 'trust_assumptions', 'attacker_notes'],
  properties: {
    boundary: { type: 'string', description: 'the trust boundary / subsystem mapped' },
    entrypoints: { type: 'array', items: { type: 'string' }, description: 'reachable routes/handlers/functions with file:line' },
    trust_assumptions: { type: 'array', items: { type: 'string' }, description: 'what this boundary TRUSTS (headers, secrets, caller identity) — each a candidate to violate' },
    attacker_notes: { type: 'string', description: 'where it looks soft / what an attacker would probe first' },
  },
}

const EXPLOIT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['candidates'],
  properties: {
    candidates: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['title', 'persona', 'attack_class', 'chain', 'preconditions', 'impact', 'reachability', 'evidence', 'claimed_severity'],
        properties: {
          title: { type: 'string' },
          persona: { type: 'string', description: 'who runs it: anon-internet / script-kiddie / malicious-tenant / leaked-secret / insider / supply-chain' },
          attack_class: { type: 'string', description: 'auth-forgery / tenant-isolation / cache-poisoning / billing-abuse / dos / secret-leak / ssrf / injection / replay / privesc' },
          chain: { type: 'string', description: 'step-by-step exploit chain — concrete requests/inputs, not theory' },
          preconditions: { type: 'string', description: 'what the attacker needs (none / a free account / a leaked key / etc.)' },
          impact: { type: 'string', description: 'concrete blast radius (cross-tenant data, money, RCE-in-victim-build, fleet auth bypass, outage)' },
          reachability: { type: 'string', enum: ['unauth-internet', 'any-free-tenant', 'authed-tenant', 'leaked-secret-only', 'insider-only'] },
          evidence: { type: 'string', description: 'file:line of the weakness + why the guard is missing/bypassable' },
          claimed_severity: { type: 'string', enum: ['critical', 'high', 'medium', 'low'] },
        },
      },
    },
  },
}

const VERDICT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['exploitable', 'confidence', 'refutation', 'corrected_severity', 'fix'],
  properties: {
    exploitable: { type: 'boolean', description: 'true ONLY if a real attacker could actually run this against prod as described' },
    confidence: { type: 'string', enum: ['high', 'medium', 'low'] },
    refutation: { type: 'string', description: 'if not exploitable: the exact guard/condition that blocks it (file:line). if exploitable: why the refutation attempt FAILED.' },
    corrected_severity: { type: 'string', enum: ['critical', 'high', 'medium', 'low', 'not-a-bug'] },
    fix: { type: 'string', description: 'the concrete SOTA fix (file:line + approach) if exploitable' },
  },
}

const BRUTAL = `You are a BLACK-HAT attacker doing a paid red-team of CoreLink, a multi-tenant
content-addressable cache + storage-governance platform on Cloudflare (Worker[TS] -> Durable Object
-> Rust container -> R2 + D1 + KV; ~71 Rust crates). Surfaces: native CAS/AC, Bazel REAPI v2
(routes/bazel_v2.rs), Turborepo (routes/turbo_v8.rs), sccache (WebDAV), OCI registry, and cargo/
brew/npm/pip cache adapters. Auth = Clerk (browser) -> mints PATs; PAT = HMAC fast-fail + Argon2id
(Argon2id verified ONLY on the adapter routes, NOT the native CAS/AC/Bazel/Turbo plane, which trusts
a Worker-injected x-corelink-tenant-id header + HMAC). A single shared CORELINK_INTERNAL_AUTH_KEY
gates 5 high-privilege internal surfaces (any-tenant PAT mint, GDPR erase, CAS erase, admin, pilots).
Billing = Stripe checkout + webhooks; per-tenant monthly $-ceiling (tenant_quota) + per-tier storage/
request quotas; a Free tier exists. The cache is CONTENT-ADDRESSED and SHARED across tenants for
public/deterministic deps (network-effect moat) while private blobs are isolated.

YOUR MANDATE IS BRUTAL. A finding that is "theoretically interesting" is WORTHLESS. Produce only
WORKING EXPLOIT CHAINS a real adversary would run on launch day. Think like:
  - an anonymous internet rando hitting endpoints with curl,
  - a script kiddie running off-the-shelf tooling + guessing IDs,
  - a malicious paying tenant abusing their own valid creds,
  - someone who leaked ONE secret (PAT_SIGNING_KEY or the shared internal-auth key) — what is the blast radius?,
  - a supply-chain attacker trying to POISON the shared multi-tenant cache so OTHER tenants pull
    a malicious build artifact (this is the crown-jewel attack for a cache product — chase it hard).

Hunt RUTHLESSLY for: tenant-isolation / IDOR breaks (read or write another tenant's blobs/AC/quota/
PAT by swapping an id or header), PAT/HMAC/JWT forgery + the native-plane header-trust gap, the
internet-reachable /_internal/pat/mint minting any-tenant admin PATs, internal-auth shared-secret
blast radius, CACHE POISONING (can a tenant write a blob under a digest another tenant will read?
is the digest verified against content on write? can AC entries be spoofed?), billing/quota bypass
(serve paid features unpaid, evade the $-ceiling via batching/races/cycle-roll, free-tier abuse,
Stripe webhook forgery/replay), resource-exhaustion DoS (Argon2id CPU exhaustion via mint loop,
decompression/zip bombs, huge findMissingBlobs batches, unbounded memory), SSRF (success_url,
webhook/JWKS/wallet-broker fetches), secret leakage in responses/logs/errors, replay (PAT, webhook,
idempotency), and privilege escalation.

For EACH exploit: give the concrete chain (the actual requests/inputs/ids), the preconditions, the
real impact, how reachable it is, and the file:line of the weakness + why the guard is missing or
bypassable. Be specific and adversarial. Do NOT pad with low-value style nits — every candidate
must be something that loses CoreLink money, data, or tenants.`

// ── Phase 1: Recon — map the attack surface (barrier: hunters need the full map) ──
phase('Recon')
const RECON_TARGETS = [
  { b: 'Edge auth + native-plane header trust', f: 'worker/src/index.ts (extractAuth, PAT gate, x-corelink-tenant-id injection, /_internal/* gate) + crates/corelink-container/src/routes/{cas,ac,bazel_v2,turbo_v8}.rs tenant resolution' },
  { b: 'Internal control plane', f: 'crates/corelink-container/src/routes/internal_pat.rs (mint) + admin.rs (internal_auth_key_from_env) + the DSR/CAS erase routes + worker internal route forward' },
  { b: 'Multi-tenant cache integrity', f: 'CAS/AC write paths: is the digest verified against content on PUT? can tenant A write a blob another tenant reads? AC entry spoofing; the shared-vs-private partitioning of content-addressed blobs' },
  { b: 'Billing + quota money path', f: 'worker/src/lib/quota.ts + crates/corelink-container/src/tenant_quota.rs + apps/signup-worker Stripe webhook + corelink-stripe-real webhook signature verify + tier resolution' },
  { b: 'Resource limits + DoS surface', f: 'rate-limit layer, Argon2id mint cost, findMissingBlobs/batch caps, blob size limits, decompression, any unbounded loop/alloc keyed on attacker input' },
  { b: 'Outbound + secret handling', f: 'any URL fetch (SSRF: success_url/cancel_url, JWKS, wallet broker, webhooks), secrets in logs/errors/responses, .env handling, CF secret usage' },
]
const surface = (await parallel(RECON_TARGETS.map((t) => () =>
  agent(`${BRUTAL}\n\nRECON TASK — map the attack surface of this boundary: "${t.b}".\nStart from: ${t.f}\nRead the real code. Enumerate every attacker-reachable entrypoint (file:line), every trust assumption that boundary makes (each is a thing to violate), and where it looks soft. Output the structured surface map ONLY.`,
    { label: `recon:${t.b.slice(0, 22)}`, phase: 'Recon', schema: SURFACE_SCHEMA, agentType: 'Explore' }
  )
))).filter(Boolean)

const surfaceBrief = surface.map((s) =>
  `## ${s.boundary}\nentrypoints: ${(s.entrypoints || []).join('; ')}\ntrusts: ${(s.trust_assumptions || []).join('; ')}\nsoft: ${s.attacker_notes}`
).join('\n\n')
log(`Recon done: ${surface.length} boundaries mapped. Unleashing exploit hunters.`)

// ── Phase 2: Exploit — persona × attack-class hunters (parallel) ──
const HUNTERS = [
  { persona: 'anon-internet', cls: 'auth-forgery + internet-reachable internal endpoints (mint/admin/erase) — what can an UNAUTHENTICATED curl do?' },
  { persona: 'script-kiddie', cls: 'IDOR / tenant-isolation — swap tenant ids, pat ids, digests, headers to read/write another tenant' },
  { persona: 'malicious-tenant', cls: 'CACHE POISONING — write a blob under a digest another tenant reads; AC spoofing; digest-not-verified-against-content; private/public partition break' },
  { persona: 'malicious-tenant', cls: 'billing/quota abuse — serve paid features unpaid, evade the $-ceiling (batch/race/cycle-roll), free-tier abuse, tier spoof' },
  { persona: 'script-kiddie', cls: 'resource-exhaustion DoS — Argon2id mint-loop CPU burn, decompression/zip bombs, huge batches, unbounded alloc/loop on attacker input' },
  { persona: 'leaked-secret', cls: 'blast radius of ONE leaked secret — PAT_SIGNING_KEY (forge any tenant PAT?) and the shared CORELINK_INTERNAL_AUTH_KEY (mint+erase+admin?)' },
  { persona: 'supply-chain', cls: 'poison the shared cache so a VICTIM tenant pulls a malicious artifact (cargo/npm/pip/oci/bazel) — trace one full chain end to end' },
  { persona: 'malicious-tenant', cls: 'SSRF + secret leakage — coerce server-side fetches to internal targets; extract secrets/hashes from responses/logs/errors' },
  { persona: 'script-kiddie', cls: 'replay + webhook forgery — replay PATs/Stripe webhooks, bypass idempotency, forge billing state' },
  { persona: 'malicious-tenant', cls: 'privilege escalation — go from a normal PAT/tenant to admin/any-tenant via header injection, scope confusion, or the internal plane' },
]
const rawCandidates = (await parallel(HUNTERS.map((h) => () =>
  agent(`${BRUTAL}\n\nATTACK-SURFACE MAP (from recon):\n${surfaceBrief}\n\nYOUR HUNT — persona: ${h.persona}. Attack class: ${h.cls}\nGo find WORKING exploit chains in this class. Read the actual code to confirm the guard is missing/bypassable. Return every candidate exploit you can substantiate (chain + preconditions + impact + reachability + file:line evidence). Quality over quantity, but miss NOTHING in your class.`,
    { label: `exploit:${h.persona}`, phase: 'Exploit', schema: EXPLOIT_SCHEMA }
  ).then((r) => (r?.candidates || []).map((c) => ({ ...c, hunter: h.persona })))
))).filter(Boolean).flat()

// dedup by title+evidence (barrier already happened) — plain code
const seen = new Set()
const candidates = rawCandidates.filter((c) => {
  const k = (c.title + '|' + (c.evidence || '')).toLowerCase().replace(/\s+/g, ' ').slice(0, 120)
  if (seen.has(k)) return false
  seen.add(k)
  return true
})
log(`Exploit phase: ${rawCandidates.length} raw -> ${candidates.length} deduped candidates. Adversarial refutation next.`)

// ── Phase 3: Refute — 3 independent skeptics per candidate try to DISprove it ──
phase('Refute')
const judged = await parallel(candidates.map((c) => () =>
  parallel([0, 1, 2].map((n) => () =>
    agent(`You are a SKEPTICAL CoreLink defender (reviewer #${n + 1}). An attacker claims this exploit. Your job is to REFUTE it by reading the actual code — find the guard/check/condition that BLOCKS it. Default to exploitable=false UNLESS you cannot find any blocker after a genuine read.\n\nCLAIM: ${c.title}\npersona: ${c.persona} | class: ${c.attack_class} | reachability: ${c.reachability}\nchain: ${c.chain}\npreconditions: ${c.preconditions}\nimpact: ${c.impact}\nevidence cited: ${c.evidence}\n\nRead the cited code (and the guards around it). Is this REALLY exploitable against prod as described, or is there a check that stops it? Give the corrected severity and, if real, the concrete fix.`,
      { label: `refute:${(c.title || '').slice(0, 18)}#${n + 1}`, phase: 'Refute', schema: VERDICT_SCHEMA }
    )
  )).then((verdicts) => {
    const v = verdicts.filter(Boolean)
    const realVotes = v.filter((x) => x.exploitable).length
    const survives = realVotes >= 2 // majority of 3 must agree it's exploitable
    const best = v.find((x) => x.exploitable) || v[0] || {}
    return { ...c, survives, realVotes, totalVotes: v.length, verdict: best }
  })
))
const confirmed = judged.filter(Boolean).filter((j) => j.survives)
log(`Refutation: ${confirmed.length}/${candidates.length} exploits SURVIVED adversarial review (>=2/3 skeptics confirmed exploitable).`)

// ── Phase 4: Synthesize — rank + emit ──
phase('Synthesize')
const rank = { critical: 0, high: 1, medium: 2, low: 3, 'not-a-bug': 9 }
const reachRank = { 'unauth-internet': 0, 'any-free-tenant': 1, 'authed-tenant': 2, 'leaked-secret-only': 3, 'insider-only': 4 }
confirmed.sort((a, b) => {
  const sa = rank[a.verdict?.corrected_severity ?? a.claimed_severity] ?? 5
  const sb = rank[b.verdict?.corrected_severity ?? b.claimed_severity] ?? 5
  if (sa !== sb) return sa - sb
  return (reachRank[a.reachability] ?? 9) - (reachRank[b.reachability] ?? 9)
})

return {
  summary: {
    boundaries_mapped: surface.length,
    raw_candidates: rawCandidates.length,
    deduped: candidates.length,
    confirmed_exploitable: confirmed.length,
    by_severity: confirmed.reduce((acc, c) => {
      const s = c.verdict?.corrected_severity ?? c.claimed_severity
      acc[s] = (acc[s] || 0) + 1
      return acc
    }, {}),
  },
  confirmed_exploits: confirmed.map((c) => ({
    title: c.title,
    severity: c.verdict?.corrected_severity ?? c.claimed_severity,
    persona: c.persona,
    attack_class: c.attack_class,
    reachability: c.reachability,
    votes: `${c.realVotes}/${c.totalVotes}`,
    chain: c.chain,
    impact: c.impact,
    evidence: c.evidence,
    fix: c.verdict?.fix,
  })),
}
