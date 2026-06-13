// CoreLink Audit Team — saved 2026-06-13. Invoke with:
//   Workflow({ name: "corelink-audit-team" })
// CAA-360 framework: PTES + OWASP/ASVS + MITRE ATT&CK + multi-agent adversarial verification.
// 16 agents (3 Opus pentest + 3 Opus + 5 Sonnet + 5 Haiku review) over 11 attack surfaces,
// every finding adversarially confirmed/refuted + fully chewed (explanation/repro/impact).
export const meta = {
  name: 'corelink-audit-team',
  description: 'CoreLink Audit Team (CAA-360): brutal pen-test + 360° multi-perspective security/bug/gap review (3 Opus pentest + 3 Opus + 5 Sonnet + 5 Haiku review), adversarially verified, fully-documented findings',
  whenToUse: 'On-demand adversarial security + quality assurance of the CoreLink codebase (post-merge, pre-release, or periodic)',
  phases: [
    { title: 'Assess', detail: '16 agents (pentest + review) across 11 attack surfaces, distinct lenses' },
    { title: 'Verify', detail: 'adversarial confirm/refute of every finding (false-positive kill)' },
    { title: 'Synthesize', detail: 'dedupe + CVSS-score + prioritized launch-risk report' },
  ],
}

// ─── CAA-360 framework (shared brief preamble) ───────────────────────────────
const FRAMEWORK = `
You are part of CAA-360 (CoreLink Adversarial Assurance, 360°) — an AUTHORIZED internal
security + quality assessment of CoreLink, the owner's own product (defensive security,
full authorization). Methodology = PTES lifecycle + OWASP/ASVS depth + MITRE ATT&CK TTPs.
Operate a Perceive→Reason→Act→Observe loop: READ the real code, REASON about an attacker's
path, gather EVIDENCE (exact file:line + code quotes), and only then conclude.

CoreLink = multi-tenant content-addressable cache + storage-governance on Cloudflare
(edge Worker [TS] + Durable Object that boots a native Rust CONTAINER + R2 + D1 + KV).
~71 Rust crates. Surfaces: native CAS/AC, Bazel REAPI v2, Turborepo, sccache; Stripe billing;
Clerk auth; PAT (HMAC) tenant auth; internal-auth between worker↔container; DSR/GDPR erasure.

RIGOR BAR (this is SOTA — be brutal but HONEST):
- A finding MUST cite exact file:line + quote the vulnerable code. No hand-waving, no
  "could potentially". If you can't point at the code, it's not a finding.
- Prefer DEPTH over breadth in your assigned surface. Chase the exploit chain to ground.
- Severity = CVSS 3.1-style reasoning. Tenant-isolation breaks, auth bypass, money-path,
  and secret-leak are CRITICAL/HIGH by default IF real.
- Mark confidence honestly. A plausible-but-unverified suspicion = confidence "low".
- Multi-tenant focus: BOLA (broken object-level authz), cross-tenant read/write, the
  PAT→tenant_id mapping, R2 key/prefix derivation, residency-trigger enforcement.
- Do NOT run destructive commands or hit live prod. This is a CODE-level assessment
  (you may read files, grep, and reason; do not mutate anything).

DOCUMENTATION BAR (owner mandate — every finding MUST be fully CHEWED, not terse):
- 'explanation': plain-language, step-by-step — what the bug IS, WHY it matters, and HOW it is
  exploited, written so a non-security stakeholder fully understands. No jargon-only one-liners.
- 'reproduction': a concrete step-by-step PoC / repro outline (exact request/sequence/conditions).
- 'impact': the concrete blast radius if exploited (which tenants/data/money, how bad).
- 'evidence': the exact vulnerable code QUOTED + file:line.
- 'recommendation': a concrete fix (ideally the corrected code or the precise change).
A finding that is not fully documented + explained + chewed is INCOMPLETE.
Return ONLY findings you can defend with code evidence.`

