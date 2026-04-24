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

---

## Specs (v2 pós Lote 6.1 — endereçando audit G-01/G-02/G-03)

| Spec file | Invariante(s) | States checked | Status |
|---|---|---|---|
| `tenant_isolation.tla` + `.cfg` | InvTenantIsolationRead + Write + Enum + InvPrefixInjective + InvNamespaceConsistency. v3 (Lote 7.1): defense-in-depth explícito — Write recebe target_tenant + DefectiveWrite adversarial com guard `defenses_active`. | ~390k states | ✅ verde |
| `gc_correctness.tla` + `.cfg` | **InvGCReachableNeverDeleted** (core INV-GC-001) + **InvGCReRefProtected** (INV-GC-004) + InvMarkingConsistent. v3 (Lote 7.1): ambos invariantes agora no cfg; Mark multi-pass com UpdateActionResult interleaved. | ~170k states | ✅ verde |
| `cas_integrity.tla` + `.cfg` | InvCASIntegrityUncorrupted + InvClientVerifyIsSound + InvCASImmutability + InvPoisoningRejected. v2 (Lote 6.1): BitRot muta r2_storage body real (não flag). | ~3k states | ✅ verde |
| `audit_immutability.tla` + `.cfg` | InvAuditAppendOnly + InvAuditChainIntact + InvAuditOrderPreserved + InvAuditRejectTamper + **InvDefenseInDepth**. v2 (Lote 7.1): adversary agora REALMENTE muta audit_log quando `object_lock_on=FALSE ∧ db_constraint_on=FALSE` (defense-in-depth explícito). | ~5k states | ✅ verde |

## Como rodar

```bash
cd specs/tla
tlc -config tenant_isolation.cfg   tenant_isolation.tla
tlc -config gc_correctness.cfg     gc_correctness.tla
tlc -config cas_integrity.cfg      cas_integrity.tla
tlc -config audit_immutability.cfg audit_immutability.tla
```

Requer TLA+ Toolbox ou `tlc` CLI (https://github.com/tlaplus/tlaplus).

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
