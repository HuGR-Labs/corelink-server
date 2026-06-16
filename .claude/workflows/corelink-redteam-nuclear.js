export const meta = {
  name: 'corelink-redteam-nuclear',
  description: 'NUCLEAR black-hat red-team of CoreLink — cycle-2, 10-25x the brutal workflow. Many more attacker personas x attack classes hunt WORKING exploit chains over LOOP-UNTIL-DRY escalating rounds (each round goes deeper: direct -> second-order/TOCTOU -> composed kill-chains), confirmed primitives are then COMPOSED into multi-step kill chains, every candidate is refuted by 5 diverse-lens skeptics (>=3/5 to survive), and a completeness critic feeds a final escalation round. Bombardeio nuclear + blitzkrieg + chuva de meteoros.',
  whenToUse: 'Cycle-2+ attacker-grade audit against a HARDENED main — when cycle-1 was "too soft" and you need maximum aggression, breadth, depth, and adversarial rigor.',
  phases: [
    { title: 'Recon', detail: 'deep map every trust boundary, cache surface, adapter, money path, crypto, DO/D1/migration, supply chain' },
    { title: 'Hunt', detail: 'persona x attack-class hunters over escalating loop-until-dry rounds build WORKING exploit chains' },
    { title: 'Chain', detail: 'compose confirmed primitives + soft spots into multi-step KILL CHAINS' },
    { title: 'Refute', detail: '5 diverse-lens skeptics per candidate try to DISprove it; only >=3/5-confirmed survive' },
    { title: 'Critic', detail: 'completeness critic finds the unprobed surface -> one final escalation round' },
    { title: 'Synthesize', detail: 'dedupe, rank by impact x reachability, emit fix per confirmed exploit' },
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
    trust_assumptions: { type: 'array', items: { type: 'string' }, description: 'what this boundary TRUSTS (headers, secrets, caller identity, invariants) — each a candidate to violate' },
    attacker_notes: { type: 'string', description: 'where it looks soft / what an attacker would probe first / the assumptions that, if false, break it' },
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
          persona: { type: 'string', description: 'who runs it' },
          attack_class: { type: 'string' },
          chain: { type: 'string', description: 'step-by-step exploit chain — concrete requests/inputs/ids, not theory' },
          preconditions: { type: 'string', description: 'what the attacker needs (none / a free account / a leaked key / a race window / etc.)' },
          impact: { type: 'string', description: 'concrete blast radius (cross-tenant data, money, RCE-in-victim-build, fleet auth bypass, outage, erase)' },
          reachability: { type: 'string', enum: ['unauth-internet', 'any-free-tenant', 'authed-tenant', 'leaked-secret-only', 'insider-only'] },
          evidence: { type: 'string', description: 'file:line of the weakness + why the guard is missing/bypassable' },
          claimed_severity: { type: 'string', enum: ['critical', 'high', 'medium', 'low'] },
        },
      },
    },
  },
}

const CHAIN_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['chains'],
  properties: {
    chains: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['title', 'steps', 'primitives_used', 'preconditions', 'impact', 'reachability', 'evidence', 'claimed_severity'],
        properties: {
          title: { type: 'string', description: 'the composed kill-chain name' },
          steps: { type: 'string', description: 'ordered steps composing 2+ primitives into a bigger outcome — each step concrete' },
          primitives_used: { type: 'string', description: 'which lower-level weaknesses/findings this chains together' },
          preconditions: { type: 'string' },
          impact: { type: 'string', description: 'the amplified blast radius the chain unlocks beyond any single primitive' },
          reachability: { type: 'string', enum: ['unauth-internet', 'any-free-tenant', 'authed-tenant', 'leaked-secret-only', 'insider-only'] },
          evidence: { type: 'string', description: 'file:line for each link in the chain' },
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
    refutation: { type: 'string', description: 'if not exploitable: the exact guard/condition that blocks it (file:line). if exploitable: why the refutation attempt FAILED + the lens you applied.' },
    corrected_severity: { type: 'string', enum: ['critical', 'high', 'medium', 'low', 'not-a-bug'] },
    fix: { type: 'string', description: 'the concrete SOTA fix (file:line + approach) if exploitable' },
  },
}

