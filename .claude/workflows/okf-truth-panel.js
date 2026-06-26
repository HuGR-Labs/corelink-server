export const meta = {
  name: 'okf-truth-panel',
  description: 'The N-lens truth-verification panel for the OKF wiki (docs/knowledge). For EACH concept it runs N=3 INDEPENDENT verifiers, each a DIFFERENT lens (TRUTH / GROUNDING+SOURCE_FILES / COMPLETENESS+overstatement), read-only + refute-stance, each opening the cited path:line. Because the lenses are DIVERSE (each authoritative for a different dimension — a real undeclared-enforcer finding lives in exactly ONE lens), the default aggregation is UNION: every lens finding survives (deduped by normalized (cite, claim-gist)); a concept is panel-VERIFIED when NO lens emits a finding. Lenses that DISAGREE on a cite (one flags it, another explicitly cleared/verified it) are surfaced on a per-concept "adjudicate" list for lead adjudication. An OPTIONAL same-lens-redundancy MAJORITY mode (run each lens N times, keep a finding only if a majority of that lens\'s own runs produced it — a denoiser) is available behind `majority`/`redundancy=N` args. The rigor ceiling above the single-lens 2026-06-26 truth audit. Reusable on demand: Workflow({name:"okf-truth-panel", args:["auth/pat-moat", ...]}) — args = concept ids, a taxonomy dir (e.g. "auth"), or "all" (+ optional `majority` / `redundancy=N`).',
  phases: [
    { title: 'Resolve', detail: 'Expand args (ids / a taxonomy dir / "all") into a concrete list of concept ids under docs/knowledge' },
    { title: 'Panel', detail: 'Per concept: 3 independent diverse-lens verifiers (TRUTH, GROUNDING+SOURCE_FILES, COMPLETENESS+overstatement) open the cited path:line and refute (each lens optionally run N times in majority mode)' },
    { title: 'Aggregate', detail: 'Default diverse-lens = UNION of all lens findings (dedup by (cite, claim-gist)); concept VERIFIED when none survive; cites where lenses disagree (flagged vs cleared) go to an adjudicate list' },
  ],
}

// ---------------------------------------------------------------------------
// pipeline(items, perItemFn): run perItemFn(item) for every item concurrently,
// each item aggregating INDEPENDENTLY as its own work completes — there is no
// global barrier before a concept's verdict is produced. Realized on the harness
// `parallel` primitive (the exact dispatch-on-ready idiom the sibling workflows
// use: parallel(items.map(x => () => ...))). If the harness later exposes a native
// pipeline(), this is a drop-in superset. This is what gives "each concept's 3
// lenses run and aggregate as it completes (no global barrier)".
const pipeline = (items, perItemFn) => parallel(items.map((it) => () => perItemFn(it)))

// ---------------------------------------------------------------------------
// Normalize args into a single resolve-spec STRING for the resolver agent. args
// may arrive as an array of ids, a single id/dir/"all" string, a comma-joined
// string, or a JSON string (the tool layer sometimes stringifies). Be defensive:
// a silent wrong default would audit the wrong set.
function parseSpec(a) {
  if (a == null) return 'all'
  let v = a
  if (typeof v === 'string') {
    const s = v.trim()
    if (!s) return 'all'
    if (s.startsWith('[') || s.startsWith('{') || s.startsWith('"')) {
      try { v = JSON.parse(s) } catch { return s }
    } else {
      return s
    }
  }
  if (Array.isArray(v)) return v.map((x) => String(x).trim()).filter(Boolean).join(', ')
  if (v && Array.isArray(v.concepts)) return v.concepts.map((x) => String(x).trim()).filter(Boolean).join(', ')
  if (v && typeof v.spec === 'string') return v.spec.trim()
  if (typeof v === 'object') return 'all'
  return String(v).trim() || 'all'
}