// ─── The 16-agent matrix (distinct surfaces + lenses for 360° coverage) ──────
const AGENTS = [
  // 3 OPUS — PEN-TEST (exploitation-first, "break it")
  { id: 'PEN-1', model: 'opus', label: 'pentest:tenant-isolation',
    brief: `ROLE: PEN-TEST (exploitation-first). SURFACE: Tenant isolation & BOLA — the #1 risk.
    GOAL: construct a concrete CROSS-TENANT access path (read OR write another tenant's CAS blobs
    / AC entries / D1 rows). Trace: PAT verification → tenant_id derivation → R2 key/prefix
    (derive_prefix / TDK / the raw-padded fallback) → blob_key layout → the residency triggers.
    Look for: missing tenant scoping on a route, a prefix collision, a fallback-derivation that
    two tenants can share, an AC key not namespaced by tenant, the negative-cache leaking,
    session-exchange minting a PAT for the wrong tenant. START: crates/corelink-container/src/routes/cas.rs,
    ac.rs; the PatVerifier crate; storage.rs (R2S3Client::blob_key / derive_prefix); migrations
    residency triggers; worker/src/lib/session_exchange.ts. Hunt broadly within this surface.` },
  { id: 'PEN-2', model: 'opus', label: 'pentest:auth-chain',
    brief: `ROLE: PEN-TEST. SURFACE: AuthN/AuthZ full-chain bypass.
    GOAL: forge/bypass auth. Attack: PAT format + HMAC (can you mint/replay a PAT? timing?),
    the internal-auth constant-time gate between worker↔container (does the Worker STRIP inbound
    x-admin-*/x-tenant-id headers? can a client inject them?), admin-scope escalation, the
    clerk→PAT session-exchange, Clerk JWKS/JWT validation (alg confusion, kid, exp, aud).
    Examine every 401 vs 403 vs 503 fail-mode (fail-open?). START: the PAT verifier crate,
    worker/src/index.ts (internalAuthKey ~1195/1291), durable_object.ts, crates/corelink-container/src/routes/admin.rs,
    auth_introspect.rs, session_exchange.ts, the clerk crates.` },
  { id: 'PEN-3', model: 'opus', label: 'pentest:money+poisoning',
    brief: `ROLE: PEN-TEST. SURFACE: Money path + cache poisoning.
    GOAL-A (billing): bypass payment / escalate tier for free. Check getTierForTenant (the
    subscription_state='active' filter — can a pending_checkout get a paid tier?), Stripe webhook
    signature forgery + REPLAY, price-id / tier tampering in the tier-select request body,
    checkout success_url open-redirect, idempotency abuse.
    GOAL-B (poisoning): can tenant A poison tenant B's cache? Is the AC write-restricted / readonly
    where it must be? digest charset/length validation, hash confusion, the 409-on-divergent-body
    guard, content-integrity (does a GET verify the returned bytes hash to the requested digest?).
    START: tier_select_checkout.rs, worker/src/lib/quota.ts, apps/signup-worker/src/webhooks/stripe.ts,
    customer_d1.rs, ac.rs + handler-ac, cas.rs.` },

  // 3 OPUS — REVIEW (deep bug/gap hunting)
  { id: 'REV-O1', model: 'opus', label: 'review:dsr+residency+env',
    brief: `ROLE: DEEP REVIEW. SURFACE: DSR/GDPR completeness + residency + worker↔container env-contract.
    Check: does DSR erasure cover ALL PII tables + ALL 5 R2 regions? the ERASURE_SALT_KEY fallback
    (predictable salt if unset?), tombstone bypass, re-identification risk. RESIDENCY: the known
    Schrems-II gap (container CAS uses a GLOBAL R2_CAS_REGION, ignoring tenant.primary_region) —
    confirm scope + blast radius. ENV-CONTRACT: durable_object.ts container.start({env}) forward-list
    vs every env::var the container reads (any set-but-not-forwarded var = silent break).
    START: crates/corelink-container/src/routes/dsr/*, corelink-privacy-erasure-worker, cas_erase.rs,
    durable_object.ts, storage.rs region logic.` },
  { id: 'REV-O2', model: 'opus', label: 'review:ssrf+secrets+boundary',
    brief: `ROLE: DEEP REVIEW. SURFACE: SSRF/egress + secret handling + worker↔container boundary.
    The container runs with enableInternet:true and reaches D1 (HTTP API) + R2 (S3 API) over the
    public internet — can any request CONTROL the egress target (SSRF to CF metadata / internal /
    attacker host)? Check d1_http.rs + storage.rs URL construction. SECRETS: Debug impls on
    secret-holding structs (StorageEnv, D1HttpClient, RSA keys), tracing/log/error messages that
    echo secrets, the internal-auth key concentration (one key gates everything?). START:
    durable_object.ts, crates/corelink-container/src/storage.rs, storage/d1_http.rs, the outbound-call sites.` },
  { id: 'REV-O3', model: 'opus', label: 'review:injection+dos+logic',
    brief: `ROLE: DEEP REVIEW. SURFACE: Injection + DoS/quota + business-logic.
    INJECTION: every D1 query — parameterized or string-built (SQLi)? path traversal via CAS
    key/digest, header/log injection. DoS/QUOTA: the per-tenant $-ceiling guard (bypass? atomic
    accrual race / TOCTOU?), the session-exchange throttle (bypass?), unbounded allocations /
    request-size limits, ReDoS. LOGIC: tier upgrade/downgrade races, idempotency keys, replay,
    state-machine gaps. START: all D1 execute/query sites (grep across crates), the quota guard,
    session_exchange throttle, digest validation, tier_select.` },

  // 5 SONNET — REVIEW (per-domain breadth)
  { id: 'REV-S1', model: 'sonnet', label: 'review:container-routes',
    brief: `ROLE: REVIEW. SURFACE: ALL container route handlers (crates/corelink-container/src/routes/**).
    For EACH handler: auth gate present + correct? input validated? fail-open vs fail-closed on error?
    tenant scoping? status codes leak info? unhandled errors? Flag logic bugs, missing checks, and
    any route mounted without its guard. Be systematic — walk every route file.` },
  { id: 'REV-S2', model: 'sonnet', label: 'review:worker-ts',
    brief: `ROLE: REVIEW. SURFACE: Worker TS layer (worker/src/**).
    Review durable_object.ts (DO lifecycle, container start/health, env forward), index.ts (routing,
    internal-auth, header handling), lib/* (session_exchange, quota.ts, any crypto). Look for: header
    trust/injection, missing auth on a route, DO state races, KV/D1 misuse, error fail-open, the
    container boundary. Flag bugs + gaps with file:line.` },
  { id: 'REV-S3', model: 'sonnet', label: 'review:cache-adapters',
    brief: `ROLE: REVIEW. SURFACE: Cache adapters + cache surfaces — routes/{cargo,brew,npm,pip,oci},
    bazel_v2.rs (REAPI v2), turbo_v8.rs (Turborepo), sccache/WebDAV. Check: PAT scope enforcement
    per adapter, dependency-confusion / namespace-squatting via the cache, upstream-proxy SSRF, auth
    on read vs write, the unified PatVerifier + 2-level moat. Flag any adapter that under-scopes or
    proxies untrusted upstreams unsafely.` },
  { id: 'REV-S4', model: 'sonnet', label: 'review:crypto-auth',
    brief: `ROLE: REVIEW. SURFACE: Crypto + auth primitives. The PAT verifier (HMAC construction,
    constant-time compare, format parsing, error types InvalidPat→401 vs Backend→503), any Argon2id,
    key derivation (the TDK / derive_prefix), signing keys (PAT_SIGNING_KEY, OCI/signup token keys),
    randomness (getrandom usage). Look for: non-constant-time compares, weak/missing validation,
    key reuse, predictable derivation, signature-verification bypasses. Grep the auth/crypto crates.` },
  { id: 'REV-S5', model: 'sonnet', label: 'review:billing+migrations',
    brief: `ROLE: REVIEW. SURFACE: Billing/Stripe + D1 migrations. Review corelink-stripe-real,
    tier_select*, customer_d1.rs, the Stripe lifecycle + webhook handling, and the migrations
    0064-0068 (esp. 0064's tenant-table rebuild: the FK/trigger handling, legacy_alter_table, the
    recreated trg_tenant_* triggers — is the rebuild truly additive + correct? any data-loss or
    constraint gap?). Flag billing-integrity bugs, webhook gaps, migration correctness issues.` },

  // 5 HAIKU — REVIEW (focused mechanical breadth)
  { id: 'REV-H1', model: 'haiku', label: 'review:panic-failmode',
    brief: `ROLE: FOCUSED SCAN. Find unwrap()/expect()/panic!/unreachable! on REQUEST-HANDLING paths
    (a panic = DoS/availability), and fail-OPEN error handling where it must fail-CLOSED. Grep the
    container + worker crates. Report each with file:line + why it is reachable from untrusted input.` },
  { id: 'REV-H2', model: 'haiku', label: 'review:secret-leak',
    brief: `ROLE: FOCUSED SCAN. Find secret LEAKAGE: #[derive(Debug)] on structs holding keys/tokens/
    credentials, tracing/println/log/eprintln of sensitive values, error messages that echo secrets,
    secrets in URLs/headers that get logged. Grep for Debug derives near key/token/secret fields and
    log macros near sensitive vars. Report file:line.` },
  { id: 'REV-H3', model: 'haiku', label: 'review:hardcoded-creds',
    brief: `ROLE: FOCUSED SCAN. Find hardcoded credentials / insecure defaults: literal keys/tokens/
    passwords in source, test keys (sk_test_/pk_test_) reachable on prod paths, insecure env-var
    defaults (unwrap_or with a permissive value), "TODO security" markers. Grep src + config. Report
    file:line. (Ignore obvious test-only fixtures clearly under tests/.)` },
  { id: 'REV-H4', model: 'haiku', label: 'review:supply-chain',
    brief: `ROLE: FOCUSED SCAN. Supply-chain: review deny.toml + .cargo/audit.toml advisory IGNORES
    (is each waiver still justified / not masking a real prod-exposed CVE?), Cargo.toml for unpinned
    or risky deps, and the cache-adapter upstream endpoints (cargo/npm/pip/brew/oci) for untrusted
    fetch. Report each questionable waiver/dep with file:line + reasoning.` },
  { id: 'REV-H5', model: 'haiku', label: 'review:claim-verify',
    brief: `ROLE: FOCUSED SCAN (claim verification). For every code COMMENT claiming a security
    property — "fail-CLOSED", "constant-time", "redacted", "validated", "tenant-scoped",
    "additive" — VERIFY the adjacent code actually does it; report MISMATCHES (comment says X,
    code does NOT). Also flag TODO/FIXME/HACK/XXX/gambiarra in security-relevant code. file:line.` },
]