const CRITIC_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['gaps'],
  properties: {
    gaps: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['unprobed_surface', 'why_it_matters', 'attack_hint'],
        properties: {
          unprobed_surface: { type: 'string', description: 'a boundary/route/modality/assumption NOT yet attacked' },
          why_it_matters: { type: 'string' },
          attack_hint: { type: 'string', description: 'concrete starting point for a hunter (file:line + the angle)' },
        },
      },
    },
  },
}

const BRUTAL = `You are an elite BLACK-HAT attacker doing a paid NUCLEAR red-team of CoreLink, a
multi-tenant content-addressable cache + storage-governance platform on Cloudflare (Worker[TS] ->
Durable Object -> Rust container -> R2 + D1 + KV; ~71 Rust crates). Surfaces: native CAS/AC, Bazel
REAPI v2 (routes/bazel_v2.rs), Turborepo (routes/turbo_v8.rs), sccache (WebDAV), OCI registry, and
cargo/brew/npm/pip cache adapters. Auth = Clerk (browser) -> mints PATs; PAT = HMAC fast-fail +
Argon2id (Argon2id verified ONLY on the adapter routes — the native CAS/AC/Bazel/Turbo plane trusts a
Worker-injected x-corelink-tenant-id header + HMAC, now with an adapter_pat Argon2id backstop wired in
#297; VERIFY whether that backstop is actually reached on every native route or bypassable). Internal
high-privilege surfaces (any-tenant PAT mint, GDPR erase, CAS erase, admin, pilots) are gated by an
internal-auth key — #297 split it into per-consumer keys (PAT_MINT/ADMIN/ERASE) WITH FALLBACK to the
single shared CORELINK_INTERNAL_AUTH_KEY; probe whether the fallback re-opens the shared-blast-radius.
Billing = Stripe checkout + webhooks; per-tenant monthly $-ceiling (tenant_quota) + per-tier storage/
request quotas; storage byte-accounting + a Turbo in-flight byte budget were wired in #297 (verify they
actually accrue/decrement correctly and cannot be raced/underflowed). A Free tier exists. The cache is
CONTENT-ADDRESSED and SHARED across tenants for public/deterministic deps (the network-effect moat)
while private blobs are isolated.

THIS IS CYCLE-2. Cycle-1 found 7 holes (all now fixed in #297). The owner's verdict on prior passes:
"extremamente soft, bolas de neve." Your mandate now is NUCLEAR — 10-25x more aggressive, violent,
invasive, exhaustive. A finding that is "theoretically interesting" is WORTHLESS. Produce only WORKING
EXPLOIT CHAINS a real adversary runs on launch day. Assume the obvious holes are patched — hunt the
SECOND-ORDER ones: the fix that's incomplete, the guard that's bypassable on one route but not another,
the race window in the new accrual code, the fallback that re-opens a closed door, the assumption in
the threat model that's actually false. Think like: an anon internet rando with curl; a script kiddie
with off-the-shelf tooling guessing ids; a malicious paying tenant abusing valid creds; someone who
leaked ONE secret (PAT_SIGNING_KEY / a per-consumer or shared internal-auth key) — exact blast radius;
a supply-chain attacker POISONING the shared multi-tenant cache so OTHER tenants pull a malicious build
artifact (the crown-jewel attack — chase it relentlessly); an insider/compromised-Worker; a competitor
doing cost-amplification / griefing.

For EACH exploit give the concrete chain (actual requests/inputs/ids), preconditions, real impact, how
reachable it is, and file:line of the weakness + why the guard is missing or bypassable. Be specific
and adversarial. Do NOT pad with style nits — every candidate must lose CoreLink money, data, tenants,
or uptime. Read the ACTUAL code before claiming anything; cite file:line.`

