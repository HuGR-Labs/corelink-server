---
id: "AUDIT-2026-04-25-AGENT-R4-S04-PART2"
type: "audit"
doc_status: "DRAFT"
audit_status: "CLOSED"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-05-27"
reviewer: "Agent R4 (Claude Opus 4.7, 1M context, independent reviewer)"
scope: "Lote 10.4 — Sprint S-04 Part 2 (WI-S04-004 .. WI-S04-006)"
sprint_contract: "specs/04_sprints/S04/_spec_contract.md v1.1.0"
calibration_baselines:
  - "specs/_audits/2026-04-25-agent-r4-s03-part1-wi-review.md (S-03 part1, 7.6/10)"
  - "specs/_audits/2026-04-25-agent-r4-s03-part2-wi-review.md (S-03 part2, 7.95/10; WI-S03-007 best-in-class 8.5)"
files_reviewed:
  - "specs/04_sprints/S04/work_items/WI-S04-004-hkdf-digest-signing-adr-0021.md (1007 lines)"
  - "specs/04_sprints/S04/work_items/WI-S04-005-ttl-worker-cron-do-adr-0019.md (859 lines)"
  - "specs/04_sprints/S04/work_items/WI-S04-006-reapi-conformance-prr-ship-gate.md (878 lines)"
cross_references:
  - "WI-S04-001..003 (read for cross-WI consistency)"
  - "_spec_contract.md S-04 v1.1.0"
---

> **CLOSED 2026-05-27** — S-04 sprint implementation sealed via git tag `s04-impl-sealed`; this independent review record is delivered. See `specs/_audits/2026-05-27-audit-triage-post-w36.md` for triage methodology.

# Agent R4 — Lote 10.4 S-04 Part 2 (WIs 004-006) WI Review

> **Reviewer**: Agent R4 (independent SOTA reviewer; ruthless, technical, no diplomacy).
> **Scope**: WI-S04-004 (HKDF sig + ADR-0021), WI-S04-005 (TTL Cron DO + ADR-0019), WI-S04-006 (REAPI conformance + PRR ship gate).
> **Calibration target**: User directive "average não serve. SOTA puro 9-10 é o target." WI-S03-007 sat at 8.5/10 best-in-class; WI-S03-008 (analogous PRR ship gate) at 7.8/10.

---

## Veredito Geral

The S-04 Part 2 trio is the **highest-stakes triple in the program to date**: WI-004 is the only crypto-grade work item the program has shipped (Mann-Whitney 0.5ms gate; key rotation; TDK custody) and WI-006 is the production go/no-go. Compared to the S-03 baseline the package shows clear maturity gains (32-section discipline, 13-row sign-off, ADR ratificação plans, cumulative INV §3.15 promotion, RB-FM-303 dry-run mandatory, 5-region cron sharding, gradual production rollout plan) and the prior R4 P0 deltas have been internalized in places (Mann-Whitney 3-prong with power+Šidák+CI is now stated correctly; 100k iter property test for tenant isolation; cripto-grade tighter |Δmedian|).

But three classes of defects keep the trio short of the 9-10 SOTA bar. **First: load-bearing crypto details that an external Crypto SME will catch on day 1.** The HKDF "info string" actually conflates the HKDF Expand `info` parameter with what should be the canonical-bytes input to the MAC; the spec's `canonical_bytes(envelope)` deliberately omits `result_hash` from the binding, meaning a `result`-tampered envelope with the same `merkle_root` would still verify (the binding is only as strong as `merkle_root` — fine, but **only if Merkle root is collision-resistant** and **only if the verifier always recomputes Merkle root before trusting the signature**, which the spec calls out only as a soft contract); BLAKE3-keyed-hash is justified with a sentence that misstates BLAKE3's structure (it is **not** Merkle-Damgård — it is a Bao/binary tree construction with proper domain separation); HKDF salt=None is "simplified" away when salt is precisely the parameter that adds source-uniqueness when keying material is structured (TDK rotation introduces correlated keys). **Second: operational handoffs that read fine but don't match the rest of the program.** ADR-0019 is treated as already ratificada in WI-S04-005, while the sprint contract §6 still lists "ADR-0019 documented" as a DoD item to be confirmed; the WI-005 tenant_prefix derivation refers to S-01 WI-S01-001 but a quick grep shows the path is `tenant_prefix = HMAC(TDK, tenant_id)[:16]` reused — TTL eviction's R2 DELETE step (a) uses `<tenant_prefix>` per WI-001 layer 4 derivation but WI-005 §6.1 shows R2 DELETE path as `ac-<region>/<tenant_prefix>/<action_digest>.json` while the eviction flow §1 [2]a mixes prose `ac-<region>/<tenant_prefix>/...` — there is no specification of how the cron worker derives `tenant_prefix` from a row that only stores `tenant_id` (it presumably re-runs the HMAC, which means TTL worker also needs TDK access — a fact buried in the trust boundary that the security review will surface). **Third: the PRR ship gate (WI-006) is the most ambitious WI in the sprint and also the most exposed to staffing reality** — same defect class flagged for WI-S03-008. 9-of-13 sign-off rows are still `_TBD_`/`_staffing-blocked_`; "Crypto SME advisory" is repeatedly self-described as both "MANDATORY EMPHATIC" and "advisory waivable per ADR-0034" in the same WI — a contradiction that must resolve before PRR convenes; "100% REAPI conformance" is asserted as non-negotiable while §19 of the sprint contract explicitly lists 95%-with-waiver as an allowed downgrade path with Architect+Crypto SME+ADR; WI-006 forbids the very waiver path the contract permits, without amending the contract.

The trio earns a **Part 2 average of 7.83/10** — slightly below the S-03 part 2 average (7.95) but with materially harder content (live crypto + ship gate). WI-S04-004 lands at **7.7** (strong density, but multiple cripto correctness issues that mandate Crypto SME pre-PRR), WI-S04-005 lands at **8.0** (cleanest of the trio; one P0 around tenant_prefix derivation in eviction path; otherwise the strongest TTL spec the program has produced), WI-S04-006 lands at **7.8** (ambitious and structurally sound but the staffing/contract drift flagged in S-03 part 2 has been ported forward unchanged). All three are **GO-WITH-FIXES** for Lote 10.4bis; none is REJECT; **WI-004 should not SEAL without a real Crypto SME independent review** (not just listed as advisory).

**Aggregate part 2 score: 7.83/10.**

---

## Per-WI Findings

### WI-S04-004 (HKDF-SHA256 digest signing + ADR-0021)

**Score: 7.7/10**

Breakdown:
| Axis | Score | Note |
|---|---|---|
| Rigor cripto | 7.0 | Several load-bearing inaccuracies; constant-time discipline & Mann-Whitney 3-prong are SOTA but the primitive justification is shaky |
| Completeness | 8.5 | 32 sections; 6 properties; 12 chaos; 50+50 test vectors; rotation grace; cargo-fuzz |
| Clarity | 7.5 | Reads well but mixes "what the API does" with "what we hand-wave"; ADR-0021 is the strongest section |
| SOTA-adherence | 8.0 | Mann-Whitney 3-prong cripto-grade |Δmedian| ≤ 0.5ms; Crypto SME mandatory emphatic; Zeroizing TDK |
| Internal consistency | 7.5 | `info="ac-sig"` semantics confused with "what gets signed"; salt rationale weak |
| Prior-WI consistency | 7.5 | `canonical_bytes` schema diverges from `AcEnvelope` (no `result_hash` binding); `sig_alg=HkdfSha256` declared in WI-003 but verify path doesn't switch on it |
| Customer-facing readiness | 8.0 | Personas + SLA addendum honest about latency; rotation runbook implied not detailed |

**Strengths:**
- Mann-Whitney 3-prong **cripto-grade** with `|Δmedian| ≤ 0.5ms` is the tightest constant-time gate the program has set; correctly distinguished from middleware-grade 5ms (prior R4 reviews flagged the missing tightness — fixed here).
- ADR-0021 ratificação table (HKDF vs Ed25519) is concrete and quantitative (sig size, latency, key mgmt, storage saving 320 MB at 10M envelopes); risks accepted are explicit.
- TDK Zeroizing wrap, in-memory cache 5min TTL with rotation-event invalidation, in-band rotation grace via `accepted_key_ids: Vec<u32>` — all correct primitives.
- Length check pre-compare correctly noted as fast-fail (length is public; constant-time post-length).
- Cargo-fuzz harness 1h CI nightly, clippy custom lint forbidding `==` in sig module, CI byte-equal assert on `b"ac-sig"`, 50+50 test vectors — full belt-and-suspenders.
- Crypto SME mandatory emphatic at multiple checkpoints + Architect mandatory; sign-off table has the strongest emphasis annotation in the program.

