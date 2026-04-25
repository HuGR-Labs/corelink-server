---
id: AUDIT-SONNET-R5-S04-WI-REVIEW
type: audit
doc_status: REVIEW
audit_status: ACTIVE
created: 2026-04-25
reviewer: Sonnet 4.6 (independent adversarial reviewer — round 5, different model from Opus R4)
scope: Lote 10.4 — Sprint S-04 all 6 WIs (WI-S04-001 through WI-S04-006) + post-Lote-10.4bis patches
sprint_contract: specs/04_sprints/S04/_spec_contract.md v1.1.0
parent_audit: specs/_audits/2026-04-25-agent-r4-s04-part1-wi-review.md
tags: [audit, sota, lote-10.4, s-04, sonnet-r5, independent]
calibration_baseline:
  - WI-S04-003 = 8.6/10 (best in S-04 per R4)
  - WI-S03-003 = 8.5/10 (program best-in-class baseline)
files_reviewed:
  - specs/04_sprints/S04/_spec_contract.md
  - specs/04_sprints/S04/work_items/WI-S04-001-reapi-actioncache-handlers.md
  - specs/04_sprints/S04/work_items/WI-S04-002-d1-ac-meta-r2-bucket.md
  - specs/04_sprints/S04/work_items/WI-S04-003-corelink-ac-merkle-dual-side.md
  - specs/04_sprints/S04/work_items/WI-S04-004-hkdf-digest-signing-adr-0021.md
  - specs/04_sprints/S04/work_items/WI-S04-005-ttl-worker-cron-do-adr-0019.md
  - specs/04_sprints/S04/work_items/WI-S04-006-reapi-conformance-prr-ship-gate.md
cross_references:
  - specs/_audits/2026-04-25-agent-r4-s04-part1-wi-review.md
  - specs/_audits/2026-04-25-agent-r4-s04-part2-wi-review.md
  - specs/03_architecture/invariant_registry.md (§3.15 promoted)
  - specs/03_architecture/adrs/ADR-0019-ttl-ownership-s04-s07.md
  - specs/03_architecture/adrs/ADR-0020-quota-ownership-s07-s08.md
---

# Sonnet R5 — Lote 10.4 S-04 Full WI Adversarial Review (Rounds 5)

> **Reviewer**: Sonnet 4.6 (independent adversarial reviewer — round 5, different model from Opus R4).
> **Date**: 2026-04-25
> **Calibration**: "Average não serve. SOTA puro 9-10." Brutally technical; no diplomacy.
> **Note on scope**: Lote 10.4bis P0 fixes were applied BEFORE this review. I am reviewing the post-patch state
> of all 6 WIs and their cross-document stitching. Confirmed P0 fixes verified in-WI. Residual defects are
> issues that Opus R4 either missed entirely or under-specified.

---

## 1. Executive Verdict

Lote 10.4bis absorbed a substantial P0 load from Opus R4 (13 P0 items across 6 WIs). Many are visibly fixed
inline: CHECK constraints are now in CREATE TABLE; `with_tenant_ctx!` claim is corrected; canonical_bytes
extended to 121 bytes; salt binds `sig_key_id`; `MerkleError::CycleDetected` variant added; `BLAKE3` description
corrected (Bao tree, not Merkle-Damgård); `BatchUpdateActionResult` struck; `tenant_prefix` materialized as
column; batch size capped at 250. Those are all closed.

What follows is what Lote 10.4bis did NOT fix and what Opus R4 missed entirely.

**Bottom line**: 6 residual defects rise to P0 level (must-fix before any SEAL). 11 at P1. The cripto-specific
residual findings are the most dangerous: the salt-binding fix introduced a new interaction ambiguity with the
grace-period verifier; the `info` domain-separation argument is formally incomplete; and the 13-row sign-off
table still has a structural contradiction between WI-004 (emphatic mandatory) and WI-006 (advisory waivable)
that was flagged by R4 but remains unfixed at the WI text level.

**Aggregate score post-10.4bis patches (Sonnet R5 perspective)**:

| WI | R4 Score | Sonnet R5 Adjusted | Delta |
|---|---|---|---|
| WI-S04-001 | 7.7 | 7.9 | +0.2 (P0 fixes landed; residual: sign flow race) |
| WI-S04-002 | 7.6 | 7.8 | +0.2 (P0 fixes landed; residual: path_key_id→sig_key_id coupling) |
| WI-S04-003 | 8.6 | 8.7 | +0.1 (strong; residual: verify_full ordering not enforced by type) |
| WI-S04-004 | 7.7 | 7.8 | +0.1 (salt fix landed; residual: HKDF domain separation incomplete) |
| WI-S04-005 | 8.0 | 8.1 | +0.1 (batch fix landed; residual: alarm panic re-arm race) |
| WI-S04-006 | 7.8 | 7.7 | -0.1 (Crypto SME contradiction still present; 10-test conformance set is dangerously small) |

**Sprint average (Sonnet R5): 8.0/10** — consistent with R4's 7.94 average, reflecting that the Lote 10.4bis
patch absorbed the easy structural P0s but left the cryptographic precision gaps and governance contradictions
unresolved.

---

## 2. Residual P0 Findings (Opus R4 missed or under-specified)

### P0-R5-001 — HKDF salt fix creates verifier-ambiguity when grace key_id = 0 (WI-S04-004)

**Severity**: P0 cripto-load-bearing. **File**: WI-S04-004 §1 HkdfVerifier impl, §9.3, §12.

Lote 10.4bis correctly added `salt = sig_key_id.to_le_bytes()` to both signer and verifier. But:

The `HkdfVerifier` computes:
```rust
let salt = sig_key_id.to_le_bytes();  // 4 bytes
let hk = Hkdf::<Sha256>::new(Some(&salt), &tdk);
```

`sig_key_id` is `u32`. Key_id `0` → salt = `[0x00, 0x00, 0x00, 0x00]`.