// ── Phase 1: Recon — deep map of the attack surface (barrier) ──
phase('Recon')
const RECON_TARGETS = [
  { b: 'Edge auth + native-plane header trust', f: 'worker/src/index.ts (extractAuth, PAT gate, x-corelink-tenant-id injection, /_internal/* gate, header stripping) + crates/corelink-container/src/routes/{cas,ac,bazel_v2,turbo_v8}.rs tenant resolution + the #297 adapter_pat Argon2id backstop wiring' },
  { b: 'Internal control plane + key split', f: 'crates/corelink-container/src/routes/internal_pat.rs (mint + #297 rate-limit) + admin.rs (internal_auth_key_from_env + the #297 per-consumer PAT_MINT/ADMIN/ERASE keys + shared fallback) + DSR/CAS erase routes + worker internal route forward' },
  { b: 'Multi-tenant CAS integrity', f: 'CAS write path: is the digest verified against content on PUT (digest-content binding)? can tenant A write a blob under a digest tenant B reads? the shared-vs-private partitioning of content-addressed blobs; compression/range reads' },
  { b: 'Multi-tenant AC + Bazel integrity', f: 'AC entry write/spoof, Bazel REAPI ActionResult/findMissingBlobs, can an AC entry point at a blob the writer never proved it owns; cross-tenant AC read/write; UpdateActionResult auth' },
  { b: 'Adapter plane (cargo/npm/pip/brew/oci/sccache)', f: 'crates/corelink-container/src/routes/* adapter routes + PatVerifier scope enforcement; sccache WebDAV auth; OCI token/manifest/blob auth; can one adapter cross to another tenant or skip Argon2id' },
  { b: 'Storage byte-accounting (#297)', f: 'crates/corelink-container/src/byte_accounting.rs + cas.rs/ac.rs accrual on write + decrement on delete; D1HttpClient UPSERT atomicity; can bytes_used be raced, underflowed, or bypassed to store unlimited at $0' },
  { b: 'Turbo in-flight byte budget (#297)', f: 'crates/corelink-container/src/routes/turbo_v8.rs FromRequestParts reservation + the global budget; can the reservation be evaded, double-counted, leaked (not released on error), or the 100MiB still buffered before the guard' },
  { b: 'Billing + $-ceiling money path', f: 'worker/src/lib/quota.ts (getTierForTenant, checkStorageQuota, checkRequestQuota) + crates/corelink-container/src/tenant_quota.rs ($-ceiling accrue/seed) + tier resolution subscription_state=active filter; serve-paid-unpaid, $-ceiling race/batch/cycle-roll' },
  { b: 'Stripe webhook + subscription state', f: 'apps/signup-worker Stripe webhook + corelink-stripe-real signature verify + idempotency/replay; can a forged/replayed webhook flip a tenant to a paid tier or reset the $-ceiling; checkout success_url SSRF' },
  { b: 'Request-count quota (#297 monthly_request_counts)', f: 'worker/src/lib/quota.ts checkRequestQuota + the monthly_request_counts D1 table + counter increment; can the counter be raced, skipped, or the cap evaded across the month boundary' },
  { b: 'PAT lifecycle + crypto', f: 'PAT_SIGNING_KEY HMAC fast-fail + Argon2id verify; token_id handling; HMAC timing/constant-time; nonce/salt reuse; revocation; PAT scope encoding; can a forged or expired or revoked PAT pass' },
  { b: 'DoS / resource-exhaustion', f: 'Argon2id mint cost + #297 mint rate-limit; decompression/zip-bomb on any upload; findMissingBlobs/batch caps; blob size limits; unbounded alloc/loop keyed on attacker input; the Turbo + byte-accounting new code paths' },
  { b: 'SSRF + outbound fetch', f: 'every server-side URL fetch: checkout success_url/cancel_url, JWKS, wallet broker, Stripe, webhook callbacks, OCI upstream — can any be coerced to an internal/metadata target' },
  { b: 'Secret leakage + error/log handling', f: 'secrets in responses/logs/errors/stack traces; .env handling; CF secret usage; do error paths echo internal-auth-key/PAT/signing material; verbose 500s; debug headers' },
  { b: 'D1 / SQL + injection surface', f: 'every D1 query built from request input (tenant_id, digest, pat id, tier, counters); parameterization vs string-build; the new #297 accrual/counter UPSERTs; path traversal in any object key' },
  { b: 'Durable Object + state integrity', f: 'worker/src/index.ts DO usage (quota/cache DO), is the DO a real authz gate or a pass-through proxy; DO id derivation (per-tenant isolation); state races across requests; the cited quota_fsm_state' },
  { b: 'Supply-chain / cache-poisoning end to end', f: 'trace ONE full chain: malicious tenant (or anon) writes/poisons a cache entry (cargo crate / npm tarball / pip wheel / oci layer / bazel action) that a VICTIM tenant pulls -> code execution in victim build; digest-content binding, shared-namespace collisions, AC spoofing' },
  { b: 'Migrations + deployment reality', f: 'crates/*/migrations + D1 migration state vs prod; #12 (container has no D1 on native path) — does PAT/quota persistence actually work in the deployed container; #297 new tables applied in prod; env-var/secret presence gating fail-open vs fail-closed' },
]
const surface = (await parallel(RECON_TARGETS.map((t) => () =>
  agent(`${BRUTAL}\n\nRECON TASK — deeply map the attack surface of this boundary: "${t.b}".\nStart from: ${t.f}\nRead the real code. Enumerate every attacker-reachable entrypoint (file:line), every trust assumption that boundary makes (each is a thing to violate), and where it looks soft — especially any #297 fix that is INCOMPLETE or bypassable on a sibling route. Output the structured surface map ONLY.`,
    { label: `recon:${t.b.slice(0, 22)}`, phase: 'Recon', schema: SURFACE_SCHEMA, agentType: 'Explore' }
  )
))).filter(Boolean)

