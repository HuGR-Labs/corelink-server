---
id: "RB-FM-062"
type: "runbook"
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
tags: ["runbook", "p1", "cryptography", "cas"]
---

# RB-FM-062 — CAS/AC Hash Collision (Criptográfico BLAKE3)

> **FM:** FM-062 (S=5, RPN=25, P1 S=5→upgrade) | **CTRLs:** CTRL-CAS-001, CTRL-CAS-002 | **SLA:** containment < 4h
>
> **Probabilidade:** ~2^-128 para BLAKE3 (256 bits). Se observada = falha de RNG, bug de implementação, OU breakthrough criptográfico (academic). Todas as alternativas são emergência.

## Detecção

- Write com `hash(body) == existing_digest` mas `body != r2_storage[digest]`.
- Métrica `corelink_cas_collision_detected_total > 0` (alert SEV-1 imediato).
- Cliente reporta digest "igual" com binários diferentes (raríssimo — verificar antes).

## Comunicação

- **SEV-1.** Page Security Lead + Architect + Cryptography Reviewer + CEO.
- Status page: `major` (integridade do CAS em questão).
- Contact NIST / CERT se confirmada collision real (seria news, não incidente).

## Triage (≤ 30 min)

### Cenário A — Falha de RNG / bug de implementação (provável)

1. `cargo-audit` recente? Check advisories para `blake3`.
2. Replicar em staging com mesmo input. Se reproduzir sempre: bug determinístico → fix.
3. Se não reproduzir: RNG instability? Check build env.

### Cenário B — Corrupção durante armazenamento (bit rot)

1. Readahead: recomputar hash do body em R2. Se hash ≠ digest: é bit rot (FM-051, ver RB-FM-051).
2. NÃO é collision real — é corruption. Redirecione.

### Cenário C — Collision criptográfica real (extremamente improvável)

1. Reproduzir com input capturado + body real.
2. Se reproduz em múltiplos runs (determinístico): academic find.
3. Publish bounty + coordenar com BLAKE3 maintainers.

## Mitigação imediata

1. Quarantine ambos os blobs envolvidos: marcar em `blob_meta.quarantined_at`.
2. Reads retornam 503 `Retry-After: 3600` para esses digests específicos.
3. Customer notification: "digest <...> temporariamente indisponível para investigation".

## Mitigação completa

### Se Cenário A (bug implementação)

1. Fix bug, bump versão `blake3` se upstream.
2. Re-hash todos os blobs afetados (idealmente nenhum, mas recomputar).
3. Scrub target: prefixo impactado.

### Se Cenário C (collision real)

1. **Defense-in-depth**: habilitar dupla hash (BLAKE3 + SHA-256) nos novos writes.
2. Migrate storage layout para incluir ambos digests.
3. Re-hash progressivo com `fallback_algo = sha256`.
4. ADR obrigatório: eventual migration para novo primary hash.

## Forensics

1. Preservar ambos bodies + digest calculation chain.
2. Git blame para libs `blake3`, `sha2`.
3. SBOM da release.
4. Build env details (compiler, target, CPU flags).

## Notificação

- Customers afetados: notificação em ≤ 24h.
- Se collision real: security bulletin + CVE reservation.

## Prevenção

- CTRL-CAS-002: client-side verify sempre enabled.
- Scrub semanal de 1% blobs; mensal 100% hot.
- Monitor `blake3` CVE feed.
- Future: dupla hash opcional para enterprise tier.