RFC 5869 §2.2 states: "if not provided, [salt] is set to a string of HashLen zeros." With `Sha256`, HashLen = 32
bytes. The spec's 4-byte salt `[0x00; 4]` is NOT equivalent to the "not provided" case (which uses 32 zero bytes
as salt). So key_id=0 produces a distinct HKDF output from the salt-less case — that's correct and intended.

**BUT**: the spec never declares what key_id=0 MEANS. Looking at `accepted_key_ids: Vec<u32>`, the
`oldest_active` fallback is `*self.accepted_key_ids.iter().min().unwrap_or(&0)`. If `accepted_key_ids` is
unexpectedly empty (e.g., misconfiguration, race during init), `unwrap_or(&0)` silently falls back to 0, and
`SigError::KeyIdUnknown` returns `oldest_active: 0`. This is a sentinel value leak: an attacker who can trigger
an empty `accepted_key_ids` state (e.g., via a concurrent rotation race that briefly drains the list before
repopulating) gets `KeyIdUnknown { oldest_active: 0 }` in the error response, revealing that the keyring is
transiently empty.

More critically: if a future rotation implementation ever attempts to provision with `current_key_id = 0` as a
starting value (not unreasonable for a new tenant), `salt = [0x00; 4]` is a very low-entropy salt for HKDF
extract — not RFC 5869 non-compliant, but the Crypto SME will ask why you're using a predictable all-zeros-ish
4-byte salt when the alternative (32 zero bytes from the RFC) is the known safe default. The asymmetry between
4-byte and 32-byte zeros is confusing enough to flag.

**Fix**:
- Reserve `key_id = 0` as sentinel ("never-issued") and document explicitly. Rotation starts from `key_id = 1`.
- Change `unwrap_or(&0)` to `unwrap_or(&1)` to avoid the sentinel in error disclosure.
- Add a `debug_assert!(!self.accepted_key_ids.is_empty())` in production verifier init path.
- Document in ADR-0021 §Risks: "key_id is u32 starting at 1; key_id=0 is reserved sentinel; HKDF salt
  derivation for key_id=0 uses a 4-byte all-zero salt, which is distinct from RFC 5869 default (32 zero bytes);
  reserved/unused."

---

### P0-R5-002 — HKDF `info` domain separation is incomplete: `b"ac-sig"` does NOT separate from other HKDF uses of the same TDK (WI-S04-004, WI-S04-001)

**Severity**: P0 cripto-load-bearing. **Files**: WI-S04-004 §1, §9.4; WI-S04-001 §1; sprint contract §5.

The HKDF usage in WI-S04-004:
```
HKDF-Extract(salt=sig_key_id_bytes, IKM=TDK) → PRK
HKDF-Expand(PRK, info=b"ac-sig", len=32) → sig_key
```

The TDK (Tenant Derivation Key) is the **same key** used for R2 path derivation:
```
tenant_prefix = HMAC(TDK, tenant_id)[:16]  // from corelink-tenant-path, S-01
```

This means a single TDK is used as keying material for:
1. Path-HMAC derivation (raw HMAC with `tenant_id` as the message; S-01 WI-S01-001).
2. HKDF-Extract→Expand chain for sig_key (this WI).

These two usages share the same root secret without a shared HKDF framework. The path-derivation is a direct
raw HMAC (not through HKDF-Extract first), while the sig derivation IS through HKDF-Extract. This is not
formally a key-reuse violation (the outputs are structurally different — raw HMAC vs HKDF output), but:

**The `info=b"ac-sig"` string in the HKDF expansion is the domain separator for expansion outputs**, not for the
IKM (TDK) itself. HKDF-Extract(salt, TDK) → PRK is the step that "randomizes" the key material. If the TDK is
also used raw as HMAC key for path derivation, an adversary who observes many `(HMAC(TDK, tenant_id_n), path_n)`
pairs could in theory learn information about TDK structure that aids in recovering PRK.

In practice, HMAC security means direct key recovery from outputs is infeasible. But the **formal security
model** says: "all uses of TDK should go through a single HKDF-Extract step before any derivation." The current
spec has two categories of TDK use with different formal properties. The Crypto SME will catch this.

**Specifically**: `corelink-tenant-path::derive_prefix(TDK, tenant_id)` uses raw HMAC-SHA256 (or BLAKE3-keyed)
directly on TDK. The HKDF sig derivation uses TDK as IKM into HKDF-Extract. These are not compositionally
secure in the formal sense (multi-derivation from the same IKM without a unified Extract step).

**Fix options** (Crypto SME must decide):
- (a) Route ALL TDK-derived keys through a single HKDF-Extract(TDK, salt=tenant_id) → PRK per-tenant, then
  use HKDF-Expand with different info strings: `info=b"ac-sig-v1"` for sig, `info=b"path-prefix-v1"` for
  path derivation. This is the SOTA approach (same pattern as TLS 1.3, HKDF-based ECDHE derivation).
- (b) Document explicitly that path-HMAC and HKDF-sig use of TDK are analyzed under the assumption of HMAC
  security for the path, and formal independence is not guaranteed but attack cost is bounded by 2^128; accept
  in ADR-0021. This is weaker but potentially acceptable.

The current spec is **silent on this composition**. That silence is the defect.

---

### P0-R5-003 — `canonical_bytes` offset 81..113 claims `result_hash` but schema says `result_hash = merkle_root`; signed binding is therefore redundant and misleading (WI-S04-004, WI-S04-002)

**Severity**: P0 cripto-clarity / spec self-contradiction. **Files**: WI-S04-004 §1 canonical.rs comment,
WI-S04-002 §1 schema comment, invariant registry §3.15 INV-AC-CANONICAL-BYTES-STABLE.

WI-S04-004 §1 canonical_bytes layout comment:
```
81      32    result_hash (= merkle_root direct per ADR-0037; redundant binding for
               defense-in-depth + future schema migration if result_hash semantic decouples)
```

WI-S04-002 §1 schema:
```sql
result_hash TEXT NOT NULL  -- 64 hex chars = BLAKE3-256 of merkle_root
```

