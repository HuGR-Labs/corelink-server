export const meta = {
  name: 'okf-truth-panel',
  description: 'The N-lens truth-verification panel for the OKF wiki (docs/knowledge). For EACH concept it runs N=3 INDEPENDENT verifiers, each a DIFFERENT lens (TRUTH / GROUNDING+SOURCE_FILES / COMPLETENESS+overstatement), read-only + refute-stance, each opening the cited path:line. A finding survives ONLY by majority vote (>=2 of 3 lenses independently flag the same claim/cite); a concept is panel-VERIFIED when no finding survives. The rigor ceiling above the single-lens 2026-06-26 truth audit. Reusable on demand: Workflow({name:"okf-truth-panel", args:["auth/pat-moat", ...]}) — args = concept ids, a taxonomy dir (e.g. "auth"), or "all".',
  phases: [
    { title: 'Resolve', detail: 'Expand args (ids / a taxonomy dir / "all") into a concrete list of concept ids under docs/knowledge' },
    { title: 'Panel', detail: 'Per concept: 3 independent diverse-lens verifiers (TRUTH, GROUNDING+SOURCE_FILES, COMPLETENESS+overstatement) open the cited path:line and refute' },
    { title: 'Vote', detail: 'Majority vote per concept: a finding survives only if >=2/3 lenses independently flag it; concept VERIFIED when none survive' },
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
const spec = parseSpec(args)

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

// One lens verifier returns a list of findings (empty = the lens found nothing).
const LENS_SCHEMA = {
  type: 'object', additionalProperties: false, required: ['findings'],
  properties: { findings: { type: 'array', items: {
    type: 'object', additionalProperties: false,
    required: ['claim', 'cite', 'dimension', 'severity', 'why'],
    properties: {
      claim: { type: 'string', description: 'the exact prose claim being challenged, short + verbatim-ish' },
      cite: { type: 'string', description: 'the file:line the claim cites (or "UNCITED" if the claim has no citation)' },
      dimension: { type: 'string', enum: ['TRUTH', 'GROUNDING', 'COMPLETENESS', 'SOURCE_FILES'] },
      severity: { type: 'string', enum: ['BLOCKER', 'MAJOR', 'MINOR'] },
      why: { type: 'string', description: 'concrete: what the cited code/doc ACTUALLY does vs what the prose says, with the real line evidence' },
    },
  } } },
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
// Normalize a finding into a dedup key so we can count DISTINCT lenses that
// independently land on the same claim/cite. The cite (file:line) is the most
// stable cross-lens signal; the claim gist disambiguates two findings on one cite.
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
// pipeline: each concept's 3 lenses run + aggregate (vote) INDEPENDENTLY as the
// concept completes — no global barrier across concepts.
const results = await pipeline(targets, async (c) => {
  const lensRuns = await parallel(LENSES.map((lens) => () =>
    agent(
      `You are an INDEPENDENT, READ-ONLY, REFUTE-STANCE verifier on the OKF-wiki truth panel. You audit ONE concept under ${lens.key}.\n\n` +
      `CONCEPT: ${c.id}\nFILE: ${c.path}\n\n` +
      `${lens.brief}\n\n` +
      `PROTOCOL:\n` +
      `1. Read ${c.path} fully (prose claims + the \`# Citations\` list + the frontmatter \`source_files\`).\n` +
      `2. For EVERY claim relevant to your lens, OPEN the exact \`path:line\` it cites in THIS repo and read the real code/doc at and around those lines. Never judge from memory — judge from the file.\n` +
      `3. Default to "the claim is FINE" and only emit a finding when the cited lines genuinely fail your lens (false / mis-grounded / uncited / overstated). Be specific: name what the code ACTUALLY does vs what the prose says, with the real line.\n` +
      `4. Your PRIMARY hunt is ${lens.key}, but if you are confident about a defect in another dimension, report it too (cross-lens corroboration is how findings survive the vote) — tag each finding with its true \`dimension\`.\n` +
      `5. Severity: BLOCKER = a factually false claim (prose contradicts code); MAJOR = overstated/misleading or grounded in the wrong/weak line, or an undeclared enforcing source_file; MINOR = imprecise wording / weak-but-not-wrong cite / polish.\n\n` +
      `Return via the schema. Empty findings is the expected, correct answer for a sound concept — do NOT invent issues to fill the list.`,
      { label: `panel:${c.id}:${lens.key}`, phase: 'Panel', schema: LENS_SCHEMA, effort: 'high' }
    ).then((r) => ({ lens: lens.key, findings: (r && Array.isArray(r.findings)) ? r.findings : [] }))
     .catch(() => ({ lens: lens.key, findings: [] }))
  ))

  // ---- Vote (per concept, as it completes) ----
  // Cluster findings by (cite, claim-gist); a cluster SURVIVES only if >=2
  // DISTINCT lenses independently produced a finding in it. One lens emitting
  // the same finding twice does NOT count as two votes.
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
  const surviving = []
  for (const cl of clusters.values()) {
    if (cl.lenses.size >= 2) {
      // Represent the surviving finding with its highest-severity variant.
      const rank = { BLOCKER: 3, MAJOR: 2, MINOR: 1 }
      const top = cl.variants.slice().sort((a, b) => (rank[b.severity] || 0) - (rank[a.severity] || 0))[0]
      surviving.push({
        claim: top.claim,
        cite: top.cite,
        dimension: top.dimension,
        severity: top.severity,
        why: top.why,
        flagged_by: [...cl.lenses].sort(),
        votes: cl.lenses.size,
      })
    }
  }
  surviving.sort((a, b) => b.votes - a.votes)

  const verified = surviving.length === 0
  log(`${verified ? 'VERIFIED ' : 'FINDINGS '} ${c.id} — ${surviving.length} surviving / ${clusters.size} raw cluster(s)`)
  return { id: c.id, path: c.path, verified, surviving_findings: surviving, raw_clusters: clusters.size }
})

phase('Vote')
const verifiedCount = results.filter((r) => r.verified).length
const flagged = results.filter((r) => !r.verified)
log(`panel done — ${verifiedCount}/${results.length} VERIFIED, ${flagged.length} with surviving finding(s)`)

return {
  spec,
  concepts: results.length,
  verified: verifiedCount,
  flagged: flagged.length,
  rule: 'a finding survives only if >=2 of 3 independent diverse-lens verifiers flag the same claim/cite; concept VERIFIED when none survive',
  results: results.map((r) => ({
    id: r.id,
    verified: r.verified,
    surviving_findings: r.surviving_findings,
  })),
}
