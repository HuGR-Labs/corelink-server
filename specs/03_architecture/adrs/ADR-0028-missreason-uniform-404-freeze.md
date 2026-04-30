---
id: "ADR-0028"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-29"
updated: "2026-04-29"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "missreason", "404-uniform", "side-channel", "rest-semantics", "reapi", "s02", "s06-deferral"]
---

# ADR-0028 — MissReason → HTTP 404 Uniform Freeze (S-02 GA); 410 Gone Deferred S-06

## Status

FROZEN (S-02 WI-S02-005 ratificada em Lote 10.2bis cycle 7+ SEAL ship gate; supersedes earlier "404 vs 403 distinction" model em WI-S02-001/002/004 narratives).

## Context

S-02 read path resolves a request com várias possible "miss" semantic outcomes. **Taxonomy (canonical) vs runtime read-side enum (impl reality)**:

**Canonical taxonomy variants** (per ADR-0028 v1.0.0 model; this is the policy schema):

| MissReason (taxonomy) | Cause | Detection path |
|---|---|---|
| `NotFound` | Digest never existed in tenant scope | KV negative cache hit OR D1 row absent |
| `CrossTenantMasked` | Digest exists em outro tenant; AuthZ check returns row count 0 | D1 query + AuthZ reject + audit emit `corelink.cas.read_miss` (read-side, low-severity, conflated com `NotFound` por design — read-handler não pode distinguir sem side-channel oracle); reclassificação para `corelink.cas.cross_tenant_attempt` (SEV-1) acontece offline pelo S-09 chain consumer com global digest index. Ver WI-S02-001 v1.1.0 §31. |
| `Tombstoned` | Digest had existed but was soft-deleted (S-06 GC) | D1 row found com `deleted_at IS NOT NULL` |

**Runtime read-side enum** (per WI-S02-001 v1.1.0 implementation; `crate::corelink_reapi::read::MissReason`):

```rust
enum MissReason {
    NeverExisted,    // folds taxonomy { NotFound, CrossTenantMasked }
    Tombstoned,      // 1:1 with taxonomy
    R2OrphanRow,     // new variant — row alive in D1 but R2 GET miss
}
```

The fold (`NotFound` + `CrossTenantMasked` → `NeverExisted`) is structural: the read seam queries `MetaStore::get((tenant_id, digest))` which returns `Ok(None)` for both arms — no side-channel oracle exists at the read seam to distinguish them. The S-09 chain consumer uses the offline global digest index to reclassify `NeverExisted` rows into the canonical `CrossTenantMasked` taxonomy variant when it confirms cross-tenant ownership; the SEV-1 `corelink.cas.cross_tenant_attempt` event fires from there. The `R2OrphanRow` variant is new in v1.1.0 — the prior taxonomy did not enumerate the orphan-window-during-GC case explicitly; it surfaces as a uniform 404 still (per the freeze) but emits `corelink.cas.r2_orphan_detected` (SEV-2) for SRE GC-reconcile triage.

**Original S-02 spec (pre-cycle 7)** mapped these to different HTTP status codes:
- `NotFound` → 404
- `CrossTenantMasked` → 403 (forbidden) com `COR_CAS_TENANT_FORBIDDEN`
- `Tombstoned` → 404 OR 410 Gone (TBD)

This created **two architectural problems**:

### Problem 1: Status-code differential = enumeration oracle

Atacante autentica em Tenant B; probe candidate digests pertencentes a Tenant A:
- 404 response → digest doesn't exist anywhere (negative info; useful but limited)
- 403 response → digest exists em outro tenant (HIGH-VALUE info; enumeration successful)

The 403 status code itself reveals existence, even sem timing analysis. INV-TENANT-ISOLATION violated indirectly (existence oracle).

### Problem 2: 410 Gone reveals prior existence

If `Tombstoned` returned 410 Gone, atacante distingue:
- 404 → never existed
- 410 → existed previously (now deleted)

This discloses customer behavior (which digests they had cached). Privacy violation independent of cross-tenant concern.

### Problem 3: REAPI v2 conformance

`bazelbuild/remote-apis` test suite assumes 404 para missing blobs uniformly. 410 Gone é não-canonical em REAPI semantics; conformance test fails.

## Decision

**Todos os MissReason variants retornam HTTP 404 uniform com `error_code = COR_CAS_BLOB_NOT_FOUND` + same response body**:

```
HTTP/1.1 404 Not Found
Content-Type: application/grpc
Grpc-Status: 5 (NOT_FOUND)

{
  "error_code": "COR_CAS_BLOB_NOT_FOUND",
  "message": "Blob with digest <X> not found"
}
```

- Atacante NÃO distingue NotFound vs CrossTenantMasked vs Tombstoned via response.
- Audit trail (S-09 chain) retém **forensic reason** distinctly via per-arm CE event types:
  - `corelink.cas.read_miss` (NotFound + read-side CrossTenantMasked, conflated)
  - `corelink.cas.cross_tenant_attempt` (CrossTenantMasked confirmed by S-09 offline using global digest index — SEV-1 reclassification)
  - `corelink.cas.tombstoned_read_attempt` (Tombstoned)
  - `corelink.cas.r2_orphan_detected` (R2 GET miss after AuthZ pass — orphan window inside GC reconcile lag)
