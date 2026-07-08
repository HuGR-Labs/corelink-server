# CoreLink Knowledge Surface — architecture concept (DRAFT)

> Working name; TBD. The verified, content-addressed, LLM-native knowledge graph delivered as a
> **CoreLink surface** — built on the same CAS+AC genome as the cache, scoped to a Workspace,
> queried deterministically. Status: **concept / pre-build.** Nothing here is implemented yet.

## 1. Thesis (one paragraph)

A codebase's architectural knowledge should be a **content-addressed, verified, composable graph**
that an AI agent queries **deterministically** — not a pile of prose, not fuzzy-similarity RAG. Every
fact is a node addressed by `hash(claim ⊕ grounding@sha ⊕ tier ⊕ test-status)`; the hierarchy is a
Merkle DAG; a query returns the **minimal, fresh, trust-tagged proof-slice** for a token budget. The
LLM lives **only in the write/maintain path** (authoring + drift-reconcile); the **read path has no
LLM** — it is mechanical, cacheable, reproducible. This is the OKF wiki, evolved: from a flat bag of
grounded concepts into a verified composition tree with content-addressed retrieval.

## 2. Why it is a CoreLink surface (the substrate match)

CoreLink already exposes its CAS+AC genome as surfaces (native CAS/AC, Bazel REAPI, Turborepo,
sccache, OCI/npm/pip/brew). The Knowledge Surface is **one more surface on the same two primitives**:

| Knowledge-graph need | CoreLink primitive |
|---|---|
| store concept-nodes content-addressed, deduped, R2-backed | **CAS** (store/get by hash) |
| memoize an agent's reasoning / a proof-slice by hash | **AC** (action/result cache) |
| Merkle rollups + composition manifests | content-addressed manifest objects (same as the CAS catalog) |
| multi-tenant isolation + the public-dedup / private-isolate moat | CoreLink's existing tenancy + `_public` dedup model |

So the substrate (persistence, content-addressing, dedup, memoization, multi-tenancy) is **inherited,
not built**. What is genuinely new lives *above* it (§5–6).

## 3. Workspace scoping (the org/team fit + the compounding moat)

A **Workspace** (per org/team) is the natural **tenant/scope** for the knowledge graph — no new
boundary to invent. Consequences:

- A team's agents + humans query **their workspace's** graph.
- **Public/shared knowledge** (e.g., concepts for a common library) **dedups across workspaces**;
  **private repo knowledge** stays isolated — the cache's network-effect moat, applied to knowledge.
- **Shared reasoning cache (the killer payoff):** the **AC is shared within a Workspace**, so one
  agent's resolved proof-slice / reasoning over a module is cached for the **whole team's** agents.
  More usage → fuller workspace reasoning-cache → cheaper + faster for everyone on the team. The moat
  **compounds per-workspace**.
- Isolation caveat (§7): the public-dedup path must never leak a private concept's content across
  workspaces — same discipline as the `_public` CAS carve-out.

## 4. The genome (inherited from the HuGR family)

This is the same architecture DNA as Keepr (AetherDB), DashR, Datarail — content-addressed,
deterministic, coordination-free, Merkle-verifiable, monotone:

- **Content-addressing** = identity + freshness-proof + dedup + cache key (Keepr `.aseg`, DashR
  `node_id=BLAKE3(sealed)`, Datarail `cofre`).
- **Determinism → memoization/replay** (Keepr CAD, DashR store-less resolution fold).
- **Merkle index** for O(log n) diff + offline reconciliation (Datarail manifest/STH, DashR Merkle-DAG).
- **Multiple indexes over the same nodes** (Keepr INV-ORDINAL-UNITY) → the multi-axis hash hierarchy (§5).

## 5. Data model — the Merkle-OKF

- **Node** = a concept/claim: `node_id = H( canonical(claim) ⊕ grounding(source_files@sha) ⊕ tier ⊕
  test-status )`. Immutable, content-addressed; a new revision is a new node.
- **Composition tree** = parent↔child edges; a parent's `node_id` rolls up its children's hashes
  (Merkle). Contract (mechanical gate): `source_files(parent) ⊆ ∪ source_files(children)` and
  `INVs(parent) ⊆ ∪ INVs(children)` — composition can't silently break.