So `result_hash = BLAKE3(merkle_root_bytes)`. But `merkle_root` is already in the signed canonical_bytes at
offset 49..81. The signed binding is therefore:
```
... || merkle_root[49..81] || BLAKE3(merkle_root)[81..113] || ...
```

This is `merkle_root` signed TWICE in two different representations (raw and hashed). This provides ZERO
additional security over signing `merkle_root` once. If an attacker can forge merkle_root, they can trivially
compute `BLAKE3(merkle_root)` to match the result_hash binding. The "defense-in-depth" claim in the comment is
false.

Furthermore, if `result_hash = merkle_root` (as the comment also says in the first interpretation), then the
layout is:
```
49..81: merkle_root bytes
81..113: merkle_root bytes AGAIN (= result_hash per ADR-0037)
```

This is literally signing the same 32 bytes twice with different offsets. The canonical_bytes comment has two
contradictory sub-cases embedded ("= merkle_root direct" vs "= BLAKE3-256 of merkle_root") and neither adds
security. The schema column says BLAKE3(merkle_root); the WI-004 intent says direct = merkle_root. **Pick one
and document why.**

The deeper issue: Lote 10.4bis added result_hash to canonical_bytes specifically to prevent API misuse where
`verify_sig` is called WITHOUT prior `verify_structure`. But that misuse vector was separately closed by making
`verify_sig` internal-only (only `verify_full` is public). If `verify_sig` is private, the API-misuse reason for
adding result_hash is moot. The canonical_bytes is now 121 bytes for a reason that no longer exists.

**Fix**:
- Decide the actual semantic of result_hash: (a) `= merkle_root` (store merkle_root in two columns — confusing),
  or (b) `= BLAKE3(some_content_that_is_NOT_merkle_root)` (e.g., BLAKE3 of the serialized ActionResult proto —
  which was the ORIGINAL intent before ADR-0037 removed proto-bytes). If it's (b), the Merkle root is NOT
  sufficient and the earlier Opus R4 P0 finding (protobuf determinism) is re-opened.
- If `verify_sig` is truly private and `verify_full` forces struct+sig ordering, consider reverting canonical_bytes
  to 89 bytes (remove redundant result_hash) and document the rationale clearly in ADR-0021.