const surfaceBrief = surface.map((s) =>
  `## ${s.boundary}\nentrypoints: ${(s.entrypoints || []).join('; ')}\ntrusts: ${(s.trust_assumptions || []).join('; ')}\nsoft: ${s.attacker_notes}`
).join('\n\n')
log(`Recon done: ${surface.length} boundaries deeply mapped. Unleashing the nuclear hunt.`)

// ── Phase 2: Hunt — persona x attack-class hunters over ESCALATING loop-until-dry rounds ──
phase('Hunt')
const PERSONAS = [
  'anon-internet (no creds, just curl)',
  'script-kiddie (off-the-shelf tooling, id/header guessing)',
  'malicious-paying-tenant (valid Free or paid creds, abuses them)',
  'leaked-PAT_SIGNING_KEY (forge tokens)',
  'leaked-internal-auth-key (per-consumer or shared fallback)',
  'supply-chain attacker (poison the shared cache for victim tenants)',
  'insider / compromised-Worker (trusted header injection)',
  'competitor (cost-amplification, griefing, denial-of-wallet)',
]
const ATTACK_CLASSES = [
  'auth-forgery (PAT/HMAC/JWT/native-plane header trust + the #297 Argon2id backstop completeness)',
  'tenant-isolation / IDOR (swap tenant/pat/digest ids + headers to read/write another tenant)',
  'CACHE POISONING (write a blob/AC entry a victim reads; digest-not-verified-against-content; shared/private partition break)',
  'billing/quota bypass + $-ceiling evasion (serve paid unpaid, batch/race/cycle-roll, free-tier abuse, tier spoof, subscription_state)',
  'storage byte-accounting bypass/race/underflow (#297) — store unlimited at $0 despite the new accrual',
  'DoS / resource-exhaustion (Argon2id mint-loop despite #297 rate-limit, zip/decompression bomb, huge batch, unbounded alloc, byte-budget leak)',
  'secret leakage (logs/errors/responses/stack traces/debug headers)',
  'SSRF (success_url/JWKS/webhook/OCI-upstream coerced to internal/metadata)',
  'injection (D1/SQL string-build, header, log, path traversal in object keys)',
  'replay (PAT/Stripe-webhook/idempotency-key reuse; cross-month counter reset)',
  'privilege-escalation / scope-confusion (normal PAT -> admin/any-tenant via header/scope/internal plane)',
  'race / TOCTOU (quota accrual, reserve->commit, dedup window, DO state, byte-budget release-on-error)',
  'crypto-misuse (HMAC timing/non-constant-time, weak nonce/salt reuse, signature-verify bypass/downgrade)',
  'data-deletion abuse (GDPR/CAS erase another tenant or ransom via the #297 split-key fallback)',
]
// curated escalating round directives
const ROUND_DIRECTIVES = [
  'ROUND 1 — DIRECT: hit each surface head-on with the most obvious working attack in your class. Confirm the guard is missing/bypassable by reading the code.',
  'ROUND 2 — SECOND-ORDER: assume round-1 obvious holes are known. Hunt the INCOMPLETE #297 fix, the guard present on one route but missing on a sibling, the race window in new accrual code, the fallback that re-opens a closed door, the error path that fails OPEN.',
  'ROUND 3 — DEEP/TOCTOU/COMPOSITION: hunt concurrency races, TOCTOU, integer under/overflow, partial-write/partial-failure states, idempotency gaps, and weaknesses that only appear under load or across request boundaries. Question every assumption the recon map marked as TRUSTED.',
  'ROUND 4 — EXOTIC/CRITIC-SEEDED: the weird stuff — protocol edge cases, encoding/normalization (digest/header/path), cross-surface confusion (one adapter to another plane), deployment-reality gaps (#12 no-D1-on-native, fail-open on missing secret), and any surface the completeness critic flagged as unprobed.',
]
function buildHunters() {
  // curated targeted pairings (not the full 8x14 cross — focus where each persona is strongest),
  // expanded each round; ~26 hunters/round
  const pairs = []
  for (const cls of ATTACK_CLASSES) {
    // pick the 1-2 personas most natural for this class
    if (/auth-forgery|privilege|crypto/.test(cls)) { pairs.push(['leaked-PAT_SIGNING_KEY', cls]); pairs.push(['malicious-paying-tenant', cls]) }
    else if (/IDOR|isolation/.test(cls)) { pairs.push(['script-kiddie', cls]); pairs.push(['malicious-paying-tenant', cls]) }
    else if (/POISON/.test(cls)) { pairs.push(['supply-chain attacker', cls]); pairs.push(['malicious-paying-tenant', cls]) }
    else if (/billing|byte-accounting|TOCTOU|race/.test(cls)) { pairs.push(['malicious-paying-tenant', cls]); pairs.push(['competitor', cls]) }
    else if (/DoS|exhaustion/.test(cls)) { pairs.push(['script-kiddie', cls]); pairs.push(['competitor', cls]) }
    else if (/SSRF|leak|injection/.test(cls)) { pairs.push(['anon-internet', cls]); pairs.push(['malicious-paying-tenant', cls]) }
    else if (/replay/.test(cls)) { pairs.push(['script-kiddie', cls]) }
    else if (/deletion/.test(cls)) { pairs.push(['leaked-internal-auth-key', cls]); pairs.push(['malicious-paying-tenant', cls]) }
    else { pairs.push(['anon-internet', cls]) }
  }
  // always add the dedicated whole-persona sweepers
  pairs.push(['anon-internet (no creds, just curl)', 'EVERYTHING reachable unauthenticated — internal endpoints, debug, misrouted'])
  pairs.push(['insider / compromised-Worker (trusted header injection)', 'forge every Worker-injected trust header the native plane believes'])
  return pairs
}