// ---------------------------------------------------------------------------
// Extract aggregation-MODE control tokens from args (so they don't leak into the
// concept spec). Default = diverse-lens UNION (per-lens redundancy R=1). The
// OPTIONAL same-lens-redundancy MAJORITY denoiser is opted into with a `majority`
// token (default R=3) and/or `redundancy=N` / `redundancy:N`. Returns the cleaned
// args (mode tokens stripped) plus the resolved redundancy R.
//   R=1  → union mode: every lens finding is kept (majority-of-1 keeps all).
//   R>=2 → majority mode: within EACH lens, keep a finding only if it appears in
//          a MAJORITY (floor(R/2)+1) of that lens's own N runs, THEN union across
//          lenses. (Cross-lens is ALWAYS union — never a cross-lens >=2/3 gate.)
function extractMode(a) {
  const toks = Array.isArray(a) ? a.slice() : (a == null ? [] : [a])
  let redundancy = 1
  let wantsMajority = false
  const kept = []
  for (const raw of toks) {
    const t = String(raw == null ? '' : raw).trim()
    const lo = t.toLowerCase()
    const mR = lo.match(/^(?:--)?redundancy[=:](\d+)$/)
    if (lo === 'majority' || lo === '--majority') { wantsMajority = true; continue }
    if (lo === 'union' || lo === '--union') { wantsMajority = false; redundancy = 1; continue }
    if (mR) { redundancy = Math.max(1, parseInt(mR[1], 10) || 1); continue }
    kept.push(raw)
  }
  if (wantsMajority && redundancy < 2) redundancy = 3 // default majority fan-out
  return { cleanArgs: Array.isArray(a) ? kept : (kept[0] ?? a), redundancy }
}
const { cleanArgs, redundancy: REDUNDANCY } = extractMode(args)
const MAJORITY = Math.floor(REDUNDANCY / 2) + 1 // per-lens majority threshold
const spec = parseSpec(cleanArgs)

const ROOT = 'docs/knowledge'

// ---------------------------------------------------------------------------
// Schemas.
const RESOLVE_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['concepts'],
  properties: { concepts: { type: 'array', items: {
    type: 'object', additionalProperties: false, required: ['id', 'path'],
    properties: {
      id: { type: 'string', description: 'concept id = path under docs/knowledge without .md, e.g. auth/pat-moat' },
      path: { type: 'string', description: 'repo-relative file path, e.g. docs/knowledge/auth/pat-moat.md' },
    },
  } } },
}

// One lens verifier returns a list of findings (empty = the lens found nothing)
// PLUS the cites it OPENED and explicitly CLEARED (verified as sound). The cleared
// set powers the cross-lens "adjudicate" list: a cite one lens flags while another
// lens cleared it is a genuine lens DISAGREEMENT that a lead must adjudicate.
const LENS_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['findings'],
  properties: {
    findings: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      required: ['claim', 'cite', 'dimension', 'severity', 'why'],
      properties: {
        claim: { type: 'string', description: 'the exact prose claim being challenged, short + verbatim-ish' },
        cite: { type: 'string', description: 'the file:line the claim cites (or "UNCITED" if the claim has no citation)' },
        dimension: { type: 'string', enum: ['TRUTH', 'GROUNDING', 'COMPLETENESS', 'SOURCE_FILES'] },
        severity: { type: 'string', enum: ['BLOCKER', 'MAJOR', 'MINOR'] },
        why: { type: 'string', description: 'concrete: what the cited code/doc ACTUALLY does vs what the prose says, with the real line evidence' },
      },
    } },
    cleared_cites: { type: 'array', description: 'cites (file:line) you OPENED under this lens and explicitly CLEARED — the cited lines genuinely support the claim. Used to detect cross-lens disagreement.', items: { type: 'string' } },
  },
}

// The 3 DISTINCT lenses (NOT three identical verifiers). Each reads the whole
// concept + all cited code, but hunts under a different primary stance.
const LENSES = [
  {
    key: 'TRUTH',
    brief:
      'LENS 1 — TRUTH. Does the cited code/doc ACTUALLY do what the prose says? For every substantive claim, OPEN the cited path:line and the surrounding context and read the REAL code. Flag: wrong mechanism, misread of what the code does, a claimed behavior the code does not exhibit, a guard asserted that is not actually there. Default skeptic: assume the claim is wrong until the cited lines prove it as stated.',
  },
  {
    key: 'GROUNDING_SOURCEFILES',
    brief:
      'LENS 2 — GROUNDING + SOURCE_FILES. Do the citations RESOLVE to the implementing lines (not a module doc-comment / rustdoc / re-export index / blank line gaming the structural check), and does the frontmatter source_files set actually COVER the enforcing code? Flag: a cite that points at the wrong/weak line while the real behavior is a few lines away; a load-bearing invariant whose enforcer lives in an UNDECLARED file (drift-uncovered); a fabricated/nonexistent symbol or path. Open every cite + check it shows the thing.',
  },
  {
    key: 'COMPLETENESS_OVERSTATEMENT',
    brief:
      'LENS 3 — COMPLETENESS + OVERSTATEMENT. Is any key behavioral/security claim UNCITED? Does the prose OVERSTATE — claim the system builds/does something that is actually deferred, partial, intra-tenant-only, or a tracked-but-unbuilt enhancement (claims-built-what-is-deferred)? Any fabricated capability? Flag uncited assertions of behavior and any prose that overreaches what the code supports.',
  },
]