- The invariant registry §3.15 `INV-AC-CANONICAL-BYTES-STABLE` claims "121 bytes incl. result_hash binding
  (Lote 10.4bis fix WI-S04-004 P0 #1)" without explaining what result_hash binds. This must be unambiguous.

---

### P0-R5-004 — WI-S04-006 Gherkin §8 still references `BatchUpdateActionResult` after Lote 10.4bis "removal" (WI-S04-006)

**Severity**: P0 spec integrity. **File**: WI-S04-006 §8 Gherkin, §6.1.1.

WI-S04-006 §1 correctly notes the Lote 10.4bis fix: "REAPI v2 has NO batch RPC on ActionCache
(only on CAS via WI-S01-005 BatchUpdateBlobs)."

WI-S04-006 §6.1.1 correctly removes the batch mention from the enumerated conformance test subset.

**BUT** WI-S04-006 §8 Gherkin scenario:
```gherkin
Scenario: REAPI v2 conformance suite 100% pass
  ...
  Then 100% AC ops pass (GetActionResult, UpdateActionResult, BatchUpdateActionResult)
```

`BatchUpdateActionResult` is STILL in the Gherkin `Then` clause. This is the definition of what "100% pass"
means in the acceptance criterion. The Gherkin is the contract; the prose is the explanation. The Gherkin has
not been updated to match the fix.

This is a direct contradiction within WI-S04-006 between the fix stated in §1 (removed) and the Gherkin still
asserting the non-existent RPC. If CI automation is written against the Gherkin, it will attempt to run a
`BatchUpdateActionResult` test that does not exist in the REAPI v2 conformance suite and either always pass
(because the test is absent, so "100% of BatchUpdateActionResult tests = 0/0 = vacuously true") or fail.

**Fix**: Strike `BatchUpdateActionResult` from the Gherkin `Then` clause in §8. Trivial fix that was missed in
the Lote 10.4bis sweep of the file.

---

### P0-R5-005 — Crypto SME sign-off contradiction between WI-S04-004 and WI-S04-006 is STILL present post-Lote 10.4bis (WI-S04-004, WI-S04-006)

**Severity**: P0 governance. **Files**: WI-S04-004 §16 PRR, §30; WI-S04-006 §6.1.6, §8 Gherkin, §30.

Opus R4 flagged this in part 2 (P0 #14 WI-004; P0 #2 WI-006). The post-Lote 10.4bis documents were reviewed.

**WI-S04-004 §30 row 13**: `**MANDATORY EMPHATIC**` — non-negotiable.
**WI-S04-004 §16 PRR**: "Crypto SME mandatory emphatic."
**WI-S04-004 §2 Narrative last line**: "13 sign-offs incl. **Crypto SME emphatic mandatory**."

**WI-S04-006 §6.1.6**: "Crypto SME (advisory)... advisory permits ship if Crypto SME unavailable on ship date
but post-ship review committed."
**WI-S04-006 §8 Gherkin**: "Scenario: Crypto SME advisory waiver (acceptable per ADR-0034)"
**WI-S04-006 §1 ship gate components**: "Crypto SME (advisory)" in the PRR 13 sign-offs list.
**WI-S04-006 §9.9 design decision**: "Crypto SME advisory + mandatory split."

The contradiction is explicit and unresolved. The Lote 10.4bis patch notes (visible in comments within the
WIs) did NOT resolve this; the text still diverges.

This is not a style inconsistency. This is a governance failure with concrete consequences: when PRR convenes,
the Crypto SME either can or cannot block ship. If WI-004 is authoritative, the SME CAN block (mandatory). If
WI-006 is authoritative, ship can proceed without the SME (advisory waiver). The PRR chair has no canonical
ruling.

**The correct resolution** (Sonnet's position, differs from R4):
Crypto SME MANDATORY for WI-004 implementation review (the crypto code review must happen before code is
considered SEALED). Crypto SME ADVISORY for the PRR ceremony in WI-006 — because the SME's work is done at
WI-004 sign-off. This IS the correct distinction, but it must be written into BOTH WIs explicitly:

- WI-S04-004 §30: "Crypto SME mandatory emphatic — required before WI-004 SEAL."
- WI-S04-006 §30 row 13: "Crypto SME (advisory at PRR ceremony — substantive review happened at WI-004 SEAL;
  PRR advisory role only)."
- Add a sentence to WI-S04-006 §9.9 explaining this distinction explicitly.

---

### P0-R5-006 — `path_key_id` and `sig_key_id` are independent columns but rotation procedures do not document their independent lifecycle (WI-S04-002, WI-S04-004, WI-S04-005)

**Severity**: P0 operational correctness. **Files**: WI-S04-002 §1 schema, WI-S04-004 §6.1 rotation,
WI-S04-005 §1 eviction flow.

WI-S04-002 schema has two key-version columns:
```sql
sig_key_id    INTEGER NOT NULL DEFAULT 1  -- HKDF tenant_key rotation
path_key_id   INTEGER NOT NULL DEFAULT 1  -- TDK rotation version for path derivation
```

These are independent: `sig_key_id` governs which TDK version was used for HKDF-SHA256 signing.
`path_key_id` governs which TDK version was used for path prefix derivation `HMAC(TDK_v<n>, tenant_id)[:16]`.

Critically: **they can diverge**. An entry could have `sig_key_id=1, path_key_id=2` if the TDK rotation
happened between the HMAC path computation and the HKDF signing in the UpdateActionResult flow. This is not a
theoretical edge case — any concurrent rotation during a write creates this window.

The WIs are completely silent on what happens when `sig_key_id != path_key_id`. The eviction flow (WI-005)
reads `tenant_prefix` from the materialized column and uses it to construct the R2 path for deletion — this is
correct (it uses the materialized value, independent of current key versions). But the GET handler (WI-001)
reconstructs the R2 path using `derive_prefix(TDK_v<path_key_id>, tenant_id)` — it must look up the correct
TDK version using `path_key_id`, which means the handler ALSO needs TDK access for the path version, not just
the sig version.

This creates a trust boundary requirement that is NOT in WI-001: the GET handler needs access to multiple TDK
versions simultaneously — `TDK_v<sig_key_id>` for signature verification AND `TDK_v<path_key_id>` for path
reconstruction. The TdkHandle trait in WI-004 only handles key fetching by integer ID, so technically it works,
but no document explicitly says "GET handler fetches TDK_v<path_key_id> for path reconstruction separately from
TDK_v<sig_key_id> for sig verification."

Actually — re-reading WI-001 §1: the GET handler uses the materialized `tenant_prefix` column from `ac_meta`
(per the Lote 10.4bis fix), so it does NOT recompute the path from TDK at GET time. This is correct. BUT:
the WI-001 §1 sequence step [3] says:
```
r2::get(ac-<region>/<tenant_prefix>/<action_digest>.json)
```
where does `tenant_prefix` come from here? From the D1 row lookup in step [2], which returns the materialized
BLOB(16). If so, the path reconstruction IS correct and the GET handler does NOT need TDK access for the path.

But this is not stated clearly enough to satisfy a security review. The sequence must explicitly say "tenant_prefix
read from ac_meta row (BLOB column, not recomputed from TDK)" so the reviewer can trace the trust boundary.

Additionally: the UPDATE handler (step [7]) computes tenant_prefix at INSERT time. What TDK version does it use?
WI-001 §1 narrative point 2: "tenant_prefix = HMAC(TDK_v<path_key_id>, tenant_id)[:16]." The current `path_key_id`
is used at INSERT time. But `path_key_id` in the schema defaults to 1 — who increments it when the path TDK
rotates? There is no documented procedure for rotating `path_key_id`. The `sig_key_id` rotation procedure is
documented (WI-004 §6.1.7: bump `current_key_id` in HkdfSigner config). No equivalent exists for `path_key_id`.

**Fix**:
- Add explicit rotation procedures for BOTH `sig_key_id` and `path_key_id` in WI-004 §6.1.7 (or a new sub-section).
- Document what happens when `sig_key_id != path_key_id` on a row — is this valid? Is it an invariant
  violation? Add an assertion in the handler.
- Add `INV-AC-PATH-SIG-KEY-VERSION-DOCUMENTED` or similar clarity note.
- Explicitly state in WI-001 §1 sequence step [3]: "tenant_prefix from D1 row (materialized; NOT recomputed
  from TDK at GET time)."

---

## 3. Residual P1 Findings (Fix before PRR)

### P1-R5-007 — Mann-Whitney baseline for "valid sig vs invalid sig" includes the length-check fast-fail exit, which is NOT constant-time path (WI-S04-004 §6.1.10)

WI-004 §6.1.10 Mann-Whitney 3-prong: "10k samples per arm; valid sigs vs random invalid sigs."

The "invalid sig" arm is not specified to be length-correct vs length-wrong. If the random invalid sig generator
produces sigs of length != 32 with any probability, those samples exit via `SigError::LengthMismatch` which
is a fast-fail (NOT constant-time by design). Including these in the "invalid" timing distribution would
inflate the timing variance for the invalid arm and produce an incorrect |Δmedian| estimate.

The spec explicitly says at §2 narrative point 3: "length check ANTES de constant-time compare (length is
public; not key-dependent); LengthMismatch is fast-fail OK." This is correct design — but the Mann-Whitney
test arm labeled "invalid sig" must ONLY use length-32 invalid sigs (1-byte flips of valid sigs) to test the
constant-time path. The spec doesn't specify this constraint on the test arm composition.

**Fix**: Add to §6.1.10: "invalid sig arm uses only length-32 sigs (1-byte flip at random offset); length-mismatch
variants measured separately and excluded from constant-time gate (they are fast-fail by design)."