const seen = new Set()
const confirmed = []
const allCandidates = []
let dry = 0
let criticHints = ''
const ROUND_KEY = ['r1', 'r2', 'r3', 'r4']

for (let round = 0; round < ROUND_DIRECTIVES.length; round++) {
  if (dry >= 2) { log(`2 consecutive dry rounds — hunt converged at round ${round}.`); break }
  const directive = ROUND_DIRECTIVES[round] + (round === 3 && criticHints ? `\n\nCRITIC-FLAGGED UNPROBED SURFACE:\n${criticHints}` : '')
  const hunters = buildHunters()
  log(`HUNT round ${round + 1}/${ROUND_DIRECTIVES.length}: ${hunters.length} hunters. ${dry} dry so far.`)

  // find -> dedupe vs seen -> refute, all per round (pipeline within the round via parallel hunt then parallel refute)
  const raw = (await parallel(hunters.map(([persona, cls], i) => () =>
    agent(`${BRUTAL}\n\nATTACK-SURFACE MAP (from recon):\n${surfaceBrief}\n\n${directive}\n\nYOUR HUNT — persona: ${persona}. Attack class: ${cls}\nGo find WORKING exploit chains in this class for THIS round's depth. Read the actual code to confirm the guard is missing/bypassable. Return every candidate you can substantiate (chain + preconditions + impact + reachability + file:line evidence). Miss NOTHING in your class at this depth. If you genuinely find nothing exploitable at this depth after a real read, return an empty candidates array — do NOT invent.`,
      { label: `hunt:${ROUND_KEY[round]}:${persona.split(' ')[0].slice(0, 12)}`, phase: 'Hunt', schema: EXPLOIT_SCHEMA }
    ).then((r) => (r?.candidates || []).map((c) => ({ ...c, hunter: persona, round: round + 1 })))
  ))).filter(Boolean).flat()

  const fresh = raw.filter((c) => {
    const k = (c.title + '|' + (c.evidence || '')).toLowerCase().replace(/\s+/g, ' ').slice(0, 120)
    if (seen.has(k)) return false
    seen.add(k)
    return true
  })
  allCandidates.push(...fresh)
  log(`Round ${round + 1}: ${raw.length} raw -> ${fresh.length} fresh candidates.`)
  if (fresh.length === 0) { dry++; continue }
  dry = 0

  // refute this round's fresh candidates (5 diverse lenses, >=3/5)
  const judged = await refute(fresh, `r${round + 1}`)
  const survived = judged.filter((j) => j.survives)
  confirmed.push(...survived)
  log(`Round ${round + 1}: ${survived.length}/${fresh.length} fresh exploits SURVIVED refutation. Cumulative confirmed: ${confirmed.length}.`)

  // after round 3, run the completeness critic to seed round 4
  if (round === 2) {
    const confirmedBrief = confirmed.map((c) => `- [${c.verdict?.corrected_severity ?? c.claimed_severity}] ${c.title} (${c.attack_class}) @ ${c.evidence}`).join('\n') || '(none yet)'
    const critics = (await parallel([0, 1, 2].map((n) => () =>
      agent(`${BRUTAL}\n\nYou are a COMPLETENESS CRITIC (#${n + 1}) for an in-progress nuclear red-team. Below is the surface map and everything CONFIRMED so far. Your job: find what is MISSING — a boundary/route/modality/assumption that has NOT been attacked yet, or a confirmed finding whose neighbors/siblings were not checked. What would a real attacker try that this audit has not?\n\nSURFACE MAP:\n${surfaceBrief}\n\nCONFIRMED SO FAR:\n${confirmedBrief}\n\nReturn the concrete unprobed surfaces with attack hints (file:line + the angle).`,
        { label: `critic#${n + 1}`, phase: 'Critic', schema: CRITIC_SCHEMA }
      ).then((r) => (r?.gaps || []))
    ))).filter(Boolean).flat()
    criticHints = critics.map((g) => `- ${g.unprobed_surface} — ${g.attack_hint} (${g.why_it_matters})`).join('\n').slice(0, 4000)
    log(`Completeness critics flagged ${critics.length} unprobed surfaces -> seeding final escalation round.`)
  }
}

