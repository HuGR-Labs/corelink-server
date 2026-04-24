---
id: "RB-FM-156"
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
tags: ["runbook", "p1", "supply-chain", "security"]
---

# RB-FM-156 — Dep com Maintainer Malicioso (Supply Chain Attack)

> **FM:** FM-156 (S=5, RPN=25, P1 S=5→upgrade) | **CTRLs:** CTRL-SUPPLY-001..005 | **SLA:** contain < 4h
>
> **Contexto:** supply chain attack em dep transitiva (ex: `event-stream` 2018, `colors.js` 2022, `node-ipc` 2022). Dep que parecia benigna passa a exfiltrar/sabotar em versão nova.

## Detecção

- `cargo-audit` alerta para versão nova + advisory HIGH/CRITICAL.
- GitHub security advisory fly-in na UI.
- Twitter/X news sobre compromisso.
- Customer reporta comportamento inesperado (rare).
- SAST/Runtime monitoring detecta connection suspeita do Worker/Container.

## Comunicação

- **SEV-1** (potencial comprometimento total).
- Page Security Lead + Architect + CEO.
- Status page IMEDIATO se exploit ativo: "investigating dependency compromise".

## Triage (≤ 1h)

1. Qual dep? Qual versão? Desde quando está no lockfile?
2. Cross-check commits da dep: quando mudou maintainer? Qual foi o diff?
3. Check SBOM de nossos releases: quais versões shipped com dep malicioso?
4. Runtime monitor: alguma connection suspeita / exfil observada em logs?

## Mitigação imediata

1. **Revert dep** para versão anterior safe.
2. Rebuild + deploy via PAT-PROGRESSIVE-ROLLOUT-001 (1% → 10% → 100%).
3. Se já deployed: avaliar impact:
   - Credentials leaked? Rotação imediata (CTRL-CRED-004).
   - Tenant data leaked? Trigger RB-BREACH-NOTIF.
   - Code integrity comprometido? Rotate signing keys.

## Mitigação completa (≤ 4h)

1. Fork/patch dep se maintainer comprometido definitivamente (publish patched crate interno).
2. Longer-term: substitute dep por alternativa menos risky.
3. SBOM re-published.
4. Cosign re-sign releases com nova build.

## Forensics

1. Diff de código da dep entre versões safe e maliciosa.
2. Análise de connect out logs: exfil foi observada? Pra onde?
3. Audit log: atacante conseguiu ação em nome de tenant?
4. Preservar evidence para possível disclosure/report.

## Notificação

- Customers afetados se exploit foi executado: ≤ 48h.
- Security bulletin público em ≤ 14d.
- Coordenar com CISA / CERT se breadth é wide.
- Contribute writeup to Rust ecosystem security (maintainer lifecycle best practice).

## Prevenção

- CTRL-SUPPLY-001 SLSA L3 (não previne tudo, mas provenance).
- CTRL-SUPPLY-003 SBOM (traceability).
- CTRL-SUPPLY-004 pinning + cargo-audit diário.
- CTRL-SUPPLY-005 no dynamic loading.
- Review maintainer de deps críticas quando adicionar: "quem é esse?" teste.
- Dependency minimization: menos deps = menor superfície.
- `cargo-deny` com `allow-git = []` (só registry oficial).