---

### P1-R5-008 — `verify_full` ordering guarantee by API design is claimed but not enforced by types (WI-S04-003 §1, WI-S04-004 §1)

WI-S04-004 §1 (Lote 10.4bis addition): "`SignatureVerifier::verify_sig` is **internal-only** (não public);
only `ActionResultVerifier::verify_full` exposed publicly."

This is correct in intent. But the public API in WI-S04-003 §1 declares:
```rust
pub trait ActionResultVerifier {
    fn verify_structure(&self, envelope: &AcEnvelope) -> Result<(), MerkleError>;
    fn verify_full(&self, envelope: &AcEnvelope, sig_verifier: &dyn SignatureVerifier) -> Result<(), VerifyError>;
}
```

`verify_structure` is ALSO public. A caller can call `verify_structure` without calling `verify_full`. That is
correct and intentional (server-side pre-persist calls verify_structure without sig check). BUT: a caller could
also call `verify_structure`, then separately call `verify_full`... but wait, `verify_sig` is internal so a
caller CAN'T call sig verify standalone. The footgun is: if a future handler author calls `verify_structure`
only (no `verify_full`), they skip the sig check entirely. The type system does not prevent this.

The Lote 10.4bis fix prevents the reverse footgun (sig without structure) by hiding `verify_sig`. But it does
NOT prevent the "structure without sig" footgun, which is the UpdateActionResult pre-persist use case (correctly
structure-only). The actual footgun is in GetActionResult, where the handler should call `verify_full` but
COULD call `verify_structure` only — and there's no type-level enforcement of this.

**Fix**: The GET handler code in WI-001 §1 step [4] calls `sig::verify(envelope, tenant_key)` — which should
be `verify_full`. Document explicitly in WI-001 that step [4] MUST call `verify_full` not `verify_structure`,
and add an integration test that asserts the GET handler does call `verify_full` (not just `verify_structure`)
on a tampered sig.

---

### P1-R5-009 — REAPI conformance test set of 10 tests is critically undersized for 100% ship gate (WI-S04-006 §6.1.1)

Lote 10.4bis enumerated the conformance test subset: "10 conformance tests covering AC subset; full list in
ADR-0036 Annex A." The 10 tests listed are: GetActionResult × 4 (hit/miss/expired/wrong-tenant) plus
UpdateActionResult × 6 (success/idempotent/digest-mismatch/payload-too-large/sig-flow/result-hash-mismatch).

This is extremely sparse for REAPI v2 ActionCache. The actual bazelbuild/remote-apis conformance suite for
ActionCache includes cases like:
- Empty `output_files` ActionResult (valid per REAPI v2; some builds produce metadata-only results).
- `output_directories` nested (not just flat files).
- ActionResult with `execution_metadata` set (timing metadata fields).
- `instance_name` variations (mentioned in WI-001 §3 Persona 2 but not in conformance tests).
- Timeout handling (action result with `exit_code` non-zero).
- Large ActionResult approaching payload limits.

10 tests is a comfortable baseline to claim "100% pass" while leaving substantial edge-case surface untested.
At 10 tests, "100% conformance" is a weak invariant.

This is distinct from the Opus R4 P1 finding about "which test binary" — that was about the test set location.
This finding is about the test set SIZE being insufficient to justify the ship gate assertion.

**Fix**: Expand the conformance test set to ≥ 25 tests before claiming the 100% gate is meaningful.
Alternatively, document explicitly that "10 tests is S-04 GA baseline; REAPI conformance suite expansion is a
post-GA backlog item (ADR-0036 evolution)." If the latter, the 100% gate claim should read "100% of the 10
enumerated baseline tests" not an unqualified "100% conformance."

---

### P1-R5-010 — DO alarm panic re-arm race is P1 not P2 (WI-S04-005)

