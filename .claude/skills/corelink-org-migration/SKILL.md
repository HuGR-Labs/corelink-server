---
name: corelink-org-migration
version: 2.0.0
description: Preparar auditoria, desenho e verificação de uma mudança de organização GitHub apenas para o repo ID 1232040291/corelink-server. Não autoriza nem executa transferência.
---

# Migração de organização do CoreLink server

## Escopo e início

Usar para auditar, planejar, preparar, ensaiar ou verificar a mudança do repositório `corelink-server` (ID `1232040291`). Ler primeiro `CLAUDE.md`, [índice](../../../docs/internal/org-migration/README.md) e contrato de gates.

Esta frente muda ownership GitHub somente. Runners, Workspaces e CLI são peers separados. Não mover App, cloud, DNS, Workers, D1/R2, billing, tenants ou outros repositórios por inferência. A issue #1702 é guarda-chuva; B-374 acompanha o kit e permanece OPEN até aceite próprio.

## Modos

| Modo | Entradas | Ações | Hard stop / saída |
| --- | --- | --- | --- |
| audit | Commit SHA e perfil | Ler Git tree; revisar findings/opacos | Partial, owner desconhecido ou consumer unresolved permanece bloqueado; relatório com limites. |
| plan | Perfil; destino pode ser null | Gerar plano offline | Sempre `PLANNING_ONLY_NOT_AUTHORIZED`, `migration_ready:false`. |
| prepare | Censo, owners e política | Criar docs/tests/projeções em branch revisada | Só allowlist; produzir PR e evidências, sem aplicar em produção. |
| rehearse | Ambiente descartável, limite e cleanup | Ensaio positivo/negativo isolado | Sem publicação/deploy, dado de cliente ou writer compartilhado. |
| cutover | G00–G11 vigentes e GO G12 explícito | Procedimento humano externo com uma request | Não existe comando de corte aqui; UNKNOWN/timeout bloqueia e não repete. |
| verify | Readback por ID, refs e observação | Provar gates G13–G19 | Não pré-aprovar; registrar evidência posterior ao evento. |

## Ferramentas

`python3 scripts/org_migration_audit.py scan --revision <FULL_SHA>` lê somente o commit, sem rede/working tree/hooks. `plan` permite perfil sem destino; `compare` é sempre REVIEW_REQUIRED. O scanner não aprova finding nem decide semântica.

`python3 scripts/org_migration_gate_check.py docs/internal/org-migration/gate-ledger.template.json` valida schema 2. Template desconhecido deve bloquear. Mesmo exit 0 significa apenas coerência declarada; não é autenticação ou permissão para transferir.

Testes focais: `python3 scripts/test_org_migration_audit.py -v` e `python3 scripts/test_org_migration_gate_check.py -v`.

## Safety contract

Preparação permite somente documentação, relatório offline, fixture isolada, atualização de perfil em branch revisada e PR preparatório. Proibidos: transfer, mirror push, rewrite de histórico/assinaturas, deploy/publish, dispatch produtivo, restart de fleet amplo, mudanças cloud/tenant, rename/visibility change, mover peers, rotação ampla ou `git clean`/`reset --hard`.

Gates G00–G19 começam UNKNOWN. Evidência vence por idade/scope; `403`, `404`, 202, redirect, busca vazia ou checklist não demonstram ausência, identidade nem GO. N/A só em G07/G17 com prova e reviewer. G12 é emitido depois do freeze final, para hash/IDs/janela exatos e janela atual; G13–G19 são posteriores ao corte.

Exigir source write-frozen/destination prep-only, drain, FREEZE_SHA, paridade backup/restore/refs/assets/LFS, evidência privada sem secrets/JWT/tenant data, recuperação para frente independente, credential rotation scoped e single-writer reconciliado. Timeout depois da única request é UNKNOWN; contenha e leia, sem retry. Transfer-back não é presumido.

G19 exige inventário/reconciliação de grants temporários human/collaborator/team/app/token; grants obsoletos de source/destination revogados com probes negativos; leitores retidos individualmente com owner, motivo, expiry futuro e reviewer; nenhum valor secreto; e remoção do source bridge confirmada por readback `OBSERVED_ABSENT` do repo ID correto.

## Referências

- [Mapa e fronteiras](../../../docs/internal/org-migration/MAP.md)
- [Desenho](../../../docs/internal/org-migration/DESIGN.md)
- [Runbook](../../../docs/internal/org-migration/RUNBOOK.md)
- [Contrato operacional](../../../docs/internal/org-migration/OPERATION-CONTRACT.md)
- [Work packages](../../../docs/internal/org-migration/WORKPACKAGES.md)
- [Fontes](../../../docs/internal/org-migration/SOURCES.md)
- [Revisão adversarial](../../../docs/internal/org-migration/ADVERSARIAL-REVIEW.md)
- [Revisão e limites](../../../docs/internal/org-migration/REVIEW.md)

**Stop** em identidade divergente, gate UNKNOWN, evidência inválida/vencida, writer residual, restore/parity quebrada, dados sensíveis expostos, split-brain, scope desconhecido ou autorização fora da janela. Registrar estado/owner e pedir a autoridade da mudança para nova decisão.