// ---------------------------------------------------------------------------
// Normalize a finding into a dedup key so the UNION can collapse the same
// claim/cite reported by multiple lenses (or multiple runs of one lens) into one
// surviving finding. The cite (file:line) is the most stable signal; the claim
// gist disambiguates two distinct findings that share one cite.
function normCite(s) {
  const m = String(s || '').match(/([\w./-]+):(\d+)/)
  if (m) return `${m[1].toLowerCase()}:${m[2]}`
  return String(s || '').toLowerCase().replace(/[^a-z0-9/._-]/g, '').slice(0, 48) || 'uncited'
}
function claimGist(s) {
  return String(s || '').toLowerCase().replace(/[^a-z0-9 ]/g, ' ')
    .split(/\s+/).filter(Boolean).slice(0, 6).join(' ')
}
const findingKey = (f) => `${normCite(f.cite)}::${claimGist(f.claim)}`

// ---------------------------------------------------------------------------
phase('Resolve')
log(`okf-truth-panel — resolve spec: ${spec.slice(0, 80)}`)
const resolved = await agent(
  `You resolve an OKF-wiki concept selection spec to a concrete file list. The wiki lives under \`${ROOT}/\` in THIS repo; every concept is a single Markdown file; a concept id is its path under \`${ROOT}\` WITHOUT the \`.md\` (e.g. the file \`${ROOT}/auth/pat-moat.md\` has id \`auth/pat-moat\`).\n\n` +
  `SPEC (comma-separated tokens): ${spec}\n\n` +
  `Resolve the tokens into the concrete set of concept files:\n` +
  `- A token "all" (or empty) => EVERY concept file under \`${ROOT}\` (recurse all taxonomy dirs). Use \`find ${ROOT} -name '*.md'\`.\n` +
  `- A bare taxonomy-dir token (no slash, e.g. "auth", "surfaces", "tenancy", "crates") => every \`*.md\` directly under \`${ROOT}/<dir>/\`.\n` +
  `- A token that looks like a concept id (has a slash, e.g. "auth/pat-moat") => that one file \`${ROOT}/<id>.md\`.\n` +
  `- A token already ending in \`.md\` or starting with \`${ROOT}/\` => normalize to the id + path.\n` +
  `Verify each resolved path actually EXISTS (\`ls\`/\`test -f\`); drop (do not invent) any that don't, but note nothing about missing ones — just omit. De-dup. Return the full concept list via the schema. Skip \`index.md\` / \`log.md\` only if the spec is "all" (they are wiki indexes, not concepts); include them if explicitly named.`,
  { label: 'resolve', phase: 'Resolve', schema: RESOLVE_SCHEMA, effort: 'low' }
).catch(() => null)

const concepts = (resolved && Array.isArray(resolved.concepts) ? resolved.concepts : [])
  .filter((c) => c && c.id && c.path)
// de-dup by id (resolver may double-count overlapping tokens)
const seenIds = new Set()
const targets = concepts.filter((c) => {
  const k = c.id.toLowerCase().trim()
  if (seenIds.has(k)) return false
  seenIds.add(k); return true
})
log(`resolved ${targets.length} concept(s)`)
if (targets.length === 0) return { spec, concepts: 0, results: [], note: 'spec resolved to no existing concepts' }

// ---------------------------------------------------------------------------
phase('Panel')
log(`aggregation: ${REDUNDANCY > 1 ? `MAJORITY denoiser (per-lens R=${REDUNDANCY}, keep ≥${MAJORITY}/R) → UNION across lenses` : 'diverse-lens UNION (R=1)'}`)

