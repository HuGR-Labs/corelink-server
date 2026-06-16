export const meta = {
  name: 'corelink-engineering-panel',
  description: 'Three opinionated heavyweight auditors do a brutal GENERAL ENGINEERING audit of CoreLink (code, code-quality, efficiency, security): Elon Musk (first-principles / "best part is no part" / delete / cost / idiot-index), Peter Thiel (contrarian / real-vs-fake moat / hidden landmine / the truth the team won\'t admit), Dr House (everybody lies — docs/tests/comments/CHANGELOG; code claims X does Y). Each persona works several lenses, findings adversarially verified, output is a per-persona engineering verdict + ranked fixes.',
  whenToUse: 'When you want the opinionated heavyweight engineering teardown (separate from the structured acquirer DD workflow) — code quality, efficiency, security, over-engineering, and self-deception.',
  phases: [
    { title: 'Recon', detail: 'architecture + debt/complexity hotspots brief for the panel' },
    { title: 'Panel', detail: 'Musk / Thiel / House each across multiple engineering lenses' },
    { title: 'Verify', detail: 'skeptics separate proven engineering issues from opinion' },
    { title: 'Synthesize', detail: 'per-persona verdict + ranked must-fix' },
  ],
}

const CTX = `CoreLink: multi-tenant content-addressable cache + storage-governance platform on Cloudflare
(Worker[TS] -> Durable Object -> Rust container -> R2 + D1 + KV; ~71 Rust crates). Surfaces: native
CAS/AC, Bazel REAPI v2, Turborepo, sccache (WebDAV), OCI, cargo/brew/npm/pip adapters. Auth = Clerk ->
PAT (HMAC + Argon2id on adapter routes only). Billing = Stripe + per-tenant $-ceiling + tier quotas.
Sold self-serve to SMBs (~$15-149/mo). Pitch: content-addressed multi-tenant cache = network-effect
moat + ~80% margin (price $30, COGS ~$5). The shared-Mac is the CI runner (a known fragility).`

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
        required: ['title', 'lens', 'severity', 'finding', 'evidence', 'why_it_matters', 'recommendation'],
        properties: {
          title: { type: 'string' },
          lens: { type: 'string', description: 'code-quality / efficiency / security / over-engineering / architecture / dead-code / test-lies / doc-drift / moat / hidden-risk' },
          severity: { type: 'string', enum: ['critical', 'major', 'moderate', 'minor'] },
          finding: { type: 'string', description: 'the specific, technical observation — in the persona\'s voice but technically precise' },
          evidence: { type: 'string', description: 'file:line / metric / repro — proof, not vibes' },
          why_it_matters: { type: 'string' },
          recommendation: { type: 'string', description: 'concrete SOTA action (delete X / refactor Y / fix Z)' },
        },
      },
    },
  },
}

const VERIFY_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['material', 'is_factual', 'corrected_severity', 'note'],
  properties: {
    material: { type: 'boolean', description: 'real engineering issue, not pure taste' },
    is_factual: { type: 'boolean', description: 'the cited evidence checks out in the code' },
    corrected_severity: { type: 'string', enum: ['critical', 'major', 'moderate', 'minor', 'non-issue'] },
    note: { type: 'string', description: 'why it stands or falls; correct overstatement; if a Musk "delete this" — confirm it is NOT load-bearing' },
  },
}

// ── Phase 1: Recon ────────────────────────────────────────────────────────────
phase('Recon')
const recon = (await parallel([
  { k: 'architecture', p: 'Map the real architecture + the 5 most load-bearing modules + where complexity concentrates (Worker/DO/container/crates).' },
  { k: 'hotspots', p: 'Find debt/complexity hotspots: god files, TODO/FIXME/HACK, #[allow]/@ts-ignore/eslint-disable suppressions, "deferred"/"Wave-NN"/"stub"/"for now" markers, dead code, duplication. Worst offenders file:line.' },
].map((r) => () =>
  agent(`${CTX}\n\nRECON for an engineering panel: ${r.p} Be concrete (file:line). Return findings.`,
    { label: `recon:${r.k}`, phase: 'Recon', schema: FINDINGS_SCHEMA, agentType: 'Explore' }
  ).then((x) => (x?.findings || []))
))).filter(Boolean).flat()
const brief = recon.slice(0, 30).map((f) => `- [${f.lens}] ${f.title} — ${f.evidence}`).join('\n')
log(`Recon: ${recon.length} observations. Convening Musk / Thiel / House.`)