const FINDINGS_SCHEMA = {
  type: 'object', additionalProperties: false,
  properties: {
    findings: { type: 'array', items: {
      type: 'object', additionalProperties: false,
      properties: {
        title: { type: 'string' },
        surface: { type: 'string' },
        severity: { type: 'string', enum: ['critical','high','medium','low','info'] },
        cvss_reasoning: { type: 'string', description: 'CVSS 3.1 vector + score reasoning' },
        location: { type: 'string', description: 'file:line (exact)' },
        explanation: { type: 'string', description: 'CHEWED plain-language: what it IS, WHY it matters, HOW exploited — step by step, stakeholder-readable. Minimum a full paragraph.' },
        attack_scenario: { type: 'string', description: 'the attacker path, concretely' },
        reproduction: { type: 'string', description: 'step-by-step PoC / repro outline (exact request/sequence/conditions)' },
        impact: { type: 'string', description: 'concrete blast radius: which tenants/data/money, how bad' },
        evidence: { type: 'string', description: 'exact vulnerable code QUOTED + file:line' },
        recommendation: { type: 'string', description: 'concrete fix — corrected code or precise change' },
        confidence: { type: 'string', enum: ['high','medium','low'] },
      },
      required: ['title','surface','severity','location','explanation','attack_scenario','reproduction','impact','evidence','recommendation','confidence'],
    } },
  },
  required: ['findings'],
}