// Run ONE lens (optionally R times for the same-lens-redundancy MAJORITY denoiser)
// and return that lens's findings + cleared cites. With R=1 (default) this is a
// single pass and every finding is kept. With R>=2 a finding is kept only if it
// recurs in a MAJORITY of that lens's OWN runs — denoising a single flaky lens
// WITHOUT requiring agreement from a DIFFERENT lens (cross-lens stays UNION).
const runLens = async (c, lens) => {
  const promptFor = () => agent(
    `You are an INDEPENDENT, READ-ONLY, REFUTE-STANCE verifier on the OKF-wiki truth panel. You audit ONE concept under ${lens.key}.\n\n` +
    `CONCEPT: ${c.id}\nFILE: ${c.path}\n\n` +
    `${lens.brief}\n\n` +
    `PROTOCOL:\n` +
    `1. Read ${c.path} fully (prose claims + the \`# Citations\` list + the frontmatter \`source_files\`).\n` +
    `2. For EVERY claim relevant to your lens, OPEN the exact \`path:line\` it cites in THIS repo and read the real code/doc at and around those lines. Never judge from memory — judge from the file.\n` +
    `3. Default to "the claim is FINE" and only emit a finding when the cited lines genuinely fail your lens (false / mis-grounded / uncited / overstated). Be specific: name what the code ACTUALLY does vs what the prose says, with the real line.\n` +
    `4. You are AUTHORITATIVE for ${lens.key}: this panel UNIONs lens findings, so a real defect in YOUR dimension survives on your word alone — do NOT suppress it just because another lens is unlikely to also see it. Tag each finding with its true \`dimension\`.\n` +
    `5. Severity: BLOCKER = a factually false claim (prose contradicts code); MAJOR = overstated/misleading or grounded in the wrong/weak line, or an undeclared enforcing source_file; MINOR = imprecise wording / weak-but-not-wrong cite / polish.\n` +
    `6. ALSO return \`cleared_cites\`: the file:line cites you OPENED under this lens and explicitly CLEARED (the cited lines genuinely support the claim). This lets the panel surface cross-lens DISAGREEMENT (a cite you cleared that another lens flags) for lead adjudication.\n\n` +
    `Return via the schema. Empty findings is the expected, correct answer for a sound concept — do NOT invent issues to fill the list.`,
    { label: `panel:${c.id}:${lens.key}`, phase: 'Panel', schema: LENS_SCHEMA, effort: 'high' }
  ).then((r) => ({
    findings: (r && Array.isArray(r.findings)) ? r.findings : [],
    cleared: (r && Array.isArray(r.cleared_cites)) ? r.cleared_cites : [],
  })).catch(() => ({ findings: [], cleared: [] }))

  if (REDUNDANCY <= 1) {
    const r = await promptFor()
    return { lens: lens.key, findings: r.findings, cleared: r.cleared.map(normCite) }
  }
  // MAJORITY denoiser: R runs of THIS lens; keep findings recurring in ≥MAJORITY runs.
  const runs = await parallel(Array.from({ length: REDUNDANCY }, () => () => promptFor()))
  const tally = new Map()
  for (const run of runs) {
    const seenThisRun = new Set() // one run = at most one vote per finding key
    for (const f of run.findings) {
      const key = findingKey(f)
      if (seenThisRun.has(key)) continue
      seenThisRun.add(key)
      let cl = tally.get(key)
      if (!cl) { cl = { count: 0, variants: [] }; tally.set(key, cl) }
      cl.count += 1; cl.variants.push(f)
    }
  }
  const rank = { BLOCKER: 3, MAJOR: 2, MINOR: 1 }
  const findings = []
  for (const cl of tally.values()) {
    if (cl.count >= MAJORITY) {
      findings.push(cl.variants.slice().sort((a, b) => (rank[b.severity] || 0) - (rank[a.severity] || 0))[0])
    }
  }
  const cleared = [...new Set(runs.flatMap((r) => r.cleared.map(normCite)))]
  return { lens: lens.key, findings, cleared }
}