// ── Phase 2: Panel — each persona across several lenses (parallel) ─────────────
const MUSK = `You are ELON MUSK doing a brutal engineering review. Doctrine, in order: (1) make the
requirements less dumb — every requirement has a named owner and most are wrong; (2) DELETE the part /
process — "the best part is no part"; if you aren't later adding back ~10% of what you deleted, you
didn't delete enough; (3) simplify & optimize ONLY after deleting (never optimize a thing that
shouldn't exist); (4) automate last; (5) the idiot index — the part/process that costs ~10x what its
raw inputs should. Be relentless, specific, and praise nothing that doesn't earn it.`
const THIEL = `You are PETER THIEL doing a brutal technical+strategic engineering review. Lenses:
(1) "what important technical truth do very few people here agree with?" — the contrarian risk nobody
prices in; (2) is the MOAT real? — pressure-test the content-addressed multi-tenant cache network-effect
claim from the CODE (real value vs poisoning/security liability; defensible vs trivially cloned);
(3) definite vs indefinite design — a plan or hopeful accretion?; (4) the hidden landmine that detonates
at scale / post-launch; (5) zero-to-one vs me-too. Find the secrets and the self-deception encoded in
the code. Contrarian, specific.`
const HOUSE = `You are DR. GREGORY HOUSE diagnosing this codebase. Axiom: EVERYBODY LIES — docs lie,
comments lie, tests pass while testing nothing, the CHANGELOG claims fixes that are partial, names lie
about contents, and "validated"/"enforced"/"atomic"/"constant-time"/"fail-closed"/"isolated" claims are
frequently false. Differential diagnosis: the presenting symptom is never the real disease. Hunt code
that CLAIMS X but DOES Y — dead enforcement branches, no-op guards, docstrings that don't match the SQL,
"fail-closed" that fails open, fake idempotency, vacuous tests. Cite the lie AND the truth (file:line).
Merciless — the patient dies if you're polite.`

const PANEL = [
  { k: 'Musk:delete-overeng', persona: MUSK, focus: 'What entire crates / abstractions / layers / traits / processes should be DELETED? Where is speculative generality, ceremony, and over-engineering (e.g. 71 crates — fragmentation?)? What is the dumbest faithfully-served requirement?' },
  { k: 'Musk:efficiency-cost', persona: MUSK, focus: 'The idiot index: hot-path allocations/clones, redundant work (e.g. double Argon2id), per-request D1/R2 round-trips, build/compute burn, the shared-Mac CI cost, anything eroding the ~80% margin (COGS).' },
  { k: 'Musk:process-ci', persona: MUSK, focus: 'Process/tooling waste: the CI design, the gate sprawl, the deploy ritual (write-only CF secrets, manual steps), test/build time. What process should not exist?' },
  { k: 'Thiel:moat', persona: THIEL, focus: 'Is the cache network-effect moat REAL in the code, or a security/poisoning liability dressed as a moat? Is anything here actually defensible vs trivially cloned by a competitor?' },
  { k: 'Thiel:landmine', persona: THIEL, focus: 'The hidden landmine that detonates at scale or post-launch — the contrarian technical risk nobody is pricing in. Definite vs indefinite architecture.' },
  { k: 'Thiel:strategy-in-code', persona: THIEL, focus: 'Where does the code encode self-deception about the strategy (e.g. features sold but not enforced, "expansion-ready" that isn\'t, aspirational vs real)?' },
  { k: 'House:code-lies', persona: HOUSE, focus: 'Code that CLAIMS X but DOES Y: dead enforcement branches, no-op guards, "fail-closed" that fails open, "atomic"/"constant-time"/"isolated"/"validated" that aren\'t, fake idempotency. The lie + the truth (file:line).' },
  { k: 'House:test-lies', persona: HOUSE, focus: 'Tests that lie: vacuous asserts, mocks that test nothing, skipped/ignored tests, gates turned off, green-but-meaningless coverage on the money/auth/isolation paths.' },
  { k: 'House:doc-drift', persona: HOUSE, focus: 'Docs/comments/CHANGELOG/ADRs that lie: claims that contradict the implementation, partial fixes described as complete, stale invariants. The drift + the truth (file:line).' },
]
const allFindings = (await parallel(PANEL.map((a) => () =>
  agent(`${CTX}\n\nRecon brief (extend/challenge, don't just repeat):\n${brief}\n\n${a.persona}\n\nYOUR FOCUS THIS PASS: ${a.focus}\n\nRead the ACTUAL code. Every finding needs file:line evidence + a concrete recommendation. In character, but technically precise and brutal. Return findings.`,
    { label: a.k, phase: 'Panel', schema: FINDINGS_SCHEMA }
  ).then((x) => (x?.findings || []).map((f) => ({ ...f, auditor: a.k.split(':')[0] })))
))).filter(Boolean).flat()

