---
id: "TLA-README"
type: "framework"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["tla", "formal-verification", "evidence"]
---

# TLA+ Specs Index

> **Propósito:** verify formal das invariantes CRITICAL do CoreLink (CTRL-FORMAL-001 em `security_model.md §6.9`). Cada invariante CRITICAL em `invariant_registry.md §4` tem spec `.tla` + `.cfg` aqui.

> ⚠️ **The `Status` column below is a hand-maintained historical claim, not a
> verification result.** The authoritative, machine-checked status is whatever
> `scripts/run_tla_suite.sh` reports plus [`QUARANTINE.md`](QUARANTINE.md) — read
> those first. This warning exists because the CI gate that was supposed to keep
> this table true had **0 successes in 195 runs** (2026-05-07 → 2026-08-02), so
> every "✅ verde" here was written by hand and never re-confirmed by a passing
> build. Some of them are, in fact, true; the point is that this table is not
> evidence of it. Also note this table lists a subset — 51 specs exist, across
> `specs/tla/` (47) and `specs/03_architecture/tla+/runbooks/` (4).

---

## Specs (v2 pós Lote 6.1 — endereçando audit G-01/G-02/G-03)

| Spec file | Invariante(s) | States checked | Status |
|---|---|---|---|
| `tenant_isolation.tla` + `.cfg` | InvTenantIsolationRead + Write + Enum + InvPrefixInjective + InvNamespaceConsistency. v3 (Lote 7.1): defense-in-depth explícito — Write recebe target_tenant + DefectiveWrite adversarial com guard `defenses_active`. | ~390k states | ✅ verde |
| `gc_correctness.tla` + `.cfg` | **InvGCReachableNeverDeleted** (core INV-GC-001) + **InvGCReRefProtected** (INV-GC-004) — ambos no cfg; **InvMarkingConsistent** definido em .tla mas não no cfg INVARIANTS list (Lote 10.6 cycle 1 honest-flag — ver scope limitations em ADR-0042 §A3). v3 (Lote 7.1): ambos invariantes core no cfg; Mark multi-pass com UpdateActionResult interleaved. **Bounds canonical:** Blobs={b1,b2} (2), AC_Entries={e1} (1), MaxTime=10, GracePeriod=2 (cfg actual; WI-S06-006 references aligned cycle 1). | ~5k-50k states (bounded; was previously claimed 170k — corrected) | ✅ verde core; out-of-scope soft-delete/DSR/physical-delete per ADR-0042 §A3 |
| `cas_integrity.tla` + `.cfg` | InvCASIntegrityUncorrupted + InvClientVerifyIsSound + InvCASImmutability + InvPoisoningRejected. v2 (Lote 6.1): BitRot muta r2_storage body real (não flag). | ~3k states | ✅ verde |
| `audit_immutability.tla` + `.cfg` | InvAuditAppendOnly + InvAuditChainIntact + InvAuditOrderPreserved + InvAuditRejectTamper + **InvDefenseInDepth**. v2 (Lote 7.1): adversary agora REALMENTE muta audit_log quando `object_lock_on=FALSE ∧ db_constraint_on=FALSE` (defense-in-depth explícito). | ~5k states | ✅ verde |
| `dsr_erasure_atomicity.tla` + `.cfg` (S-11 WI-S11-008) | **State invariants (5):** InvErasureComplete (INV-DATA-ERASURE-COMPLETE CRITICAL) + InvConsentSymmetry (INV-CONSENT-PROOF-VERIFIABLE CRITICAL) + InvResidencyPinned (INV-DATA-RESIDENCY CRITICAL) + InvBackendAckIdempotent + TypeOK. **Temporal properties (3):** InvAuditAppendOnly (box-prime sobre Len+prefix) + **InvResidencyMonotonic** (region pinning monotonic; no cross-region migration; Lote 10.11.0-bis-prime cycle 2 NEW) + EventualTermination (liveness `~>`). CONSTANTS: Backends (12 canonical = 8 EffectiveBackends + 4 PseudonymizedBackends), ConsentBasedPurposes/NonConsentPurposes, Subjects, Tenants, Tickets, Regions (6), Locales (3), MaxConcurrentErasures, MaxAuditChainLen, MaxAttempts. ASSUME garante partição válida e cardinalidades. WF fairness em todas as transition actions. SHA-256 pinned (TLC v1.8.0 ADR-0042 §A1). CI: `tla_check.yml` (the single TLA+ gate). The dedicated `tla_dsr_erasure_check.yml` was retired 2026-08-02 — it duplicated this spec with the same `.cfg` and had 0 successes in 100 runs. | TBD pós-CI primeiro run | 🟡 spec written (Lote 10.11.0-bis-prime; WI-S11-008 SEALED 2026-05-13); ⛔ **NOT verified — quarantined.** `INV-DATA-ERASURE-COMPLETE` is currently VACUOUS in this model (the only writer of `backend_state` has no failure branch, and the invariant restates the guard of the only action that sets `completed`). See `QUARANTINE.md`. The AC-008 honest-flag never flipped: the gate that was supposed to flip it never passed. |