// pipeline: each concept's 3 lenses run + aggregate INDEPENDENTLY as the concept
// completes — no global barrier across concepts.
const results = await pipeline(targets, async (c) => {
  const lensRuns = await parallel(LENSES.map((lens) => () => runLens(c, lens)))

  // ---- Aggregate (per concept, as it completes): diverse-lens UNION ----
  // Each lens is authoritative for its own dimension, so we UNION every lens
  // finding, deduping by (cite, claim-gist). NO cross-lens >=2/3 gate — that
  // would suppress a real undeclared-enforcer finding that only ONE lens (by
  // construction) can see. We still record WHICH lenses flagged each cluster.
  const clusters = new Map()
  for (const run of lensRuns) {
    for (const f of run.findings) {
      const key = findingKey(f)
      let cl = clusters.get(key)
      if (!cl) { cl = { key, lenses: new Set(), variants: [] }; clusters.set(key, cl) }
      cl.lenses.add(run.lens)
      cl.variants.push({ ...f, lens: run.lens })
    }
  }
  const rank = { BLOCKER: 3, MAJOR: 2, MINOR: 1 }
  const surviving = []
  for (const cl of clusters.values()) {
    // UNION: every cluster survives (≥1 lens flagged it). Represent with its
    // highest-severity variant; record corroborating lenses (informational).
    const top = cl.variants.slice().sort((a, b) => (rank[b.severity] || 0) - (rank[a.severity] || 0))[0]
    surviving.push({
      claim: top.claim,
      cite: top.cite,
      dimension: top.dimension,
      severity: top.severity,
      why: top.why,
      flagged_by: [...cl.lenses].sort(),
      corroborating_lenses: cl.lenses.size,
    })
  }
  surviving.sort((a, b) => (rank[b.severity] || 0) - (rank[a.severity] || 0) || b.corroborating_lenses - a.corroborating_lenses)

  // ---- Adjudicate list: cross-lens DISAGREEMENT ----
  // A cite where one lens emitted a finding AND a DIFFERENT lens explicitly
  // CLEARED that same cite → the lenses disagree; a lead must adjudicate (don't
  // silently keep or drop it). Keyed on the normalized cite (file:line).
  const clearedBy = new Map() // normCite -> Set(lens)
  for (const run of lensRuns) {
    for (const nc of run.cleared) {
      if (!clearedBy.has(nc)) clearedBy.set(nc, new Set())
      clearedBy.get(nc).add(run.lens)
    }
  }
  const adjudicate = []
  const seenAdj = new Set()
  for (const cl of clusters.values()) {
    const nc = normCite([...cl.variants][0].cite)
    const clr = clearedBy.get(nc)
    if (!clr) continue
    const clearedOnly = [...clr].filter((l) => !cl.lenses.has(l)) // a DIFFERENT lens cleared it
    if (clearedOnly.length === 0 || seenAdj.has(nc)) continue
    seenAdj.add(nc)
    const top = cl.variants.slice().sort((a, b) => (rank[b.severity] || 0) - (rank[a.severity] || 0))[0]
    adjudicate.push({
      cite: top.cite,
      claim: top.claim,
      flagged_by: [...cl.lenses].sort(),
      cleared_by: clearedOnly.sort(),
      why: top.why,
    })
  }

  const verified = surviving.length === 0
  log(`${verified ? 'VERIFIED ' : 'FINDINGS '} ${c.id} — ${surviving.length} union finding(s)${adjudicate.length ? `, ${adjudicate.length} to adjudicate` : ''}`)
  return { id: c.id, path: c.path, verified, surviving_findings: surviving, adjudicate, raw_clusters: clusters.size }
})

phase('Aggregate')
const verifiedCount = results.filter((r) => r.verified).length
const flagged = results.filter((r) => !r.verified)
const adjudicateCount = results.reduce((n, r) => n + (r.adjudicate ? r.adjudicate.length : 0), 0)
log(`panel done — ${verifiedCount}/${results.length} VERIFIED, ${flagged.length} with finding(s), ${adjudicateCount} cite(s) to adjudicate`)

return {
  spec,
  mode: REDUNDANCY > 1 ? `majority(R=${REDUNDANCY})` : 'union',
  concepts: results.length,
  verified: verifiedCount,
  flagged: flagged.length,
  adjudicate: adjudicateCount,
  rule: REDUNDANCY > 1
    ? `diverse-lens UNION across lenses; within EACH lens a finding is kept only if it recurs in ≥${MAJORITY} of that lens's ${REDUNDANCY} same-lens runs (redundancy denoiser); concept VERIFIED when no lens finding survives; cites flagged by one lens but cleared by another are listed for adjudication`
    : 'diverse-lens UNION: every lens finding survives (deduped by (cite, claim-gist)) — each lens is authoritative for its dimension, NO cross-lens >=2/3 gate; concept VERIFIED when no lens emits a finding; cites flagged by one lens but cleared by another are listed for adjudication',
  results: results.map((r) => ({
    id: r.id,
    verified: r.verified,
    surviving_findings: r.surviving_findings,
    adjudicate: r.adjudicate,
  })),
}