// ── Phase 3: Chain — compose confirmed primitives + soft spots into multi-step kill chains ──
phase('Chain')
const primitivesBrief = confirmed.map((c) =>
  `- [${c.verdict?.corrected_severity ?? c.claimed_severity}] ${c.title} | class=${c.attack_class} | reach=${c.reachability} | ${c.evidence}\n    chain: ${(c.chain || '').slice(0, 240)}`
).join('\n')
const softBrief = surface.map((s) => `- ${s.boundary}: ${s.attacker_notes}`).join('\n').slice(0, 3000)
let chainCandidates = []
if (confirmed.length >= 2) {
  const CHAINERS = [
    'unauth-to-RCE: chain an unauth/anon primitive all the way to code execution in a victim tenant build (cache poisoning is the prize)',
    'leaked-secret-amplification: take ONE leaked secret and chain it across surfaces for maximum blast radius (mint -> erase -> billing -> cross-tenant)',
    'money-extraction: chain billing/quota/storage/$-ceiling primitives into sustained free paid-tier usage or denial-of-wallet against a victim',
    'tenant-takeover: chain isolation + auth + privilege primitives to fully read+write+erase another tenant',
    'persistence-and-stealth: chain a write/poison primitive with a secret-leak or replay primitive into a durable, hard-to-detect foothold',
  ]
  chainCandidates = (await parallel(CHAINERS.map((goal, i) => () =>
    agent(`${BRUTAL}\n\nYou are composing KILL CHAINS. Below are CONFIRMED exploit primitives and SOFT spots from the recon. Your goal: ${goal}.\nCompose 2+ of these primitives (and/or soft spots) into a multi-step chain whose IMPACT is greater than any single primitive. Only output chains where EVERY link is supported by the cited code — no hand-waving. If the primitives genuinely do not compose toward this goal, return an empty chains array.\n\nCONFIRMED PRIMITIVES:\n${primitivesBrief}\n\nSOFT SPOTS:\n${softBrief}`,
      { label: `chain:${goal.split(':')[0].slice(0, 16)}`, phase: 'Chain', schema: CHAIN_SCHEMA }
    ).then((r) => (r?.chains || []).map((c) => ({
      // normalize chain -> candidate shape for refutation
      title: c.title, persona: 'chained', attack_class: 'kill-chain', chain: `${c.steps}\n[primitives: ${c.primitives_used}]`,
      preconditions: c.preconditions, impact: c.impact, reachability: c.reachability, evidence: c.evidence,
      claimed_severity: c.claimed_severity, hunter: 'chainer', round: 'chain',
    })))
  ))).filter(Boolean).flat()
  log(`Chain phase: ${chainCandidates.length} composed kill-chains proposed. Refuting.`)
  if (chainCandidates.length) {
    const judgedChains = await refute(chainCandidates, 'chain')
    confirmed.push(...judgedChains.filter((j) => j.survives))
  }
} else {
  log('Chain phase skipped: <2 confirmed primitives to compose.')
}