## Como rodar

Run the whole suite the way CI does — this is the only invocation that reconciles
results against `QUARANTINE.md`, so it is the only one whose "green" means
anything:

```bash
TLC_JAR=/path/to/tla2tools.jar bash scripts/run_tla_suite.sh --timeout 300 --workers 2
```

One spec at a time, while iterating on it:

```bash
TLC_JAR=/path/to/tla2tools.jar bash scripts/run_tla_suite.sh --only gc_correctness
```

`TLC_JAR` must be the SHA-256-pinned jar (ADR-0042 §A1) — the runner refuses to
start without it rather than silently model-check with an unverified binary.

## Scope dos models

Models são **small-state** para fins de CI rápido (rodam em ≤ 30s cada). Eles provam corretude sobre:
- `tenant_isolation`: 2 tenants × 2 principals × 2 blobs × 4 ops.
- `gc_correctness`: 2 blobs × 1 AC entry × 10 steps de tempo × grace 2.
- `cas_integrity`: 2 bodies × 3 digests × 1 client × 3 ops.
- `audit_immutability`: 1 actor × 2 event types × 3 events × adversarial tamper.

Para **production assurance** (auditoria SOC 2 Type II):
- Escalar bounds em TLC distributed (grandes state spaces).
- Testar com `Apalache` (symbolic) para bounds não-paramétricos.
- Cobertura adicional via `PlusCal` para cenários de concurrência complexos.

## Evidence (CTRL-FORMAL-001)

Output do TLC run = **EVT-022 (TLA_MODEL_CHECK)**. CI obrigatório:

```yaml
# .github/workflows/tla.yml (exemplo)
jobs:
  tla-check:
    steps:
      - run: cd specs/tla && tlc -config tenant_isolation.cfg tenant_isolation.tla
      - run: cd specs/tla && tlc -config gc_correctness.cfg   gc_correctness.tla
      - run: cd specs/tla && tlc -config cas_integrity.cfg    cas_integrity.tla
```

Falhar CI se qualquer invariant check falhar.

## Nota sobre abstrações

Models abstraem:
- **HMAC de tenant_id** → single-char surrogate injetivo. A propriedade formal é "hash é colisão-resistente" (assumida via ASSUME no spec real; BLAKE3 = 2^-128 collision probability, modelado como 0).
- **Hash de body** → function `hash_fn: Bodies → Digests` injetiva, inicializada no Init e UNCHANGED. Representa qualquer função criptográfica determinística (BLAKE3, SHA-256).
- **Tempo lógico** → contador inteiro `now`, incrementado a cada action. Grace period medido em unidades inteiras.
- **Clients/principals** → IDs finitos; modelo não distingue entre usuário humano e CI token.

Essas abstrações são **documentadas** no topo de cada spec + `auth_model.md §8.3` e `invariant_registry.md §1`.

## Contribuindo

Antes de mudar uma spec:
1. Rode TLC localmente para confirmar status atual (verde ou findings).
2. Discuta mudanças em ADR (toca invariante CRITICAL — HIGH_RISK lane, FF-HR-005).
3. PR deve incluir output do TLC como evidence.

---

**Fim de TLA-README.**