const VERDICT_SCHEMA = {
  type: 'object', additionalProperties: false,
  properties: {
    is_real: { type: 'boolean' },
    verdict: { type: 'string', enum: ['confirmed','refuted','needs-info'] },
    adjusted_severity: { type: 'string', enum: ['critical','high','medium','low','info','none'] },
    reasoning: { type: 'string' },
    poc_or_disproof: { type: 'string' },
  },
  required: ['is_real','verdict','adjusted_severity','reasoning'],
}

// ─── Phase 1: Assess (16 agents, parallel) ───────────────────────────────────
phase('Assess')
log(`CAA-360 launching ${AGENTS.length} agents across 11 attack surfaces…`)
const raw = await parallel(AGENTS.map(a => () =>
  agent(`${FRAMEWORK}\n\n=== YOUR ASSIGNMENT [${a.id}] ===\n${a.brief}`,
    { label: a.label, phase: 'Assess', model: a.model, schema: FINDINGS_SCHEMA })
    .then(r => ({ agent: a.id, model: a.model, findings: (r && r.findings) || [] }))
))
const all = raw.filter(Boolean).flatMap(r => r.findings.map(f => ({ ...f, foundBy: r.agent })))
log(`Assess complete: ${all.length} raw findings from ${raw.filter(Boolean).length}/${AGENTS.length} agents.`)

// Dedupe by normalized (surface + first 60 chars of title + file part of location)
const seen = new Map()
for (const f of all) {
  const fileKey = String(f.location || '').split(':')[0].toLowerCase().trim()
  const key = `${(f.surface||'').toLowerCase().trim()}|${(f.title||'').toLowerCase().replace(/[^a-z0-9]/g,'').slice(0,60)}|${fileKey}`
  if (!seen.has(key)) seen.set(key, { ...f, alsoFoundBy: [] })
  else seen.get(key).alsoFoundBy.push(f.foundBy)
}
const unique = [...seen.values()]
log(`Deduped: ${all.length} → ${unique.length} unique findings.`)