Opus R4 flagged this as P1 (WI-005 P1 finding #5): alarm re-arm at end vs start of tick creates a silently-dying
cron on panic. The Lote 10.4bis patch does NOT address this. WI-005 §1 still shows:
```rust
// Re-arm alarm (at END of alarm handler)
let next = SystemTime::now() + Duration::from_secs(3600);
self.state.storage().set_alarm(next).await?;
```

If the `evict_batch` call panics or returns an error that propagates past the loop, `set_alarm` is never called.
CF DO alarm semantics: on panic, the alarm is retried by CF infrastructure (not silently dropped), BUT the retry
is the same alarm invocation at the same scheduled time, not a newly re-armed future alarm. This means:

1. Alarm fires at T+0, panics.
2. CF retries at T+0 (same invocation, different isolate). If it panics again, CF retries again.
3. Eventually CF marks the alarm as failed. At this point: the DO has no future alarm scheduled (because
   set_alarm at end of handler never ran). **The cron is dead.**

The metric alert `corelink.ac.ttl.cron_tick_rate < expected` catches this in ≤ 1h. But for a TTL worker, 1h
of silent failure is nontrivial: at 10k AC entries and a 90-day default TTL, 1h downtime is negligible. But
under emergency TTL=0 remediation (all entries expired immediately), 1h of cron failure = 1h of no eviction.

**Fix**: Move `set_alarm` to the BEGINNING of the alarm handler (before any work), as Opus R4 recommended. This
ensures even a panicking handler leaves the next alarm scheduled. This is the standard CF DO pattern for
reliable cron.

---

### P1-R5-011 — Negative cache write under audit failure is unspecified (WI-S04-001)

WI-S04-001 §1 UpdateActionResult flow step [9]: "negative_cache::invalidate(action_digest) → KV.delete"
WI-S04-001 §1 UpdateActionResult flow step [10]: "audit::emit(ac.update.ok | ...)"

If the audit emission in step [10] fails (outbox D1 INSERT fails), does the Update return error? If it returns
error, the KV.delete in step [9] has already run. The client retries; the retry calls KV.delete again (idempotent)
and writes R2 again (idempotent via ON CONFLICT) and AGAIN attempts audit emit. This retry loop is safe for
the data path but the AUDIT is permanently missing for the first attempt (the one that "failed" at step [10]).

For a HIGH_RISK sprint where audit trail is a compliance requirement (LGPD Art. 38 cited in §26), a silent audit
miss is a P1. The outbox pattern is supposed to provide at-least-once audit delivery, but the at-least-once
guarantee only holds if the outbox INSERT is atomic with the data INSERT. In the UpdateActionResult flow, the
D1 ac_meta INSERT and audit_outbox INSERT are described as atomic in WI-001 §9.8 ("outbox pattern; pre-emit +
post-emit em mesma D1 batch transaction"). But step [10] is post-all-data-ops, not necessarily atomic with step [8].

**Fix**: Clarify that audit_outbox INSERT for UpdateActionResult is in the SAME D1 batch as ac_meta INSERT
(step [8] and step [10] are one batch), so KV.delete in step [9] runs AFTER the batch commits. If the batch
fails, KV.delete is NOT called, and the client's retry will repopulate the negative cache correctly.

---

### P1-R5-012 — TDK warm cache hit ratio target 95% is stated WITHOUT a definition of "warm" (WI-S04-004 §10.s04.004.5, §14.s04.004.4)

WI-S04-004 §10.s04.004.5: "TDK warm cache hit ratio ≥ 95%."

"Warm" is undefined. CF Workers spawn new isolates on cold starts. Each new isolate starts with an empty
in-memory TDK cache. In a high-concurrency scenario with many Worker isolates (CF scales horizontally), the
fraction of requests hitting warm caches depends on:
- Traffic rate per tenant.
- Worker isolate lifetime.
- Number of concurrent isolates.

At low traffic rates (100 req/min for a given tenant), a Worker isolate may be recycled before the 5-min TTL
expires. The effective cache hit ratio could be << 95% for small customers.

The 95% target is only meaningful if tied to a traffic model. Without one, the metric `corelink.ac.sig.tdk_cache.hit_ratio`
will be measured globally across all tenants and all isolates, where high-traffic enterprise tenants inflate
the average while small free-tier tenants have ~50% hit ratio.

**Fix**: Define "warm" as "tenant with > X requests/min sustained (where X produces ≥ 5-min expected isolate
lifetime)." Acknowledge that free-tier low-traffic tenants will have lower hit ratio and this is expected/accepted.
The SLO target 95% applies to tenants at ≥ the defined traffic threshold.

---

### P1-R5-013 — `sig_alg = 'hkdf-sha256'` CHECK constraint precludes algorithm migration without DDL (WI-S04-002)

WI-S04-002 §1 Lote 10.4bis fix:
```sql
CHECK (sig_alg = 'hkdf-sha256')  -- chk_ac_sig_alg: v1 only; v2+ via ADR migration
```

This is correct in intent (reject the `hkdf-blake3` placeholder that Opus R4 flagged). But the CHECK constraint
now locks the schema to `hkdf-sha256` forever at the SQLite level. SQLite does not support `ALTER TABLE ...
DROP CONSTRAINT` or `ALTER TABLE ... MODIFY CONSTRAINT`. The 12-step recipe (CREATE new table, INSERT SELECT,
DROP old, RENAME) is the ONLY migration path — and WI-002 anti-scope explicitly says "❌ DROP TABLE in prod."

The "v2+ via ADR migration" comment implies there IS a migration path, but that path requires either:
1. SQLite 12-step recipe (which requires a DROP TABLE equivalent, violating anti-scope).
2. A dummy row that satisfies the new alg value and then a migration that fails the CHECK.

This is a soft lock-in. It's not wrong today (hkdf-sha256 is the right v1 alg), but the comment creates a
false promise of "v2+ migration possible" when the actual migration is painful.

**Fix**: Either (a) remove the comment "v2+ via ADR migration" and just say "v1 only; changing requires schema
rebuild per D1 SQLite 12-step recipe (documented in ADR-0036 §Migration)," or (b) change the CHECK to something
that doesn't need modification for v2: `CHECK (sig_alg IN ('hkdf-sha256'))` is functionally identical and no
better, but a comment like "extend this IN list via new migration 003b + ADR" is more operationally honest.

---

### P1-R5-014 — WI-S04-003 `result_hash` in `AcEnvelope` has no codec-level check that it equals `merkle_root` (WI-S04-003, WI-S04-002)

Per ADR-0037 (cited in WI-003 §2 narrative point 4): `result_hash = merkle_root` direct. This means the struct
`AcEnvelope` has both `merkle_root: [u8; 32]` and `result_hash` (implicitly in the schema; the AcEnvelope struct
in WI-003 §1 shows `merkle_root` but NOT `result_hash` as a distinct field).

The schema (WI-002) has `result_hash TEXT NOT NULL` — a separate DB column. But the `AcEnvelope` Rust struct
in WI-003 §1 only shows `merkle_root`. Where is `result_hash` in the struct? It must be derived from
`merkle_root` at serialization time (to populate the schema column), but this derivation is NOT in the
`AcEnvelope` definition nor in the canonical_bytes computation description in WI-004.

If `result_hash = merkle_root` (raw bytes, no hashing), then the schema column `result_hash TEXT NOT NULL —
64 hex chars = BLAKE3-256 of merkle_root` contradicts: BLAKE3-256(merkle_root) is NOT the same as merkle_root
hex. This is the same ambiguity found in P0-R5-003 but from the struct perspective.

**Fix**: Either add `result_hash: [u8; 32]` to the `AcEnvelope` struct (and assert at builder time that
`result_hash == merkle_root` or that `result_hash == BLAKE3(merkle_root)` per ADR-0037), or remove the
`result_hash` schema column and derive it on-the-fly at SELECT time (`hex(merkle_root) as result_hash`).
The current state is a latent struct-schema desync.

---

### P1-R5-015 — INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY is mentioned in WI-003 §2 but NOT added to invariant registry §3.15 (WI-S04-003, invariant_registry.md)

WI-S04-003 §2 narrative point 6: "INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY tier explicitly documented
(registry §3.15 + chaos test)."

I searched invariant_registry.md §3.15 (the Action Cache domain section promoted in Lote 10.4bis). The 17 INVs
listed in §3.15 do NOT include `INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY`. They include `INV-AC-EVICT-CONSISTENCY`
and `INV-AC-OUTPUTS-VALID` (from §3.3), but NOT the eventual-consistency qualifier for the outputs check.

This is a "promised but not delivered" invariant — exactly the same defect class Opus R4 flagged for the original
§3.15 absence. Lote 10.4bis created §3.15 with 17 entries, but missed this one.

**Fix**: Add `INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY` to §3.15 with description: "INV-AC-OUTPUTS-VALID
enforcement at handler time is point-in-time best-effort (TOCTOU window with S-06 GC tombstone); reconcile
diário (S-06) is the only mechanism for drift detection + correction; orphan_rate metric monitors."

---

### P1-R5-016 — WI-S04-006 sign-off table §30 has `SRE Lead (staffing waiver per ADR-0034)` already pre-waived but ADR-0034 itself is not in the ADR directory (WI-S04-006)

WI-S04-006 §30, §6.1.6, §8 Gherkin all reference ADR-0034 for the SRE Lead staffing waiver and the Crypto SME
advisory waiver. I searched the ADR directory: only ADR-0019 and ADR-0020 are present. ADR-0021 through ADR-0039
are referenced throughout S-04 WIs but only ADR-0019 and ADR-0020 exist in the file system.

ADR-0034 specifically governs: "staffing waiver for SRE Lead (solo-tier)" and is the authority for the Crypto
SME advisory-waiver path. If ADR-0034 does not exist, the waiver paths are undocumented and any PRR chair must
accept sign-off waivers on good faith alone.

This is not unique to ADR-0034 — ADR-0021 (HKDF decision), ADR-0035 (handler invariants), ADR-0036 (schema
migration), ADR-0037 (Merkle protocol) are also referenced but not present. Lote 10.4bis marked ADR-0021 as
"ratificada" but the file does not exist in `specs/03_architecture/adrs/`. The `validate_references.py`
whitelist presumably allows these forward references, but a real adversarial reviewer who wants to verify the
rationale hits a dead end.

**Fix**: This is a sprint-level structural gap. At minimum, ADR-0034 must exist before PRR (because it governs
the PRR itself). ADR-0021 must exist before WI-S04-004 SEAL (because it's the cripto decision document). The
other ADRs (0035-0039) can be created as lightweight decision records during implementation, but should be present
before WI SEAL. Add to WI-006 §7 Anti-scope: "❌ Ship with any referenced ADR in DRAFT or absent."

---

## 4. P2 Findings (Next Sprint / Doc-Only)

- **WI-S04-004 §22**: The `$4k/yr` cost at `10M verify/dia` uses per-request pricing, not CF Workers Unbound
  CPU pricing. Verify which plan CoreLink targets; if Unbound, the cost model changes. Provide sensitivity.
- **WI-S04-004 §2 narrative point 9**: "HKDF salt unspecified vs random" — still says `salt = None` in the
  narrative text even though the code shows `salt = sig_key_id.to_le_bytes()`. The Lote 10.4bis fix landed in
  §6.1 code but the narrative prose in §9.3 still says "Decisão: `salt = None`." Prose-code desync.
- **WI-S04-005 §22**: Per-eviction cost cited as `$0.000003 amortized over batch` but the breakdown derives
  `$0.000011` per row. Opus R4 caught this; not fixed in Lote 10.4bis.
- **WI-S04-001 §1**: `outputs_check::warn-if-missing` on GET changed to 1% sampled (Lote 10.4bis P0 fix).
  The sampling rate `1%` is arbitrary. At 10M GET/day × 1% = 100k sampled checks → 100k D1 SELECTs on blob_meta
  per day. The cost model in §22 should include this. Current §22 does not.
- **WI-S04-006 §1**: "ADR-0021 ratificada confirmação" is listed as a ship gate component. But ADR-0021 file
  does not exist in the ADR directory (see P1-R5-016). Gate confirmation would fail a file-existence check.
- **WI-S04-002**: `created_by_pat_id TEXT NULL` — Opus R4 noted this; not fixed. The sentinel value approach
  (`'__legacy__'`) is cleaner. Low priority but persistent doc smell.
- **All WIs §31 Change Log**: still 1 row each (no Lote 10.4bis amendment row added). The §31 change log
  should record the Lote 10.4bis pass explicitly for audit traceability.

---

## 5. Sonnet vs Opus Differential

### What Opus R4 caught that Sonnet confirms

- D1 ALTER TABLE ADD CONSTRAINT failure (P0; fixed in 10.4bis). Confirmed closed.
- `with_tenant_ctx!` on D1 false claim (P0; fixed in 10.4bis). Confirmed closed.
- `MerkleError::CycleDetected` missing variant (P0; fixed). Confirmed closed.
- Protobuf determinism (P0; fixed via ADR-0037 result_hash = merkle_root). Partially closed — residual
  ambiguity persists (see P0-R5-003).
- HKDF salt binding (P0; fixed via `salt = sig_key_id.to_le_bytes()`). Confirmed closed. New residual
  introduced (P0-R5-001: key_id=0 sentinel).
- BatchUpdateActionResult in REAPI (P0; fixed in prose but NOT in WI-006 Gherkin — P0-R5-004 is new finding).
- Crypto SME mandatory vs advisory contradiction (P0 per Opus; confirmed still present — P0-R5-005).
- BLAKE3 Merkle-Damgård mischaracterization (P0; fixed in prose). Confirmed closed.

### What Sonnet found that Opus missed

1. **P0-R5-001**: HKDF salt key_id=0 sentinel ambiguity — Opus fixed the salt but didn't note the key_id=0 edge.
2. **P0-R5-002**: TDK used for both raw HMAC (path) and HKDF-Extract (sig) without unified Extract step —
   formal domain separation incomplete. Opus noted "salt rationale weak" but did not trace the multi-derivation
   composition issue.
3. **P0-R5-003**: `canonical_bytes` result_hash binding is either redundant (if = merkle_root) or re-opens the
   protobuf determinism P0 (if = BLAKE3(proto_bytes)). The schema says one thing; the comment says another.
   Opus missed this contradiction.
4. **P0-R5-004**: WI-006 Gherkin §8 still says `BatchUpdateActionResult` after Lote 10.4bis "removal." Opus
   flagged the REAPI API name error in WI-001; did not verify WI-006 Gherkin after the fix.
5. **P0-R5-006**: `path_key_id` vs `sig_key_id` independent lifecycle — no rotation procedure for path TDK,
   no invariant for their relationship. Opus noted the path materialization fix but did not follow the key
   version lifecycle through to operational procedures.
6. **P1-R5-007**: Mann-Whitney arm contamination via length-mismatch fast-fail paths. Subtle methodology issue
   that Opus' timing methodology section didn't catch.
7. **P1-R5-009**: Conformance test set of 10 is critically undersized for a "100% pass" ship gate. Opus asked
   about which test binary and which subset but didn't push back on the size once the 10-test list was produced.
8. **P1-R5-013**: CHECK constraint locks sig_alg forever with a false "v2+ migration possible" comment.
9. **P1-R5-014**: `result_hash` field missing from `AcEnvelope` Rust struct while present in D1 schema.
10. **P1-R5-015**: `INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY` promised in WI-003 §2 but missing from
    invariant registry §3.15 despite the Lote 10.4bis promotion.
11. **P1-R5-016**: ADR-0034 (and ADR-0021/0035/0036/0037) all referenced but none exist in the file system.

### Assessment of differential value

Opus R4 correctly identified the structural P0 class (SQL syntax, Postgres-vs-D1 confusion, cross-doc drift,
sign-off governance, missing enum variants). Sonnet R5 provides additive value primarily in:

- **Cripto composition precision**: the HKDF multi-derivation issue (P0-R5-002) and the salt/key_id edge
  (P0-R5-001) require analyzing the system as a whole rather than each primitive in isolation. These are
  typical model-of-model issues that require holding multiple contexts simultaneously.
- **Cross-WI desync that survives Lote 10.4bis**: the Gherkin residual (P0-R5-004), result_hash ambiguity
  (P0-R5-003), and missing ADR files (P1-R5-016) are post-patch residuals that only surface by re-reading
  the documents after the fixes were nominally applied.
- **Statistical methodology subtleties**: the Mann-Whitney arm contamination (P1-R5-007) is a methodological
  precision issue, not a structural one — different kind of attention required.

---

## 6. Final Scoring (Sonnet R5 Post-Lote-10.4bis)

| WI | Score | Dominant weakness |
|---|---|---|
| WI-S04-001 | **7.9** | Sign-flow race (P1-R5-011); outputs_check cost not in §22 |
| WI-S04-002 | **7.8** | path_key_id/sig_key_id lifecycle (P0-R5-006); sig_alg lock-in (P1-R5-013); result_hash struct gap (P1-R5-014) |
| WI-S04-003 | **8.7** | verify_full ordering (P1-R5-008); INV-AC-OUTPUTS-VALID-EVENTUAL missing (P1-R5-015) |
| WI-S04-004 | **7.8** | HKDF composition (P0-R5-002); key_id=0 sentinel (P0-R5-001); prose desync on salt (P2) |
| WI-S04-005 | **8.1** | Alarm re-arm race (P1-R5-010); cost math persists wrong |
| WI-S04-006 | **7.7** | Crypto SME contradiction (P0-R5-005); Gherkin residual (P0-R5-004); ADR files missing (P1-R5-016); 10-test conformance set (P1-R5-009) |

**Sprint average (Sonnet R5): 8.0/10**

---

## 7. Verdict

**CONDITIONAL GO — requires Lote 10.4ter P0 pass before any WI SEAL.**

The Lote 10.4bis patch closed the most dangerous structural defects (SQL, wrong primitives, missing enum
variants, protobuf determinism, salt binding). The program is clearly trending upward. But:

Six residual P0s remain. Three are cripto-load-bearing (R5-001, R5-002, R5-003) and require Crypto SME
attention. Two are governance (R5-004 trivial fix, R5-005 requires WI text change). One is operational
(R5-006 key lifecycle gap).

**Non-negotiable before SEAL**:
1. Resolve the `result_hash` = what exactly (P0-R5-003). This decision propagates to canonical_bytes layout,
   the D1 schema column, and the `AcEnvelope` struct. One answer must be chosen and documented in ADR-0037.
2. Crypto SME must review P0-R5-002 (TDK multi-derivation composition) and confirm acceptability or prescribe
   the HKDF-Extract unification. This is precisely the kind of subtlety the Crypto SME is listed as mandatory
   to catch.
3. Strike `BatchUpdateActionResult` from WI-006 §8 Gherkin (P0-R5-004). One-line fix.
4. Resolve Crypto SME mandatory vs advisory contradiction (P0-R5-005) with explicit text in both WIs.
5. Reserve key_id=0 as sentinel and fix `unwrap_or(&0)` (P0-R5-001).
6. Document `path_key_id` rotation procedure (P0-R5-006).

**Program SOTA bar**: WI-S04-003 at 8.7 is now the strongest WI the program has produced. The Merkle dual-side
design, bounded parser, Bao-tree correctness, and cycle detection combine into a genuinely SOTA library spec.
The rest of the sprint is being dragged by cripto-precision gaps in WI-004 and governance gaps in WI-006.
Resolving the 6 P0 residuals and addressing the key P1s would lift the sprint average to approximately 8.4,
which would be comfortably above the S-03 baseline.

---

*End of AUDIT-SONNET-R5-S04-WI-REVIEW.*