// ── Phase 5: Synthesize — rank + emit ──
phase('Synthesize')
const rank = { critical: 0, high: 1, medium: 2, low: 3, 'not-a-bug': 9 }
const reachRank = { 'unauth-internet': 0, 'any-free-tenant': 1, 'authed-tenant': 2, 'leaked-secret-only': 3, 'insider-only': 4 }
const finalSort = (a, b) => {
  const sa = rank[a.verdict?.corrected_severity ?? a.claimed_severity] ?? 5
  const sb = rank[b.verdict?.corrected_severity ?? b.claimed_severity] ?? 5
  if (sa !== sb) return sa - sb
  return (reachRank[a.reachability] ?? 9) - (reachRank[b.reachability] ?? 9)
}
confirmed.sort(finalSort)

return {
  summary: {
    cycle: 2,
    boundaries_mapped: surface.length,
    rounds_run: Math.min(dry >= 2 ? ROUND_DIRECTIVES.length : ROUND_DIRECTIVES.length, ROUND_DIRECTIVES.length),
    total_candidates: allCandidates.length + chainCandidates.length,
    confirmed_exploitable: confirmed.length,
    by_severity: confirmed.reduce((acc, c) => {
      const s = c.verdict?.corrected_severity ?? c.claimed_severity
      acc[s] = (acc[s] || 0) + 1
      return acc
    }, {}),
    by_reachability: confirmed.reduce((acc, c) => {
      acc[c.reachability] = (acc[c.reachability] || 0) + 1
      return acc
    }, {}),
  },
  confirmed_exploits: confirmed.map((c) => ({
    title: c.title,
    severity: c.verdict?.corrected_severity ?? c.claimed_severity,
    persona: c.persona,
    attack_class: c.attack_class,
    reachability: c.reachability,
    round: c.round,
    votes: `${c.realVotes}/${c.totalVotes}`,
    chain: c.chain,
    preconditions: c.preconditions,
    impact: c.impact,
    evidence: c.evidence,
    fix: c.verdict?.fix,
  })),
}