// ─── Phase 2: Verify (adversarial confirm/refute, parallel) ──────────────────
phase('Verify')
const verified = await parallel(unique.map(f => () => {
  const vModel = (f.severity === 'critical' || f.severity === 'high') ? 'opus' : 'sonnet'
  return agent(
    `${FRAMEWORK}\n\n=== ADVERSARIAL VERIFICATION ===\nYou are a SKEPTIC. A CAA-360 agent reported the
finding below. Your job: try to REFUTE it. Read the cited code yourself. Default to refuted=true
if the evidence does not hold up, the path is unreachable, the input is already validated upstream,
or it is a test-only artifact. Only confirm (is_real=true) if you independently reproduce the exact
vulnerable path in the real code. Adjust severity to reality (downgrade hype).\n\nFINDING:\n` +
    `title: ${f.title}\nsurface: ${f.surface}\nclaimed severity: ${f.severity}\nlocation: ${f.location}\n` +
    `attack: ${f.attack_scenario}\nevidence: ${f.evidence}\nrecommendation: ${f.recommendation}`,
    { label: `verify:${(f.location||'?').split('/').pop()}`, phase: 'Verify', model: vModel, schema: VERDICT_SCHEMA })
    .then(v => ({ finding: f, verdict: v }))
}))
const confirmed = verified.filter(Boolean).filter(v => v.verdict && v.verdict.is_real)
const refuted = verified.filter(Boolean).filter(v => v.verdict && !v.verdict.is_real)
log(`Verify complete: ${confirmed.length} CONFIRMED, ${refuted.length} refuted/false-positive.`)

// ─── Phase 3: Synthesize (1 Opus → prioritized launch-risk report) ───────────
phase('Synthesize')
const confirmedDigest = confirmed.map((v, i) =>
  `[${i+1}] sev=${v.verdict.adjusted_severity} | ${v.finding.title}\n` +
  `    surface: ${v.finding.surface} | where: ${v.finding.location} | cvss: ${v.finding.cvss_reasoning||'n/a'}\n` +
  `    explanation: ${v.finding.explanation}\n` +
  `    attack: ${v.finding.attack_scenario}\n` +
  `    reproduction: ${v.finding.reproduction}\n` +
  `    impact: ${v.finding.impact}\n` +
  `    evidence: ${v.finding.evidence}\n` +
  `    fix: ${v.finding.recommendation}\n` +
  `    verify-reasoning: ${v.verdict.reasoning}${v.verdict.poc_or_disproof ? '\n    verify-poc: '+v.verdict.poc_or_disproof : ''}`
).join('\n\n')

const report = await agent(
  `${FRAMEWORK}\n\n=== SYNTHESIS (FINAL REPORT) ===\nYou are the CAA-360 lead. Below are the
ADVERSARIALLY-CONFIRMED findings (false-positives already filtered). Produce the final launch-risk
report as a complete Markdown document. OWNER MANDATE: every finding must be FULLY DOCUMENTED,
EXPLAINED, and CHEWED — do NOT compress or summarize away the detail; preserve and improve it.

STRUCTURE:
1. ## Executive Summary — is CoreLink launch-safe? top risks in one honest paragraph + a counts table.
2. ## Findings — one '### [SEV] Title' subsection PER finding, ordered Critical→High→Medium→Low.
   For EACH finding write the FULL chewed write-up (multiple paragraphs, not a line):
     - **What it is** (plain language, stakeholder-readable)
     - **Why it matters / Impact** (concrete blast radius)
     - **How it's exploited** (step-by-step attack + reproduction)
     - **Evidence** (the exact file:line + quoted code)
     - **CVSS** (vector + score)
     - **Fix** (concrete — corrected code or precise change)
   Do not drop any field. If the input detail is thin, expand it by reading the cited code yourself.
3. ## Remediation Order — prioritized list (fix-before-launch vs fix-after).
4. ## Surface Coverage — table of the 11 surfaces: clean vs has-findings.
Be precise and honest. If there are NO criticals/highs, say so plainly — do NOT invent risk.

CONFIRMED FINDINGS (${confirmed.length}):\n${confirmedDigest || '(none confirmed)'}\n\n` +
  `Also note: ${refuted.length} reported findings were refuted as false-positives during verification.`,
  { label: 'synthesize:report', phase: 'Synthesize', model: 'opus' })

return {
  raw_findings: all.length,
  unique_findings: unique.length,
  confirmed: confirmed.length,
  refuted: refuted.length,
  by_severity: confirmed.reduce((m,v)=>{const s=v.verdict.adjusted_severity;m[s]=(m[s]||0)+1;return m},{}),
  confirmed_findings: confirmed.map(v => ({ ...v.finding, verdict: v.verdict })),
  refuted_findings: refuted.map(v => ({ title: v.finding.title, location: v.finding.location, reason: v.verdict.reasoning })),
  report,
}