**P0 — must fix before Lote 10.4bis SEAL:**

1. **Canonical bytes binding does not include `result_hash`.** §1 + §6.1 §6.1.5 specify:
    ```
    canonical_bytes = version || tenant_id || action_digest || merkle_root || created_at_ms  (89 bytes)
    ```
    `result_hash` is **not** in the binding. The implicit assumption is "the Merkle root commits to the entire result, so signing merkle_root is signing the result by transitivity." That is **only true if the verifier always recomputes Merkle root from the actual `AcEnvelope.result` before trusting `envelope.merkle_root`**. WI-S04-003 §1 says `verify_full(envelope, sig_verifier)` does both (structure + sig delegate), but the order is not pinned: if a buggy handler in WI-S04-001 step [4] calls `sig::verify(envelope, tenant_key)` BEFORE `verify_structure(envelope)`, then an attacker who flips bytes in `result` (without changing `merkle_root`) gets a valid signature on the lie. Mitigation: either (a) include `BLAKE3(serialized_result)` in `canonical_bytes` (extends layout to 121 bytes), OR (b) make `verify_full` API force `verify_structure` first and **make the handler API impossible to misuse** (SignatureVerifier::verify_sig private; only `verify_full` public). Currently the spec exports both `verify_sig` (sig only) and `verify_structure` (Merkle only) as independent traits — exactly the API misuse the chaos test #1 of WI-S04-003 was meant to prevent. **Decide and document explicitly.**