- Side-channel timing parity enforced via ADR-0023 (constant-time middleware; 3-arm methodology).

**410 Gone semantic deferred to S-06 GC sprint** com explicit ADR + privacy review + REAPI conformance re-validation. Não é S-02 GA scope.

**`COR_CAS_TENANT_FORBIDDEN` (403)** continua existindo em error_taxonomy.md mas **NÃO é retornado por CAS read handlers** — reservado para PAT scope failures (S-03 auth); CAS handler returns 404 uniform on cross-tenant.

## Consequences

### Positive

- **INV-TENANT-ISOLATION existence oracle fechado** at status-code layer.
- **Privacy preserved**: atacante não infere customer behavior via 410 Gone.
- **REAPI v2 conformance maintained** (404 canonical for missing blobs).
- **Single-source canonical**: WI-S02-001 + WI-S02-002 + WI-S02-004 + WI-S02-005 + WI-S02-006 all aligned.

### Negative

- **Forensic visibility**: customer support cannot easily distinguish "your blob was tombstoned by GC" from "you're querying wrong digest" via API response. Mitigation: audit log + admin API S-13 expose forensic reason to authorized roles.
- **Cliente DX**: developer debugging "why is my blob 404" needs admin tools (not API response). Acceptable trade-off para security.
- **Side-channel deferred to timing layer**: status-code uniform alone insuficient — ADR-0023 timing-padding middleware required to close timing oracle.

### Trade-offs rejected

1. **Differentiated status codes** (404/403/410): enumeration + privacy oracle; rejected.
2. **Same status code different error_code**: error_code é response body; same enumeration concern; rejected.
3. **Different response body sizes**: timing/size oracle; rejected.

## Alternatives considered

1. **404 + error hint via header (`X-CoreLink-Miss-Reason`)**: header reveals existence; rejected (same oracle as status).
2. **403 for CrossTenantMasked, 404 for others**: original spec; enumeration oracle; rejected.
3. **410 Gone for Tombstoned**: prior-existence oracle; deferred to S-06 GC sprint com explicit ADR.
4. **Random response shuffling**: not robust against statistical attack; rejected.

## Compliance

- **CTRL-ISO-002** (AuthZ check; `security_model.md §6.3`): mismatch returns 404 (não 403; updated cycle 7).
- **CTRL-ISO-004** (constant-time response; `security_model.md §6.3`): timing parity enforced via ADR-0023.
- **INV-TENANT-ISOLATION** (invariant_registry.md §3.X): existence oracle fechado at status + timing layers.
- **INV-CAS-SIDE-CHANNEL-INDISTINGUISHABLE** (invariant_registry.md §3.12): aligned com 3-arm methodology per ADR-0023.
- **REAPI v2** (`bazelbuild/remote-apis`): 404 canonical for missing blobs.
- **error_taxonomy.md §3.1 CAS errors**: COR_CAS_BLOB_NOT_FOUND covers NotFound + CrossTenantMasked + Tombstoned uniform; COR_CAS_TENANT_FORBIDDEN reserved para PAT scope failures (não cross-tenant blob read).

## Implementation evidence

- WI-S02-001 §1 narrative + §6.1.4 + §6.1.6 + §8 AC (cross-tenant scenario): row count 0 → 404 uniform; tombstone → 404.
- WI-S02-002 §1 + §3 + §8 AC: cross-tenant batch returns "missing" (não distinguishable from truly missing).
- WI-S02-005 §6.1.5 + §6.1.6 + §9.10: GA freeze 404 uniform; MissReason enum forward-future-ready mas all map to 404.
- WI-S02-006 §2 narrative + §8 AC: property test asserts 100% cross-tenant reads return 404 (NUNCA 403).

## Migration path

- **S-02 GA**: ADR-0028 freeze; 404 uniform.
- **S-06 GC sprint**: revisit 410 Gone semantic com explicit ADR + privacy review + REAPI conformance re-validation; only if customer feedback demands it.
- **Forensic API S-13**: admin endpoint exposes MissReason to authorized roles (Compliance, Privacy, Security) for debugging without revealing to standard cliente.

## Changelog

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação ADR-0028. 404 uniform freeze; 410 Gone deferred S-06. |
| 1.1.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | WI-S02-001 implementation alignment per codex round-4 P2 finding: read-side handler emits `corelink.cas.read_miss` (low-severity, info) for the conflated `NeverExisted/CrossTenantMasked` arm because the trait-level `MetaStore::get` cannot distinguish without a side-channel oracle; the SEV-1 `corelink.cas.cross_tenant_attempt` event is reserved for the offline S-09 chain consumer that has access to the global digest index. Tombstone event_type literal corrected to `corelink.cas.tombstoned_read_attempt` (matches `crate::audit::REAPI_TOMBSTONED_READ_ATTEMPT`). Added `corelink.cas.r2_orphan_detected` for the R2 GET miss after AuthZ pass arm. |

---

**Fim ADR-0028.**