// ── refute helper: 5 diverse-lens skeptics per candidate, >=3/5 to survive ──
async function refute(cands, tag) {
  const LENSES = [
    'CODE-PATH: trace the exact code path and find the guard/check/early-return that blocks this (file:line).',
    'CRYPTO/AUTH: scrutinize the auth/crypto claim — is the signature/HMAC/scope/Argon2id actually verifiable-bypassable here, or does a check stop it?',
    'CONCURRENCY/STATE: is the claimed race/TOCTOU/underflow real given the actual locking/atomicity/UPSERT semantics, or is it serialized?',
    'DEPLOYMENT-REALITY: would this actually work against PROD as deployed (env vars present, D1/#12 reality, Worker header-strip, fail-open vs fail-closed), or only in theory?',
    'REPRODUCE: walk the literal requests/inputs/ids end to end — does the chain actually execute, or does a step not connect to the next?',
  ]
  return await parallel(cands.map((c) => () =>
    parallel(LENSES.map((lens, n) => () =>
      agent(`You are a SKEPTICAL CoreLink defender, lens ${n + 1}/5. Apply THIS lens to REFUTE the attacker's claim by reading the actual code. Default to exploitable=false UNLESS, after a genuine read through your lens, you cannot find any blocker.\n\nLENS: ${lens}\n\nCLAIM: ${c.title}\npersona: ${c.persona} | class: ${c.attack_class} | reachability: ${c.reachability}\nchain: ${c.chain}\npreconditions: ${c.preconditions}\nimpact: ${c.impact}\nevidence cited: ${c.evidence}\n\nIs this REALLY exploitable against prod as described, or is there a check that stops it? Give corrected severity and, if real, the concrete SOTA fix (file:line + approach).`,
        { label: `refute:${tag}:${(c.title || '').slice(0, 14)}#${n + 1}`, phase: 'Refute', schema: VERDICT_SCHEMA }
      )
    )).then((verdicts) => {
      const v = verdicts.filter(Boolean)
      const realVotes = v.filter((x) => x.exploitable).length
      const survives = realVotes >= 3 // supermajority of 5
      // pick the highest-confidence exploitable verdict for the fix, else any
      const best = v.filter((x) => x.exploitable).sort((a, b) => ({ high: 0, medium: 1, low: 2 }[a.confidence] ?? 3) - ({ high: 0, medium: 1, low: 2 }[b.confidence] ?? 3))[0] || v[0] || {}
      return { ...c, survives, realVotes, totalVotes: v.length, verdict: best }
    })
  ))
}
