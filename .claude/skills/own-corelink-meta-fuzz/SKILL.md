---
name: own-corelink-meta-fuzz
description: Ownership routing for corelink-meta-fuzz; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-meta-fuzz
  manifest: crates/corelink-meta/fuzz/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-meta-fuzz-structural-normalization-20260921
---

# Ownership — corelink-meta-fuzz

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Alterar commit_put_roundtrip, audit_idempotency, seus inputs ou assertions. | Alterar a implementação MetaStore, schema/migration D1, worker/R2 wiring ou operação persistente; encaminhe ao owner desses limites. |
| Avaliar o job de fuzz que seleciona estes targets. | Afirmar que targets declarados foram executados ou que seus resultados alcançam produção sem run evidence. |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação:** os dois arquivos em crates/corelink-meta/fuzz/fuzz_targets/ e seu manifesto.
**Contratos verificados:** os oráculos dos targets; APIs de MetaStore/MetaError pertencem a corelink-meta e Digest pertence a corelink-hash.
**Fora do território:** D1, schema persistido, adapter Cloudflare, R2 e composition root em corelink-worker.
**Papéis:** implementation owner é o package deste harness; contract owners são os packages provedores. Pessoa/time responsável, operador da campanha e rota independente de escalonamento não foram confirmados.

Os workflows nomeiam runners self-hosted mac/corelink-builder. CODEOWNERS tem uma regra geral para @gmhelmold, mas o próprio arquivo descreve-a como solicitação de review, não gate. Não trate essa regra como escalonamento ou aprovação independente.

Preserve estes cinco invariantes do contrato de ownership:

1. Documentação não disfarça mudança funcional.
2. Esta skill não concede autoridade para produção, gastos ou alteração de outros sistemas.
3. Fuzz source/configuração não é falsa evidência de execução ou produção.
4. Relações e riscos conhecidos não são omitidos para caber no perfil S.
5. O autor não aprova seus próprios documentos.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| O que os targets fazem? | [Referência](../../../docs/ownership/crates/corelink-meta-fuzz/REFERENCE.md#r01) |
| Quais relações e efeitos estão demonstrados? | [Blast radius](../../../docs/ownership/crates/corelink-meta-fuzz/BLAST_RADIUS.md#b03) |
| Como validar ou recuperar? | [Manutenção](../../../docs/ownership/crates/corelink-meta-fuzz/MAINTENANCE.md#m02) |
| A API MetaStore ou o schema/provider mudou? | Ative [own-corelink-meta](../own-corelink-meta/SKILL.md) e leia o contrato do provider; mantenha neste package somente o efeito no oracle |
| O tipo ou parsing Digest mudou? | Ative [own-corelink-hash](../own-corelink-hash/SKILL.md) e reconcilie `REL-002`; não transfira ownership da implementação |
| Onde estão os bytes fixados? | [Manifesto](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-meta/fuzz/Cargo.toml), [target round-trip](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-meta/fuzz/fuzz_targets/commit_put_roundtrip.rs), [target idempotency](https://github.com/HuGR-dev/corelink-server/blob/cca798ff5bc2df660ecf2570ed243eb9775ff3d0/crates/corelink-meta/fuzz/fuzz_targets/audit_idempotency.rs) |

Carregue apenas as APIs, invariantes, RELs e PROC pertinentes. O target chama InMemoryMetaStore; o nome fuzz ou o job configurado não prova execução nem alcançabilidade de D1.

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação e evidência | Parar quando |
|---|---|---|
| Mudar o mapeamento de bytes ou o oráculo de commit_put_roundtrip. | Atualizar API-001/INV-001/INV-002; selecionar PROC-001 e registrar o target e resultado. | Não há input reproduzível, baseline ou resultado completo. |
| Mudar retries, payload ou tratamento de conflito em audit_idempotency. | Atualizar API-002/INV-003; selecionar PROC-002; conferir o fake e não inferir rollback de D1. | A afirmação depende de persistência ou estado fora do fake. |
| Alterar API do provedor ou workflow. | Conferir REL-001/002/006 e coordenar com os packages/owners verificados. | A relação/owner do contrato, gate de merge ou impacto está desconhecido. |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Fixe o commit de fonte e confirme manifesto, target e diff.
2. Leia o contrato e a implementação do fake antes de interpretar uma assertion.
3. Verifique os efeitos externos e os três sentidos de cada REL.
4. Selecione somente o PROC aplicável e confirme toolchain, diretório e isolamento.
5. Preserve logs e reproducer; diferencie código, corpus/artifact e dado persistido.
6. Atualize os quatro documentos afetados e encaminhe seus hashes para cold review independente.

<a id="s06"></a>
## S06 — Condições de parada

Pare diante de target/feature não declarado, fonte fora do commit fixado, falha sem artefato reproduzível, ou divergência entre o fake e a propriedade alegada. Pare também antes de limpar corpus/artifacts não criados pela execução corrente. Questões de D1, R2, workflow runner compartilhado ou produção exigem o owner/operador real; sua identidade não foi inferida aqui.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue objetivo, baseline, target, API/INV/REL/PROC afetados, comando e resultado realmente observados, reproducer preservado, estado local e limites. Registre execução por target. O estado desta autoria continua draft; cold review não ocorreu. Não chame código compilável, workflow configurado ou fuzz intent de campanha executada.

[Voltar ao início](#s01)