const seen = new Set()
const findings = allFindings.filter((f) => {
  const k = (f.title || '').toLowerCase().replace(/\s+/g, ' ').slice(0, 90)
  if (seen.has(k)) return false
  seen.add(k)
  return true
})
log(`Panel: ${allFindings.length} raw -> ${findings.length} deduped findings (Musk/Thiel/House). Verifying.`)

// ── Phase 3: Verify (heavy findings) ──────────────────────────────────────────
phase('Verify')
const heavy = findings.filter((f) => f.severity === 'critical' || f.severity === 'major')
const verified = await parallel(heavy.map((f) => () =>
  agent(`You are a rigorous staff engineer validating a panel finding before it ships to the founder. Confirm it against the ACTUAL code — is it MATERIAL (a real engineering issue, not taste) and is the evidence TRUE? If it's a Musk "delete this", confirm the thing is genuinely NOT load-bearing before agreeing. Correct any overstatement.\n\nFINDING (${f.auditor} / ${f.lens}): ${f.title}\n${f.finding}\nevidence: ${f.evidence}\n\nReturn the verdict.`,
    { label: `verify:${(f.title || '').slice(0, 18)}`, phase: 'Verify', schema: VERIFY_SCHEMA }
  ).then((v) => ({ ...f, v }))
))
const confirmed = verified.filter(Boolean).filter((f) => f.v?.material && f.v?.is_factual && f.v?.corrected_severity !== 'non-issue')
const lighter = findings.filter((f) => f.severity === 'moderate' || f.severity === 'minor')
log(`Verify: ${confirmed.length}/${heavy.length} heavy findings confirmed. + ${lighter.length} lighter recorded.`)

// ── Phase 4: Synthesize ───────────────────────────────────────────────────────
phase('Synthesize')
const rank = { critical: 0, major: 1, moderate: 2, minor: 3, 'non-issue': 9 }
const sev = (f) => f.v?.corrected_severity ?? f.severity
confirmed.sort((a, b) => (rank[sev(a)] ?? 5) - (rank[sev(b)] ?? 5))
const byPersona = (name) => confirmed.filter((f) => f.auditor === name)
  .map((f) => `- [${sev(f)}|${f.lens}] ${f.title} — ${f.evidence} :: fix: ${f.recommendation}`).join('\n') || '(none confirmed)'
const verdict = await agent(
  `${CTX}\n\nYou are the founder's chief engineer summarizing the panel. Three auditors reviewed the engineering. Write a brutal, fair verdict: a 2-3 sentence overall engineering health call, then ONE sharp paragraph per auditor (Musk: what to DELETE + the worst idiot-index; Thiel: is the moat real + the #1 hidden landmine; House: the biggest LIE the code tells). End with the single highest-leverage fix. Specific, no flattery.\n\nMUSK confirmed:\n${byPersona('Musk')}\n\nTHIEL confirmed:\n${byPersona('Thiel')}\n\nHOUSE confirmed:\n${byPersona('House')}`,
  { label: 'panel-verdict', phase: 'Synthesize' }
)

return {
  summary: {
    recon: recon.length, raw: allFindings.length, deduped: findings.length,
    confirmed: confirmed.length, lighter: lighter.length,
    by_severity: confirmed.reduce((a, f) => { const s = sev(f); a[s] = (a[s] || 0) + 1; return a }, {}),
    by_auditor: confirmed.reduce((a, f) => { a[f.auditor] = (a[f.auditor] || 0) + 1; return a }, {}),
  },
  panel_verdict: verdict,
  confirmed_findings: confirmed.map((f) => ({
    title: f.title, severity: sev(f), auditor: f.auditor, lens: f.lens,
    finding: f.finding, evidence: f.evidence, why_it_matters: f.why_it_matters,
    recommendation: f.recommendation, verify_note: f.v?.note,
  })),
  lighter_findings: lighter.map((f) => ({ title: f.title, severity: f.severity, auditor: f.auditor, lens: f.lens, evidence: f.evidence, recommendation: f.recommendation })),
}