- **Multi-axis hash hierarchy** (the owner's "hashed index"): the *same* nodes participate in several
  orthogonal Merkle indexes — by **folder**, **topic**, **module/crate**, **primitive** (the code
  construct), **criticality-tier**. Each axis emits its own rollup hashes, independently queryable
  ("has the SECURITY-tier subtree changed?" vs "has the billing MODULE changed?").
- **Tiers:** `T0` (load-bearing: auth, isolation, money, data, crypto — MUST cite a passing test),
  `T1` (important: freshness + LLM-advisory), `T2` (descriptive: freshness only). Tier is assigned
  once at a node and **inherited** by descendants (solves "which of 452 invariants matter").

## 6. Query model — deterministic, no LLM in the read path

The read path is **mechanical, like SQL** — the LLM built the data; queries do not invoke a model.

- **Trust-tags** (100% deterministic): every returned claim carries `[freshness, tier, test-status]`
  (= gate output + frontmatter + CI test report).
- **Proof-tree traversal** (deterministic): "why is X true?" → walk composition + citation + coverage
  edges → return plane→subsystem→invariant→test.
- **Delta-since-commit** (deterministic, O(log n)): compare axis rollup hashes between two SHAs →
  descend only changed subtrees. An agent that cached a subtree by hash re-checks one hash.
- **Budget-aware proof-slice** (deterministic *assembly*): given (entry, token-budget, required-tier),
  greedily assemble the minimal subtree by tier-priority + graph-distance until budget. Honest edge:
  this is a **policy-optimal slice**, not provable "sufficiency" (sufficiency is undecidable).
- Entry point: structural (by id/path/tag/INV) is fully deterministic; an *optional* thin embedding
  layer handles fuzzy NL entry (a single similarity lookup, not generation). Agents mostly navigate
  structurally.
- **Interface:** MCP (the transport — not the moat). The differentiation is the data + query +
  guarantees behind it, not "it's an MCP server."

## 7. Write / maintain path — where the LLM lives (gated, amortized)

- **Authoring:** an LLM reads code → writes/tier-assigns concepts → grounded + checkpointed. Expensive,
  per-repo, occasional. **This is the real cost floor; the substrate is cheap, the truth is O(repo).**
- **Self-heal (already wired today):** on a merge that drifts a cited line, the deterministic reporter
  detects it (0-cost) and the `okf-reconcile` skill re-anchors + re-verifies the claim, gated
  fail-closed (CI `okf-autoreconcile.yml` / local `hooks/post-merge`). The Knowledge Surface is the
  read-optimized projection of this maintained graph.
- **Truth, where it matters:** a `T0` node's claim is true because **a cited test passes** (executable
  invariant) — the gate enforces "every T0 node cites a passing test." Truth is mechanical for the
  load-bearing tier; advisory-LLM for prose; freshness-only for the rest.

## 8. Honest open problems (the "não é fácil")

1. **Canonical form for hashing** — hash the *semantic* tuple (claim+grounding+tier+test), not the
   markdown bytes, or cosmetic edits cause false "changed". Defining the canonical form precisely is
   the load-bearing unsolved problem.
2. **Graph + query engine** — real new engineering above CAS+AC (edges, multi-axis rollups,
   proof-slice assembly).
3. **Content cost** — authoring/maintaining verified knowledge is O(repo), LLM-priced. The substrate
   does not make the truth cheap.
4. **Private/public isolation** — the dedup moat must not leak private concept content across
   workspaces (the `_public` discipline).
5. **Tier inheritance vs straitjacket** — inherited criticality must allow per-node override without
   becoming bureaucracy.

## 9. Non-goals (what this is NOT)

- Not RAG / embeddings (fuzzy similarity is the wrong primitive; this is exact verified addressing).
- Not human documentation (optimized for agent retrieval + token-budget, not prose readability).
- Not a deterministic *truth* oracle (truth is semantic; T0=test is the only mechanical truth, the
  rest is freshness + advisory).
- Not a separate product (decision: a CoreLink **surface**; the moat is the framework + the
  CAS+AC+Workspace substrate, not the docs).

## 10. Build sequence (calm, slice by slice — orchestrator pre-decided)

1. **This doc** — the canonical concept (done = the régua).
2. **Spike (cheapest validation):** one wiki module → nodes as CAS objects (key = canonical hash) +
   a `GET by hash` proving deterministic ctrl-C-turbinado + one Merkle rollup proving O(log n)
   "changed?". Validates the thesis at minimal cost. **No graph engine, no query layer yet.**
3. **Canonical-form spec** — solve open problem #1 before scaling (define the hashed tuple precisely).
4. **Graph + axis rollups** — the multi-axis Merkle indexes over the CAS nodes.
5. **Deterministic query engine** — trust-tags → proof-tree → delta → budget-slice, in that order.
6. **Workspace scoping + MCP surface** — tenant binding + the shared reasoning cache (AC) + MCP verbs.
7. **Tier-test enforcement** — the T0-cites-a-passing-test gate.

Each slice is independently valuable + verifiable; stop and assess after each. Do not skip #3.

---

*Concept, not commitment. The win is the design + the substrate fit; the feature is unbuilt and the
hard parts (§8) are real. Slice #2 is the cheapest thing that proves or kills the thesis.*