2. **HKDF salt=None rationale is weak; salt is exactly the right place to inject `sig_key_id`.** §9.3 says "HKDF-Extract step takes optional salt; salt-less mode uses zero-bytes; per-tenant separation already via TDK." Correct that salt-less is RFC 5869 compliant. **But:** TDK rotation produces `TDK_v1, TDK_v2, ...` — these are **not unrelated keys** (they're produced by the same KMS/CF Secrets pipeline, possibly with correlated entropy depending on the KMS). HKDF-Extract with `salt = sig_key_id_LE32_bytes` removes the "correlated keying material" risk at zero cost (5 bytes of constant overhead). Industry SOTA (TLS 1.3, Signal Protocol) **always** binds key versioning into the HKDF salt or info string. Recommendation: `salt = sig_key_id.to_le_bytes()` (4 bytes; salt-less compat broken on rotation, which is the point); OR `info = b"ac-sig-v" || sig_key_id_le_bytes` (if you prefer info-based domain separation). **Either way, the current `salt=None + info=b"ac-sig" constant + key_id NOT bound into HKDF` lets a rotation event with a correlated TDK pair produce identical sig_key for the same canonical_bytes** — bad. Fix the binding.

3. **BLAKE3-keyed-hash justification mischaracterizes BLAKE3 internals.** §9.2: "BLAKE3-keyed mode is BLAKE3 with 32-byte key prefix; native; cryptographically sound (Merkle-Damgård but with strong domain separation)." **BLAKE3 is not Merkle-Damgård.** It is a Bao tree (binary Merkle tree of 1024-byte chunks) with explicit domain separation flags. The MD claim is incorrect; if a Crypto SME reads this they will lose confidence in the entire WI. Fix: "BLAKE3-keyed mode uses BLAKE3 with the 32-byte key as the IV via the keyed_hash flag; the construction is a binary Merkle tree of 1024-byte chunks (Bao); collision-resistance ~2^128 against MAC forgery; designed-in domain separation prevents length-extension." Also: BLAKE3-keyed_hash is a **PRF**, not just a "MAC primitive" — the strength claim should reference its NIST-compatible analysis (BLAKE3 paper §6) rather than wave at "collision-resistant 2^128" which is the wrong adversarial model for a MAC (you want PRF-security against forgery, ~2^128 with key length 256 bits, not collision-resistance which is a hash function property).

4. **HMAC-SHA256 dismissed too quickly.** §9.2: "BLAKE3 4× faster (SIMD); critical for verify p99 ≤ 1ms. ... HMAC-SHA256 alternative also acceptable; chose BLAKE3 for performance + consistency with Merkle (WI-003 BLAKE3)." For an 89-byte input the BLAKE3 SIMD advantage is **negligible** — both will run in single-digit microseconds. The justification "critical for verify p99 ≤ 1ms" is therefore false; the budget is dominated by TDK fetch (cached at 1 µs) not the MAC compute. The real argument is "consistency with Merkle WI-S04-003" which is fine, but say so explicitly. The current framing reads like cargo-cult performance optimization. Also: HMAC-SHA256 is FIPS 140-3 approved; BLAKE3 is **not** (only SHA-3-based KMAC is FIPS-approved per NIST SP 800-185 — which the sprint contract §17 *explicitly references*). For SLSA L3 / SOC 2 / FedRAMP customers this matters. Either: (a) explicitly accept "we are not FIPS-compliant for the MAC layer; SLSA L3 alignment is via Merkle + audit chain" in ADR-0021, or (b) reconsider HMAC-SHA256. The current spec is silent.

5. **Cross-WI inconsistency: `sig_alg` enum exists but verifier ignores it.** WI-S04-003 §1 declares `pub sig_alg: SigAlg, // = HkdfSha256 in v1`. WI-S04-004 §1 `verify_sig` signature is `verify_sig(canonical_bytes, sig, sig_key_id)` — **no sig_alg parameter**. If a future v2 introduces a different `SigAlg`, the verifier would happily try to compute BLAKE3-keyed-hash on a sig that was actually Ed25519, length check passes (32 ≠ 64 → LengthMismatch — actually that catches Ed25519, OK) but a hypothetical `SigAlg::Hmac384` or `SigAlg::HkdfSha512` (32 bytes both) would silently mis-verify. Either: (a) verifier accepts `sig_alg` and dispatches; (b) `SigAlg` enum is removed in v1; (c) the verify_sig API is widened. Currently `sig_alg` is declarative metadata that does nothing. This is a footgun for forward-compat (v2 migration breaks).

6. **Replay attack analysis is incomplete.** §1 narrative point 7 + Gherkin scenario "Replay attack — same envelope sig OK; cross-envelope FAIL" describes only the **trivial case** of re-using sig of E2 on E1 with different `action_digest`. Missing: **idempotent UPDATE replay across `created_at_ms`**. Two envelopes with same `(version, tenant_id, action_digest, merkle_root)` but different `created_at_ms` produce different `canonical_bytes` and hence different sigs. So far so good. But: an attacker who captures sig_v1 of envelope_at_T1 and replays it as envelope_at_T2 — the verifier computes `canonical_bytes(envelope_with_T2)` and `canonical_bytes ≠ T1's canonical_bytes`, so verify fails. Good. **However**: the sig itself is not bound to wall-clock freshness. There's no nonce, no replay window. If an attacker captures any one valid sig and replays the *exact same envelope* later (same created_at_ms), it verifies — which the spec calls "OK by design (idempotent replay)". Confirm this is acceptable in the threat model. For AC the answer is yes (cache entries are content-addressable; replaying is a no-op). For audit chain S-09 forward this would be a vulnerability; cross-reference and document the difference.

**P1 — fix this sprint, before PRR:**

7. **TDK in-memory cache 5min TTL: rotation invalidation timing race.** §6.1.8 says "rotation event invalidates cache." How? CF Workers don't have cross-isolate IPC; each Worker isolate has its own cache. Rotation event has to be either (a) a global broadcast (not natively supported), (b) a TTL-based eventual consistency where the 5min TTL is the actual rotation propagation latency, or (c) a per-request "is the cached TDK stale?" check (defeats the cache). The spec says (a) is the design but doesn't specify the mechanism. The chaos test #5 ("Key rotation race: simulate rotation event T+0; cache invalidation lag T+5s") **assumes** lag bounded at 5s but this is hand-waved. Realistic mechanism: rotation event triggers a Worker `cron` re-deploy OR a CF Durable Object broadcast OR a KV-poll-on-each-cold-isolate strategy. Pick one and document.

8. **`accepted_key_ids` rotation grace: 1 prev = 2-year coverage claim is wrong.** §9.8 says "1-prev grace = 2-year coverage; sufficient for AC entries which expire ≤ 365d." The math: rotation annual + AC TTL ≤ 365d. If rotation happens at T=0 and AC entry was signed at T=−360d (just before rotation) it has expires_at ≤ T=+5d max. With 1-prev grace, after T=0 verifier accepts both; at T=+5d the entry expires anyway. **Coverage = max AC TTL, not "2 years."** "2-year coverage" reads like the spec is claiming sigs from 2 years ago verify, which is false. Re-derive: "1-prev grace covers entries signed within (rotation_period + max_TTL) = 365d + 365d = 730d window from rotation event, but only the most-recent-but-pre-rotation entries are in that window." Or simpler: "1-prev grace ensures no envelope is rejected during the rotation cutover." Current claim is misleading.

9. **`prop_constant_time_compare` property test is a Mann-Whitney test (not a property test).** §6.1.9 lists it as a property. Property tests use `proptest`/`quickcheck` over input domains and check invariants on outputs; constant-time is a **statistical property over runtime**, not a per-input invariant. This is a category error: `prop_constant_time_compare` should live in `tests/timing_sig.rs` (Mann-Whitney) not `tests/prop_sig.rs` (property). The spec already creates `tests/timing_sig.rs` separately (§13 artifacts table). Drop from property list, count drops to 5.

10. **Cargo-fuzz harness only fuzzes `verify_sig` input.** §6.1.12: "arbitrary sig + canonical_bytes input." Missing fuzz target: `tdk_handle.fetch(arbitrary_key_id)` panics on overflow; canonical_bytes serialization with `created_at_ms = u64::MAX`; HKDF expand with arbitrary info length (RFC 5869 limit: info ≤ 255 × hash_len = 8160 bytes; the constant is 6 bytes `b"ac-sig"`, fine, but a future ADR change without re-validation could exceed). Add 2 more fuzz targets (sign + tdk_handle).

11. **Test vector Annex C/D coverage gap.** 50 known sigs is good; 50 invalid sigs covering "each error variant" is weak — there are 5 variants (LengthMismatch/Invalid/KeyIdUnknown/BackendError/TdkDerivationFailed), so 10 vectors per variant. But many of these aren't deterministic (BackendError depends on KMS state). Reframe: 30 LengthMismatch (varying lengths 0,1,16,31,33,64,128,...) + 30 Invalid (1-byte flips at varying offsets) + 5 KeyIdUnknown + 5 boundary cases. Total 70+ ; current 50 is undersized for the variant space. Also missing: vectors for canonical_bytes byte-stability (same envelope, different field order in serialization, asserts identical canonical_bytes).

12. **Cost analysis $4k/yr at 10M/dia is inconsistent with the rest of the program.** §22: per-sign $0.000001 at 1M sign/dia + per-verify $0.000001 at 10M verify/dia = $11/dia × 365 = $4015/yr. But per-op CPU cost on CF Workers is usually billed per 50ms CPU-time slot at $0.50/M slots; 1ms verify ÷ 50ms slot = 0.02 slots, but Workers Bundled charges **per request** at $0.30/M (after free tier), not per CPU-ms. The $4k/yr breakdown doesn't show whether these are CF Workers Paid plan (Bundled) or Unbound CPU pricing. Also: if KMS fetch cold ≤ 10ms p99 occurs more than the assumed 5% of requests (cache hit ratio < 95%), the cost balloons. Provide sensitivity analysis.

13. **Side-channel section is hand-waved.** §1 narrative point 6 / atacante scenario "Side-channel via memory access patterns: BLAKE3 has memory access patterns; AVX-512 vectorized; not constant-cache-time strictly. For S-04 GA: acceptable (CF Workers shared infra; isolation imperfect anyway); ADR-0021 documents." This is **not in ADR-0021** as currently written (§1 Risks accepted only mentions HKDF compromise + symmetric trust model). If the spec promises ADR-0021 documents the side-channel acceptance, ADR-0021 must say so. Either: (a) add explicit "BLAKE3 SIMD memory access pattern is a known leakage channel; CF Workers shared infrastructure precludes constant-cache-time guarantees; mitigated by per-tenant TDK isolation + 256-bit MAC strength; future migration path post-quantum migration via S-XX ADR" to ADR-0021, or (b) drop the claim. **Crypto SME will read this and want to see the actual ADR text.**

14. **Sign-off table: "Crypto SME MANDATORY EMPHATIC" but waiver path implied.** §30 row 13 says `**MANDATORY EMPHATIC**`; §16 PRR section says "Architect + **Crypto SME mandatory emphatic**". WI-S04-006 §6.1.6 then says "Crypto SME (advisory) ... advisory permits ship if Crypto SME unavailable on ship date but post-ship review committed" + Gherkin "Crypto SME advisory waiver (acceptable per ADR-0034)." This is a **direct contradiction across WIs**. If WI-004's Crypto SME review is MANDATORY EMPHATIC, then WI-006's advisory waiver path is incompatible. Pick one consistently. Recommendation: keep WI-004 MANDATORY (this is the cripto WI; SME review is non-waivable for the spec/code), keep WI-006 advisory only for the PRR ceremony (SME signs off on the WI-004 implementation pre-PRR; PRR is about the integration/ship-readiness, not the cripto). Document this distinction explicitly in both WIs.

**P2 — next sprint or doc-only:**

15. §9.5 "subtle::ConstantTimeEq is industry-standard; audited by Rust crypto WG" — minor: `subtle` is maintained by Isis Lovecruft / dalek-cryptography, audited as part of curve25519-dalek but not formally standalone; the audit chain is informal. Just say "actively maintained, ubiquitous in Rust cripto stack."

16. §1 narrative point 9 "HKDF salt unspecified vs random ... per-tenant separation via TDK already" — circular justification (already addressed in P0 #2).

17. Risk register R-014 "TDK exfiltration via KMS audit gap" — Probability=L is optimistic given that KMS audit completeness is itself a separate concern (S-XX); should be M with detection difficulty H.

18. Sub-task ST-018 "Crypto SME review iteration (independent verification): 4h" — **4h is wildly insufficient for an external Crypto SME to do an independent verification of HKDF + BLAKE3-keyed + constant-time + key rotation + ADR-0021.** Industry-standard cripto review of a new sig protocol is 1-2 weeks (40-80h). Either book that time or call this "Crypto SME consultation, not full independent review."

19. §10.s04.004.5 "TDK warm cache hit ratio ≥ 95%" — this assumes traffic mix. Quantify: at what total request rate? Cold isolate spawn rate? Define operational baseline.

20. §15 chaos #6 "TDK exfil attempt via memory inspection: Zeroizing wrap test; post-drop, inspect VM memory; assert TDK bytes zeroed." — In CF Workers, Rust memory is in WASM linear memory; Zeroize zeros the bytes within WASM linear memory but the host-side memory page is not directly inspectable from within the Worker. The chaos test as written is unrunnable in CF Workers production environment; only runnable in dev/`wasmtime` host. Document the test environment.

---

### WI-S04-005 (TTL Worker Cron DO + ADR-0019)

**Score: 8.0/10**

Breakdown:
| Axis | Score | Note |
|---|---|---|
| Rigor cripto | 8.5 | Tenant-scoped DELETE strict, R2-then-D1 ordering correct, idempotent retry semantics |
| Completeness | 8.5 | 32 sections; 6 properties; 12 chaos; 8 metrics; 2 runbooks; ADR-0019 boundary explicit |
| Clarity | 8.5 | Best-written WI of the trio; reads like operations engineering |
| SOTA-adherence | 7.5 | Property tests only 10k/100k (not cripto-grade like WI-004); adequate for non-cripto path |
| Internal consistency | 7.5 | tenant_prefix derivation in cron not specified; D1 batch atomic claim fragile |
| Prior-WI consistency | 7.5 | Refers to S-01 path conventions but doesn't show how cron derives tenant_prefix without TDK access |
| Customer-facing readiness | 8.5 | Personas + SLA addendum + customer dashboard alignment honest |

**Strengths:**
- Tenant-scoped DELETE is **strictly mandated** with multiple defenses: SQL `WHERE tenant_id = ? AND action_digest = ?`, integration test, property test `prop_ttl_tenant_isolation`, and chaos test #2 introducing the cross-tenant SQL bug as a regression case. This is the cleanest cross-tenant prevention pattern in the program.
- R2-then-D1 ordering is explicitly justified (§9.3) with the orphan-cost trade-off (orphan R2 < orphan D1-ref); chaos test #3 simulates R2 outage and asserts the D1-preserved invariant.
- Refresh-on-hit threshold 60s is **the correct primitive** for D1 lock contention reduction; spec quantifies the 100× reduction; chaos test #5 validates.
- ADR-0019 boundary handoff S-04 → S-07 is the cleanest forward-handoff in the program: explicit `TierTtlResolver` trait with two impls (env-config now, S-07 config-singleton later); chaos test #11 simulates the impl swap.
- Per-region cron sharding (5 regions, 5 DOs) eliminates the single-cron-coordination problem at correct design time; multi-region isolation property test green by construction.
- Bounded batch (1000 rows) with sleep 100ms between, alarm budget 30s — operationally tight; aligns with D1 statement-batch realities and CF DO alarm constraints.
- Audit emission via outbox (atomic with eviction); INSERT batch reuse from WI-S01-005 — pattern consistency.

**P0 — must fix before Lote 10.4bis SEAL:**

1. **`tenant_prefix` derivation in cron worker is unspecified.** §1 [2]a: `R2 DELETE ac-<region>/<tenant_prefix>/<action_digest>.json`. The cron worker has rows from D1 `SELECT tenant_id, action_digest, region`; it does NOT have `tenant_prefix` materialized. It must compute `tenant_prefix = HMAC(TDK, tenant_id)[:16]` per WI-S04-001 layer 4 derivation. **Implication: TTL cron DO needs TDK access** (KMS / CF Secrets binding). This is a **trust boundary expansion** that the security review will catch and the spec doesn't acknowledge. Three fixes: (a) materialize `tenant_prefix` as a column in `ac_meta` (storage cost: 16 bytes × N rows; pre-computed at INSERT); (b) document TDK binding for cron DO + audit access path + Zeroizing wrap reuse from WI-S04-004; (c) drop tenant_prefix from R2 path and use `tenant_id` directly (defeats Layer 4 of the 5-layer defense). Recommendation: (a) — column is the cleanest because it doesn't expand the cron's trust boundary. **The current spec is silent and would ship with cron DO either crashing on missing TDK access OR silently expanding the secret blast radius.**

2. **R2-first then D1-DELETE consistency claim is partially wrong.** §9.3: "R2 DELETE then D1 DELETE: if R2 fails, D1 preserved; row still references R2 (orphan recoverable via re-insert)." **R2 is content-addressable AC envelope storage** — once R2 envelope is deleted, the D1 row references a non-existent envelope; subsequent GET path returns 200 with cached metadata that points to dead R2 → 404 from R2 layer → handler must surface 410 or 404. The spec's framing "orphan recoverable via re-insert" suggests the D1 row could re-create the R2 envelope, but the cron worker doesn't have the original envelope content (it was deleted). The "compensating action" §6.1.7 note "re-INSERT D1 row (idempotent via ON CONFLICT)" suggests undoing the D1 DELETE — but the actual sequence already had R2 DELETE first, then attempted D1 DELETE failed, so D1 row is still there (this case is fine). The confusing thing is §9.3 mixing two scenarios. Rewrite: "R2 DELETE → on success, D1 DELETE → on success, KV invalidate. If R2 DELETE fails, abort batch row; preserve D1 (still references valid R2). If D1 DELETE fails after R2 success, R2 envelope is gone; D1 row references dead R2; next GET returns 404 (404 from R2-not-found path is OK customer-visible); next cron tick re-attempts D1 DELETE (idempotent: expires_at < now still selects row). Orphan window = 1 cron interval." Currently §9.3 is muddled.

3. **Audit emission atomicity claim is unverified.** §9.8: "D1 batch (ac_meta DELETE + audit_outbox INSERT) atomic; both succeed or both rollback." D1 batch transactions support this for **statements within the same `Database.batch()` call** (CF D1 supports batch APIs). But the WI-005 flow (§1 [2]) is: R2 DELETE (external) → D1 DELETE (external) → KV DELETE (external) → audit outbox INSERT (D1). Only the last two are D1 ops; if R2 fails after D1 batch commits, audit log says "evicted" but R2 is intact. The atomicity claim is between the D1 DELETE and audit outbox INSERT *within the same batch*, which is fine, but the framing makes it sound like the whole eviction is atomic. Either: (a) clarify "D1 DELETE and audit outbox INSERT are atomic within D1 batch; R2 DELETE precedes the batch" — and document the failure mode (R2 succeeds but D1 batch fails → orphan R2; R2 fails before D1 batch → no DELETE attempted; D1 batch fails → row preserved + no audit). (b) Move audit outbox INSERT to its own row-level transaction post-D1 commit, accept eventual consistency on audit (within bounded retry). The chaos experiment #10 ("Audit outbox D1 batch fail: simulate audit_outbox INSERT fail mid-batch; verify atomic rollback") asserts (a) but contradicts the §1 flow which puts R2 DELETE before D1.

4. **D1 batch size limit conflict.** Cloudflare D1 batch transactions have a **100KB maximum size** per batch (cited by R4 in S-03 part 2 review for audit chain). 1000-row eviction with each row generating (D1 DELETE + audit_outbox INSERT) at ~200 bytes/audit-event = 200KB+ — exceeds D1 batch limit. The spec assumes D1 can atomically process 1000-row + audit batches; it can't without chunking. Either: (a) reduce batch size to 250 rows (200KB / 800 bytes per row-pair); (b) chunk audit emissions into separate sub-batches (loses atomicity per row); (c) document the actual chunking strategy. Currently the spec is silent. **Cross-WI consistency check**: WI-S03-007 (audit) flagged the same D1 100KB limit issue — this is a known constraint that WI-S04-005 hasn't internalized.

**P1 — fix this sprint, before PRR:**

5. **Cron alarm re-arm reliability.** §6.1.4 says "alarm re-arm at end of each tick." If the alarm handler **panics** (Rust unwind, OOM, deadline exceeded), the re-arm code never runs and the cron silently dies. CF Durable Object alarm semantics: if the alarm handler returns Err or panics, the alarm is automatically retried (per CF docs), but the retry is the same alarm — it doesn't re-arm a future one. The spec's chaos #1 asserts the metric alert fires within 1h ("rate < 1/h sustained"), which is a 60-min detection lag for a critical infrastructure failure. Mitigation propose: (a) re-arm at the START of each tick (before doing any work), so the next alarm is set even if this tick panics; (b) external watchdog cron (CF Cron Trigger global, every 1h) that checks each region's last_tick metric and forces a manual alarm if stale. Currently single point of failure. P1 because the failure mode is recoverable in 1h, but on a HIGH_RISK WI 1h is too long.

6. **Refresh-on-hit threshold 60s creates audit emission gap.** Each refresh-on-hit extends `expires_at`; if threshold = 60s, then 1 emission per minute per hot digest. For a 10k req/s workload on a single hot digest, that's 1 audit event/min — fine. But: refresh-on-hit is *not in the §6.1.10 metrics or audit emit list*. Cross-WI check: WI-S04-001 step [6] (refresh-on-hit) does not emit audit events; only eviction does. From a forensic standpoint, refresh-on-hit IS a TTL extension (a write op); for compliance you may need to log it. Decide and document whether refresh-on-hit emits audit (recommend yes, sampled if necessary).

7. **Bounded batch claim "well below D1 ~100-statement-batch limit per op" is incorrect.** §9.2: "batch size 1000; D1 SELECT 1000 LIMIT is OK fetch-only (no batch-statement limit)." SELECT fetch is fine. But the eviction loop does 1000× (D1 DELETE + audit INSERT) = 2000 statements. D1 has a per-batch statement limit + 100KB byte limit — both are independent. SELECT 1000 with `LIMIT 1000` is one statement so fine; the 1000 DELETEs are 1000 statements. If executed serially (not as a batch), each round-trip ~10ms × 1000 = 10s — bumps against alarm budget 30s. If batched, hits D1 limits. Document the actual execution strategy (per-row commit serial vs chunked batch).

8. **Multi-region "isolation by design" misses cross-region replication.** §9.5: "Per-region scoped SELECT; cross-region pollution impossible." But D1 in CF is a single global database (no native multi-region replication for D1 yet; sessions are routed via locality hints). If region=sam runs SELECT WHERE region='sam', that filters rows correctly — but **all 5 regions' workers query the same D1 database**. If two regions trigger alarms within milliseconds (5/h × 5 regions = 5/h total cron load on D1), 5 concurrent SELECT 1000 + DELETE 1000 + audit INSERT 1000 = D1 lock contention spike. Currently the §6.1.4 says "1 instance per region" but the bottleneck is the shared D1, not the cron instances. Recommendation: stagger alarms via region-specific offset (e.g., sam=00, iad=12, lhr=24, nrt=36, syd=48 minutes past the hour) to spread D1 load; OR make the alarm interval tunable per-region with deliberate staggering.

9. **Per-eviction cost $0.000003 doesn't sum.** §22: R2 DELETE $4.5/M + D1 DELETE $1/M + KV DELETE $5/M + audit outbox INSERT $0.50/M = **$11/M = $0.000011 per eviction**. Spec then says "$0.000003 amortized over batch." The amortization is over batch overhead, not per-op cost — per-op is $0.000011. The $0.000003 number is wrong unless explained. Re-derive.

10. **Refresh threshold drift not auto-detected.** §6.1.10 metric `refresh_skipped_total` is good; missing: alert if `refresh_skipped_total / refresh_on_hit_total > 99%` — that means threshold too high or workload pattern changed; D1 lock contention isn't a problem so threshold could be lowered for hit-rate accuracy. Currently no such alert.

**P2 — next sprint or doc-only:**

11. ADR-0019 already ratificada per spec contract; WI text says "this WI implements per ADR" — confirm ratification version matches.

12. §14.5.10 cost gate per-eviction ≤ $0.000003 is incompatible with the actual $0.000011 from §22 — pick one and reconcile.

13. §6.1.13 chaos suite "12 scenarios"; only 12 listed; OK.

14. §6.1.4 default cron interval 3600s + env override `CORELINK_AC_TTL_CRON_INTERVAL_S`; chaos #6 sets it to 10s and asserts R2 rate limit holds — but no minimum bound enforcement in DO init code is described. Add validation: "DO init asserts `interval ≥ 60s`; rejects below."

15. R-007 "Multi-region race" Probability=L Impact=LOW — given P0 #8 above, raise to M / MEDIUM.

16. §17 sub-task estimates total 41h; WI-006 estimate also 41h — both look optimistic compared to S-03 baselines (50-80h for similar HIGH_RISK WIs).

17. Sign-off row 13 "DBA (advisory)" — DBA in S-03 was `_TBD_`/staffing-blocked; same situation here. No mention of compensating control if DBA absent.

---

### WI-S04-006 (REAPI conformance + PRR ship gate)

**Score: 7.8/10**

Breakdown:
| Axis | Score | Note |
|---|---|---|
| Rigor cripto | 8.0 | Inherits WI-004's cripto via cumulative INVs §3.15 + property test 100k tenant isolation + 72h SLO |
| Completeness | 9.0 | 32 sections; 13 sub-gates; 8 dashboard panels; 13 review checkpoints D+0..D+20 production rollout |
| Clarity | 7.5 | Reads as a checklist + ceremony; some sub-gates handwave detail (e.g., 72h SLO incident reset criteria) |
| SOTA-adherence | 8.0 | 100% conformance, 100k iter, 72h SLO, 10%→50%→100% gradual; gradual rollout is SOTA-positive |
| Internal consistency | 6.5 | Contradiction with sprint contract §19 (95% conformance waiver); Crypto SME mandatory vs advisory contradiction with WI-004 |
| Prior-WI consistency | 7.5 | 19 INVs §3.15 promotion plan complete and traceable; staffing realism still unresolved |
| Customer-facing readiness | 8.5 | SLA addendum + release notes + Bazel onboarding doc explicit; production rollout plan with rollback procedure |

**Strengths:**
- 19 INV §3.15 promotion list is the most complete cumulative invariant audit the program has produced; each WI source traced; aligns cleanly with TLA+ alignment line.
- Production rollout plan **gradual 10% → 50% → 100%** with 24h monitoring intervals between is operationally mature; rollback procedure to 0% via Wrangler version revert documented; RTO ≤ 10 min.
- 13 review checkpoints D+0..D+20 are concrete, time-bound, and include post-ship review (D+20) — most ship gates skip the post-ship retrospective.
- DASH-AC dashboards 8 panels, alerts to PagerDuty + Slack, chaos-tested calibration (chaos #5).
- RB-FM-303 dry-run is **mandatory** with detection ≤ 5min / remediation ≤ 30min / customer comm ≤ 1h targets — concrete operational expectations.
- Property test 100k tenant isolation as gate (zero violations tolerated) — inherits cripto-grade rigor from WI-004 pattern.
- Customer-facing communication (SLA addendum + release notes + Bazel onboarding doc) is **pre-ship**, not post-ship — this is correctly noted as the customer-trust-preserving choice.

**P0 — must fix before Lote 10.4bis SEAL:**

1. **Sprint contract drift: 100% conformance non-negotiable vs §19 95% waiver allowed.** Sprint contract `_spec_contract.md` §19 explicitly lists:
    > Itens waivable com Architect + Crypto SME + ADR:
    > ⚠️ Conformance suite 100% → 95% com explicit waiver list per check.

    WI-S04-006 §1 + §6.1.1 + §10.s04.006.1 all assert "100% pass; <100% blocks ship; quarterly bumps via ADR." §7 anti-scope: "❌ Ship without 100% conformance." This **directly contradicts** sprint contract §19 which permits 95% with waiver. Either: (a) amend sprint contract §19 to remove the waiver path (recommended for HIGH_RISK GA); (b) amend WI-006 to permit the 95% path with the documented waiver mechanism; (c) note explicitly "WI-006 raises the bar from sprint contract §19's 95% waiver path to 100% non-negotiable as a conscious tightening; sprint contract §19 should be deprecated for S-04 GA." **This is exactly the kind of WI-vs-contract drift R4 flagged in S-03 part 2 (10k iter vs 100k iter); the program has not internalized the lesson.** Same defect class.

2. **Crypto SME "mandatory emphatic" vs "advisory waivable" contradiction across WIs.** Already flagged in WI-004 P0 #14. WI-006 §6.1.6 + §30 row 13 says Crypto SME is "advisory" and §6.1.6 + Gherkin "Crypto SME advisory waiver (acceptable per ADR-0034)" + "ship proceeds with 12 mandatory complete." But WI-004 §30 row 13 says **MANDATORY EMPHATIC** and PRR §16 says "Architect + Crypto SME mandatory emphatic." **Pick one consistently across both WIs.** Recommendation: Crypto SME independent review of WI-004 implementation is non-waivable (it's the cripto WI; SME signs off pre-PRR); the PRR ceremony in WI-006 may proceed with 12 mandatory + Crypto SME's WI-004 sign-off serving as the cripto domain validation. Either way, the current text contradicts itself.

3. **PRR sign-off staffing reality is exactly the same gap flagged in S-03 part 2; carried forward unfixed.** §30: 9 of 13 rows are `_TBD_`/`_staffing-blocked_`. Owner+Final Approver+Product = 3 (Gustavo). SRE Lead = staffing-blocked (ADR-0034 waiver). Security Lead, Engineer×2, QA, Compliance, Privacy, Architect, AppSec, Crypto SME = 8 unfilled. R-009 "Crypto SME unavailable on ship date" mitigation says "2-week advance booking; advisory waiver path ADR-0034" — paper mitigation; no contractor pool documented; no firm names. S-03 part 2 review §"PRR sign-off staffing — Tier-1 reviewer pipeline" identified this as a structural blocker; same issue lives here. **This is not a defect of WI-006 alone; it is a sprint-level staffing risk that gates the ship gate.** Either: (a) explicit sign-off retention plan with names/firms by D-0 sprint kickoff, (b) reduce mandatory sign-off count via sprint contract amendment, (c) accept staffing-blocked SEAL slip. Pick one and write to `_spec_contract.md`. This issue should NOT ship into Lote 10.4bis without resolution.

4. **72h SLO incident reset criteria are underspecified.** §1 + §6.1.7: "Pause clock on P1/P2 incidents; resume after fix; require continuous 72h pre-ship." Gherkin "P3 minor incidents acceptable; do not reset clock." But: who classifies P1 vs P2 vs P3? What IS a P3 in this context (the program doesn't have a defined incident classification policy that I can find in the materials)? What's the threshold between "P3 metric drift" (no reset) and "P2 metric drift sustained" (reset)? Currently a single-engineer Owner can self-classify any incident as P3 to keep the clock running — defeats the gate. Mitigation: (a) define P0/P1/P2/P3 explicitly in `_spec_contract.md` or a referenced runbook; (b) require Architect + Security Lead joint classification; (c) automatic reset on any DASH-AC alert firing for > 5min sustained. Currently subjective.

5. **REAPI conformance "100% pass" is asserted but the test set is unspecified.** §6.1.1: "bazelbuild/remote-apis test suite pinned commit (vendored em WI-S04-001 build.rs)." But: which test set within bazelbuild/remote-apis covers AC operations? The bazelbuild/remote-apis repo has multiple test directories (`tests/`, `examples/`, conformance harnesses); the AC subset has to be enumerated. Currently "100% AC ops" is a moving target — it could be 12 tests or 200 tests depending on commit. Pin the test set list (e.g., "the 47 tests under `bazelbuild/remote-apis/conformance/action_cache/` at commit X") in ADR-0036 (referenced §6.1.10 / §11). Without this, the gate is unverifiable.

**P1 — fix this sprint, before PRR:**

6. **Production rollout 10% → 50% → 100% with 24h monitor lacks SLO-budget criteria for advancement.** §6.1.10 + checkpoints D+9..D+13 list the % advancement on calendar but not on telemetry. What signal advances 10% → 50%? "No P1/P2 incidents during 24h" — same subjective bar as the 72h SLO reset. Add: "advance only if (a) DASH-AC error budget < 50% consumed, (b) cache hit ratio ≥ 70% for the % under load, (c) zero cross-tenant alerts, (d) cost per-op within +10% of baseline." Currently advance criteria are temporal not telemetric.

7. **Cost regression gate flake handling is hand-waved.** §1 narrative point 8: "bench latency varies ±15%; gate fails ±10%. Mitigação: bench warm-up + multiple runs (5×); take median; flake retry 3×." 5 runs + median + 3 flake-retries = up to 15 runs per CI. At criterion 1-2 min per run × 5+ runs × 5 ops (verify/sign/GET/UPDATE/eviction) = 25-50 min. CI budget impact undocumented. Also: "median-of-5 with flake-retry 3" is statistically weak vs trimmed mean or running-window; document the actual statistical method.

8. **RB-FM-303 dry-run "post-mortem written; runbook gaps documented + iterated" lacks acceptance criteria.** §6.1.4 + Gherkin: "Dry-run is gate; gap → iterate runbook → re-run; cannot ship until clean execution; post-mortem documents iteration." What is "clean execution"? If detection takes 6min instead of 5min — gap or acceptable noise? If customer notification takes 75min instead of 60min — gap or noise? Add tolerances or absolute "clean = all targets within +20% of spec."

9. **DASH-AC alert "noisy" mitigation is reactive.** §1 point 5 + chaos #5 mention threshold tuning but only after noise observed. Pre-emptive: alert thresholds should be derived from synthetic workload baselines (chaos test 7 days prior); document the calibration procedure. Currently `>5/h sig invalid` is asserted without baseline justification.

10. **Synthetic Bazel workload calibration is critical-path but undersized in sub-tasks.** ST-009 = 4h. Calibrating a synthetic workload that produces ≥ 70% cache hit ratio on first deploy, against real Bazel project structures, with reproducibility guarantees for external audit, is a 20-40h effort. The current 4h is "set up the harness" not "calibrate the workload." If the synthetic workload produces 50% hit ratio in staging, the cache hit ratio business métrica panel shows 50%, customer dashboard S-16 shows 50%, and the SLA addendum's ≥ 70% promise is broken before customer traffic arrives. Block fix: pre-S-04-PRR a Product+Architect review of the workload definition is needed.

11. **Customer-facing comm "ready" gate has no review chain.** §6.1.10 + Gherkin: "all drafts complete; reviewed by Product + Compliance + Privacy." Compliance and Privacy are both `_TBD_` in the sign-off table. If they don't exist, who signs off on the SLA addendum? Realistically the docs ship with Owner + Product (both Gustavo) review only. Document the actual review chain or amend the gate.

12. **Quarterly REAPI conformance bump cadence has no escalation path for emergency Bazel security release.** §1 + §6.1.10 + chaos #12: "ADR for bumps; quarterly cadence." Bazel/REAPI has had emergency security releases (cve-driven). Quarterly cadence is too slow for emergencies. Add: "emergency bump path: bypass quarterly via Architect approval + ADR within 1 sprint." Currently not specified.

13. **19 INVs §3.15 promotion plan is not actually a plan.** §12 lists the 19 INVs and says "Total: 19 INVs in §3.15 to be promovidas in Lote 10.4bis (P0 fix)." The promotion sub-task is ST-021 = 2h. R4 reviews of S-01/S-02/S-03 repeatedly flagged that "X INVs to be promoted in §3.15" never actually got executed (registry was not updated post-SEAL). 2h to write 19 invariant entries in the registry is plausible but the *integration test that the registry is consistent with the INVs declared in WIs* requires CI tooling. Add: CI gate "for each WI declaring INV-X, INV-X exists in `invariant_registry.md` §3.15" — this is the program's persistent gap.

14. **19 INVs cross-check vs WI sources reveals a gap.** The list includes `INV-AC-NEG-CACHE-INVALIDATED-ON-UPDATE` (sourced WI-S04-001), `INV-AC-RESULT-HASH-IMMUTABLE` (WI-S04-001), and `INV-AC-IDEMPOTENT` (WI-S04-001) — but I do not see `INV-AC-OUTPUTS-VALID` source attribution to a specific WI implementation in the §12 list. Sprint contract §8 lists INV-AC-OUTPUTS-VALID as **mantida** (not new). Whose chaos test enforces it under TTL eviction race + S-06 GC race? WI-S04-003 §6 mentions `OutputsValidator` but the cumulative validation in WI-006 §12 doesn't cite it. Verify completeness; either add to the 19 list or document why mantida invariants are out-of-scope for §3.15 promotion.

**P2 — next sprint or doc-only:**

15. ST-019 "Final review iteration (Architect + AppSec + Crypto SME + Security Lead): 6h" — 6h for final review across 4 specialists is extremely tight; a real architecture/security final review is 1-2 days each.

16. §14.6.7 cost regression gate ±10% tolerance — same number across all WIs without per-op variance characterization; verify p99 cost variance differs from GET p99 cost variance; one threshold may be too loose for sign and too tight for verify.

17. §22 cost analysis: $850/yr ship gate maintenance excludes Crypto SME ongoing engagement (post-ship review per advisory waiver path); add ongoing cripto consultation cost line.

18. §28 R-014 "Production rollout incident at 50%" Probability=M Impact=HIGH Exposure=M Residual=LOW — residual LOW is optimistic given the documented mitigation is "rollback procedure documented + customer comm template ready," not active incident response capability.

19. §6.1.6 PRR meeting "2h structured" — for HIGH_RISK 13 sign-off review across 5 WIs, 2h is exceptionally tight (8 minutes per agenda item). Realistic minimum 4h.

20. ADR-0036 "schema migration governance" referenced multiple times but I haven't seen its content; assume it exists from prior sprints. Confirm ratification status.

---

## Cross-WI Consistency Check (WI-004 / WI-005 / WI-006 + handoffs to WI-001/002/003)

**Dependency graph correctness:**
- WI-006 declares hard-blockers on WI-001..005 SEALED — declared correctly.
- WI-004 declares hard-blocker on WI-003 SEALED (sub-crate of corelink-ac) — declared.
- WI-005 declares hard-blockers on WI-001 (refresh-on-hit consumed by handler) + WI-002 (D1 ac_meta + R2 bucket) + ADR-0019 (already ratificada) — declared.
- WI-004 declares **soft** blocker on WI-001 (handler consumes traits) and **soft** on WI-006 (outbound) — declared correctly. But WI-001 step [4] handler verify path should be a hard blocker for WI-004 traits consumed by WI-001; currently soft. Minor.
- WI-005 declares **soft** blocker on S-07 (per-tier defaults) — correct given ADR-0019 boundary.

**ADR whitelisting:**
- ADR-0021 (WI-004), ADR-0034 (WI-006 staffing waiver), ADR-0035 (WI-001), ADR-0036 (WI-002), ADR-0037 (WI-003), ADR-0019 (WI-005) — WI-006 §6.1.10 claims "all four" whitelisted but lists 4 (0021/0035/0036/0037), missing 0034 and 0019. Either 0034 is a forward ADR not in scope or the count is wrong. Verify against `scripts/validate_references.py`.

**Sign-off harmonization:**
- WI-004 emphasizes Crypto SME MANDATORY EMPHATIC + AppSec mandatory emphatic + Architect mandatory.
- WI-005 emphasizes Architect mandatory + AppSec mandatory + DBA advisory.
- WI-006 emphasizes Security Lead mandatory + Architect mandatory + AppSec mandatory emphatic + Crypto SME advisory.
- **Inconsistency**: Crypto SME is MANDATORY EMPHATIC in WI-004 and ADVISORY in WI-006. (Already flagged P0 in WI-006 #2.)

**Shared technical primitives:**
- `tenant_prefix = HMAC(TDK, tenant_id)[:16]` — consistent across WI-001/002/005 (path layer 4 of 5-layer defense).
- `sig_key_id` rotation versioning — consistent across WI-002 (D1 column) and WI-004 (verifier accepts current + 1 prev).
- HKDF info string `b"ac-sig"` — consistent in WI-004; WI-001 step [6] mentions `info="ac-sig"` correctly.
- `canonical_bytes` layout 89 bytes — defined in WI-004 only; WI-001/003 don't reproduce or cross-reference the layout. P1: cross-reference in WI-003 §1 or §6.

**Forward INVs declared (cumulative §12 in WI-006):**
- 19 INVs listed: TENANT-SCOPED, OUTPUTS-VALID, MERKLE-VALID, DIGEST-SIGNED, MERKLE-DETERMINISTIC, IDEMPOTENT, RESULT-HASH-IMMUTABLE, EVICT-TENANT-SCOPED, TTL-MONOTONIC, NEG-CACHE-INVALIDATED-ON-UPDATE, BOUNDED-PARSER, CYCLE-FREE, SIG-CONSTANT-TIME, SIG-INFO-FIXED, KEY-ROTATION-GRACE, TDK-ZEROIZED, CANONICAL-BYTES-STABLE, DUAL-SIDE-VERIFY, EVICT-CONSISTENCY.
- Cross-WI source attribution checks out for all 19.
- **Persistent program gap**: Lote 10.4bis ST-021 (2h) is the registry update. The S-01/S-02/S-03 R4 reviews repeatedly flagged that INVs declared in WIs never actually landed in `invariant_registry.md` §3.15 post-SEAL. There's no CI gate enforcing "if WI declares INV-X, registry §3.15 contains INV-X." Without CI, this drifts every sprint.

**TLA+ alignment:**
- WI-004 references `cas_integrity.tla`.
- WI-005 references `tenant_isolation.tla`.
- WI-006 references `tenant_isolation.tla AC variant; cas_integrity.tla extension for AC.`
- Per S-03 R4 reviews, none of these specs actually exist yet (planned forward). The alignment claim is hedged but the §11 DoD "TLA+ verde" is unsatisfiable until the specs exist. Same defect as WI-S03-008 #8.

**Staging environment ownership unstated.**
- All three WIs assume `staging.corelink.humangr.com` exists with full S-04 stack deployed. There is no sub-task (in any of the 6 WIs) that provisions/maintains staging. This is a sprint-level dependency not surfaced. Cross-cutting with S-03 part 2 R4 review #11.

---

## Comparison vs Canonical SOTA Bar (WI-S03-007 best-in-class 8.5)

WI-S03-007 (audit events EVT-047) earned 8.5/10 by:
- Strongest semantic clarity (CloudEvents 1.0 envelope, 23 event types, redaction macro architecture).
- Compile-time enforcement primitives (`#[non_exhaustive]`, redaction macros, deterministic JSON).
- Cross-WI integration ready (per-WI emit hooks listed).
- Per-tenant retention hint as event field (S-11 worker tenant-tier-aware without re-querying).
- Threat model complete (STRIDE+LINDDUN delta full, per-event-type forensic granularity).

**WI-S04-004 vs S03-007 bar:**
- Strengths beyond 8.5: Mann-Whitney 3-prong cripto-grade tighter than middleware (0.5ms vs 5ms); ADR-0021 ratificação explicit; cargo-fuzz harness; test vector annexes.
- Below 8.5: Crypto correctness imprecisions (BLAKE3 misclassified as MD; HKDF salt rationale weak; canonical_bytes missing result_hash binding); HMAC-SHA256 dismissal too quick; sig_alg metadata orphaned.
- **Concrete improvements to reach 9.0**: (a) fix the 6 P0 cripto items above; (b) book a real Crypto SME 40-80h not 4h ST-018; (c) add explicit FIPS-compliance discussion in ADR-0021; (d) introduce `EncryptedTdk` newtype with no Display/Debug/Serialize as compile-time enforcement (mirrors WI-S03-007 redaction newtype pattern); (e) add CI lint enforcing Hkdf-Extract gets the full input via salt+IKM, not just IKM.

**WI-S04-005 vs S03-007 bar:**
- Strengths matching 8.5: cleanest TTL spec the program has produced; tenant-scoped DELETE strict with multiple defenses; ADR-0019 boundary handoff well-engineered; per-region cron sharding; bounded batch with operational realism.
- Below 8.5: tenant_prefix derivation in cron unspecified (P0 #1 above); D1 batch size limit conflict (P0 #4); audit emission atomicity claim partially wrong (P0 #3); cost arithmetic doesn't sum.
- **Concrete improvements to reach 9.0**: (a) fix the 4 P0s; (b) add `tenant_prefix` materialized column in `ac_meta` (so cron doesn't need TDK access); (c) auto-stagger alarm fires across regions (D1 lock contention); (d) add CI gate "DELETE statement in TTL module must include `tenant_id = ?` clause" via clippy lint or grep; (e) add observability for refresh-on-hit (currently only eviction emits audit).

**WI-S04-006 vs S03-007 bar:**
- Strengths matching 8.5: 19 INV cumulative validation list; gradual production rollout 10%→50%→100%; 13 review checkpoints D+0..D+20 incl. post-ship; SLA addendum + release notes + Bazel onboarding doc pre-ship.
- Below 8.5: contract drift 100% vs §19 95%-waiver (P0 #1); Crypto SME mandatory vs advisory contradiction (P0 #2); staffing reality unresolved (P0 #3); 72h SLO reset criteria subjective (P0 #4); REAPI test set not pinned (P0 #5); production rollout advancement criteria temporal not telemetric (P1 #6); synthetic workload calibration undersized (P1 #10).
- **Concrete improvements to reach 9.0**: (a) fix 5 P0s, especially staffing reality (sprint-level retention plan with names, not paper mitigation); (b) define P0/P1/P2/P3 incident classification in `_spec_contract.md` referenced runbook; (c) replace temporal advancement gates (24h monitor) with telemetric (error budget < 50% consumed + cache hit ratio + zero cross-tenant alerts); (d) pin REAPI test set list in ADR-0036; (e) CI gate "for each WI declaring INV-X, registry §3.15 contains INV-X" — solves the persistent program gap.

---

## Verdict per WI + Aggregate

| WI | Score | Verdict | Highest-Risk Item |
|---|---|---|---|
| WI-S04-004 | 7.7 | **GO-WITH-FIXES**; do not SEAL without real Crypto SME independent review (40-80h, not 4h ST-018) | Canonical bytes missing `result_hash` binding (P0 #1); HKDF salt rationale incorrect (P0 #2); BLAKE3 mischaracterized as Merkle-Damgård (P0 #3) |
| WI-S04-005 | 8.0 | **GO-WITH-FIXES**; cleanest of the trio | tenant_prefix derivation in cron unspecified — cron may need TDK access (P0 #1); D1 batch 100KB limit conflict (P0 #4) |
| WI-S04-006 | 7.8 | **GO-WITH-FIXES**; PRR cannot convene with 9-of-13 sign-offs unstaffed | Contract drift 100% vs §19 95%-waiver (P0 #1); Crypto SME mandatory vs advisory contradiction (P0 #2); staffing reality (P0 #3) |

**Aggregate part 2 score: 7.83/10** = (7.7 + 8.0 + 7.8) / 3.

Comparison points:
- S-03 part 1 average: 7.6/10
- S-03 part 2 average: 7.95/10 (with WI-S03-007 at 8.5 best-in-class)
- S-04 part 2 average: **7.83/10** (no WI reaches 8.5; WI-005 closest at 8.0)
- User's SOTA target: 9-10. **Gap to target: ~1.2 points; closable with Lote 10.4bis P0 fixes.**

---

## Lote 10.4bis P0 Fix Plan

### Day 0 (immediate; before fix-cycle starts)

1. **Resolve Crypto SME mandatory vs advisory contradiction** (cross WI-004 / WI-006). Decision: WI-004 SME review is non-waivable pre-PRR (40-80h booking); WI-006 PRR ceremony may proceed with that sign-off serving as cripto domain. Document in both WIs and `_spec_contract.md`.

2. **Resolve 100% conformance vs sprint contract §19 95%-waiver.** Decision recommended: amend sprint contract §19 to remove the 95% waiver path (or explicitly mark it deferred to S-XX post-GA). HIGH_RISK GA should not ship with a 5% conformance gap.

3. **Resolve PRR sign-off staffing reality.** Either: explicit retention plan with names by D-0 sprint kickoff, OR amend mandatory sign-off count, OR accept staffing-blocked SEAL slip. **Cannot ship without.**

### Day 1-2 (WI-004 cripto correctness fixes)

4. **Fix HKDF salt parameterization.** Recommendation: `salt = sig_key_id.to_le_bytes()` (4 bytes); update §1 code; update §6.1.5 Constraint cripto-driven; update ADR-0021 rationale; add CI byte-equal test for salt as well as info.

5. **Fix canonical_bytes binding to include result_hash OR enforce verify_full API monolith.** Recommendation (a): extend canonical_bytes layout to include `BLAKE3(serialized_result_canonical)` (32 bytes; total 121 bytes); update §1 code, §6.1.5 layout, §6.1.9 properties; or recommendation (b): make `verify_sig` private in the trait and only `verify_full` public (forces structure-then-sig).

6. **Fix BLAKE3 mischaracterization.** §9.2 rewrite per P0 #3 above. Quick fix; high reader-confidence impact.

7. **Add explicit FIPS-compliance posture in ADR-0021.** §1 ADR-0021 Risks accepted: add row "BLAKE3-keyed-hash is not FIPS 140-3 approved; SLSA L3 alignment via Merkle + audit chain; FedRAMP/SOC2 customers requiring FIPS MAC will need post-GA migration to HMAC-SHA256 or KMAC."

8. **Add explicit side-channel discussion in ADR-0021.** Per P1 #13.

9. **Resolve `sig_alg` orphan metadata.** Either widen `verify_sig` API to take `sig_alg` and dispatch, OR remove `sig_alg` from `AcEnvelope` in v1 (reserve for v2+ ADR migration).

### Day 3 (WI-005 P0 fixes)

10. **Specify `tenant_prefix` materialization in `ac_meta`** — add column `tenant_prefix BLOB(16) NOT NULL`; pre-compute at INSERT in WI-001 step [7]. Update WI-005 §1 cron flow. Update WI-002 §6.1 schema. **This is a cross-WI change; coordinate.**

11. **Reconcile R2-then-D1 ordering claim §9.3.** Rewrite per P0 #2 above.

12. **Document audit emission atomicity boundary explicitly.** §9.8 rewrite per P0 #3 above.

13. **Reduce batch size from 1000 to 250 for D1 100KB limit OR document explicit chunking.** Per P0 #4. Update §1, §6.1, §9.2, chaos #4.

### Day 3-4 (WI-006 ship gate fixes)

14. **Pin REAPI test set list in ADR-0036.** Enumerate the conformance subset with commit hash; update WI-006 §6.1.1.

15. **Define P0/P1/P2/P3 incident classification.** Add to `_spec_contract.md` or referenced runbook; require Architect+Security Lead joint classification; auto-reset on DASH-AC alert sustained > 5min.

16. **Replace temporal advancement gates with telemetric.** Per P1 #6.

17. **Add CI gate "WI INV declarations must exist in `invariant_registry.md` §3.15".** Persistent program gap; closes at Lote 10.4bis.

### Day 5 (cross-WI hygiene)

18. **CI gate enforcing TTL DELETE has tenant_id clause.** Clippy lint or grep on `crates/corelink-worker/src/ac/ttl/`.

19. **Add `EncryptedTdk` newtype** (mirrors WI-S03-007 RedactedPrincipal) with no Display/Debug/Serialize.

20. **Verify ADR whitelist count.** WI-006 §6.1.10 says 4 ADRs; should be 5-6 (incl. 0019, 0034). Reconcile.

### Day 6 (re-review by R4)

Re-run targeted re-review on the touched sections; aim for WI-004 → 8.5+, WI-005 → 8.7+, WI-006 → 8.5+, aggregate 8.6+. Closes the gap to the 9-10 SOTA target.

---

## Final Verdict

**GO-WITH-FIXES** for all three WIs; **do not SEAL Lote 10.4 without addressing the 12 P0 items** (3 cross-WI + 6 WI-004 + 4 WI-005 + 5 WI-006, with overlap).

The trio reflects genuine engineering ambition (Mann-Whitney 0.5ms gate; tenant-scoped DELETE strict; gradual production rollout) and the SOTA discipline (32 sections; 13-row sign-off; 12 chaos; cumulative INV §3.15). But it has not yet earned the 9-10 SOTA bar that the user has set: WI-004 has crypto correctness imprecisions that a real Crypto SME will surface on first read; WI-005 has a tenant_prefix derivation gap that expands the cron's trust boundary silently; WI-006 has the same staffing reality gap and contract drift gap that WI-S03-008 had — *carried forward unfixed*. The recurring program-level defects (INV registry never updated post-SEAL; staffing-blocked Tier-1 reviewers; sprint contract drift) need a system-level fix, not per-WI patches.

The single highest-leverage Lote 10.4bis action: **add the CI gate "WI INV declarations must exist in `invariant_registry.md` §3.15"**. This closes a 4-sprint-old persistent gap with one tooling investment. The second-highest: **resolve the Crypto SME mandatory-vs-advisory contradiction** by booking a real 40-80h cripto review for WI-004 — the program cannot ship cripto without a real cripto review, and the current 4h ST-018 budget is below industry-norm by 10×.

If Lote 10.4bis lands all 12 P0s + the high-leverage system fixes, the trio reaches **8.5-8.7/10 average**, comparable to WI-S03-007's best-in-class. The remaining gap to 9-10 is held by (a) the 19 INVs actually being in the registry post-SEAL (not just claimed), (b) actual staffed sign-offs (not _TBD_), (c) actual TLA+ specs that exist (not "planned forward"). Those are sprint-cycle structural items, not per-WI items; the program needs a system-level lift before any single sprint can hit 9-10.

---

**Reviewer**: Agent R4 (Claude Opus 4.7, 1M context)
**File**: `/Users/gustavoschneiter/Documents/HuGR/corelink-server/specs/_audits/2026-04-25-agent-r4-s04-part2-wi-review.md`
**Status**: COMPLETE
