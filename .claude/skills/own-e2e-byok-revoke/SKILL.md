---
name: own-e2e-byok-revoke
description: >-
  Assuma ownership de e2e-byok-revoke ao alterar seu manifesto, helpers locais,
  fluxos de teste ou assertions declaradas; não use para implementar corelink-byok,
  corelink-ops, operar providers ou alegar execução em produção.
metadata:
  schema: "corelink-ownership/1.1"
  profile: "S"
  package: "e2e-byok-revoke"
  manifest: "tests/e2e-byok-revoke/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "e2e-byok-revoke-static-source-20260921"
---

# Ownership — e2e-byok-revoke

[Acionamento](#s01) · [Território](#s02) · [Leitura](#s03) · [Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07)

<a id="s01"></a>
## S01 — Acionamento

| Ative para | Não ative / recuse esta skill para |
|---|---|
| Alterar uma asserção ou fixture local de `tests/happy_revoke_flow.rs`; investigar mudança em `src/helpers.rs::KillSwitchRunner`; reconciliar um dos oito `[[test]]` de `tests/e2e-byok-revoke/Cargo.toml`. | Tarefa que apenas menciona CoreLink/BYOK ou altera outro package sem tocar esta unidade; alteração de `crates/corelink-byok` (provider, cache, detector ou contrato); alteração de `crates/corelink-ops::MultiChannelAlerter`; executar provider/KMS real, deploy ou mutação de produção. |

Nos dois últimos casos, não edite este package para substituir o owner. Roteie código/contrato BYOK ao owner de `corelink-byok`, wiring/contrato de alerta ao owner de `corelink-ops`; operações reais exigem autorização e procedimento próprios, não concedidos por esta skill.

<a id="s02"></a>
## S02 — Território e autoridade

Implementação local: `src/{lib,helpers}.rs` e os oito testes explicitamente
declarados. Contratos locais: [R04 API-001–API-015](../../../docs/ownership/crates/e2e-byok-revoke/REFERENCE.md#r04).
S01 identifica positivo/negativo; aqui “território” não expande a autoridade:
o package fornece fixtures e um runner de teste, nunca implementação ou operação
de KMS, `DekCache`, `RevocationDetector`, store/audit persistente ou canal real.

Trabalho em `corelink-byok` deve parar neste pacote e ser roteado à
[referência do owner](../../../docs/ownership/crates/corelink-byok/REFERENCE.md#r01);
trabalho em `corelink-ops` deve ser roteado à
[referência do owner](../../../docs/ownership/crates/corelink-ops/REFERENCE.md#r01).
`.github/CODEOWNERS` confirma `@gmhelmold` na regra BYOK e na regra geral `*`;
`corelink-ops` não tem regra específica. Não assuma acesso ou autorização de
operação por existir um owner no arquivo.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| Identidade, contratos, fluxo e pontos de imposição | [R01–R08](../../../docs/ownership/crates/e2e-byok-revoke/REFERENCE.md#r01) |
| Dependências, oito alvos, CI e fronteiras | [B01–B06](../../../docs/ownership/crates/e2e-byok-revoke/BLAST_RADIUS.md#b01) |
| Diagnóstico, seleção de checks e recuperação | [M01–M06](../../../docs/ownership/crates/e2e-byok-revoke/MAINTENANCE.md#m01) |
| Contexto canônico aplicável | [Perfil OKF verificado](../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) — rota apenas; siga o link aos documentos canônicos pertinentes, sem copiar/redefinir política |

Carregue só as seções pertinentes. Não copie nem redefine política OKF.

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação e evidência | Parar quando |
|---|---|---|
| Um símbolo público local muda | Atualize seu `API-ID`, ponto de imposição, consumidores e `REL-ID`; prove com diff de fonte. | A compatibilidade depende de consumidor ou target resolvido não inspecionado. |
| Um status/call order do `KillSwitchRunner` muda | Trace o fluxo e os estados parciais em `FLOW-001/002`; preserve erros antes/depois de escrita. | A conclusão precisa de evento, status ou alerta durável observado. |
| Uma alteração é de provider, detector, cache, store ou alerter externo | Recuse atribuir a implementação a este harness e encaminhe a `corelink-byok` ou `corelink-ops`. | A rota de owner/escopo não está clara; mantenha UNKNOWN e pare. |

`INV-*` descreve predicados falsificáveis da implementação local. `AX-*` em R08
são limites epistemológicos, não regras que o código impõe.

<a id="s05"></a>
## S05 — Fluxo

1. Fixe manifesto, nome `[package]`, baseline e escopo autorizado. 2. Separe implementação local, contrato dependente, composition root e operação. 3. Siga API/INV/FLOW da referência e as relações inversas da blast map. 4. Selecione um procedimento M/PROC compatível com o ambiente; registre o estado anterior. 5. Se houver execução autorizada, escolha procedimento cujo modo seja exatamente `READ_ONLY`, `LOCAL_ISOLATED` ou `AUTHORIZED_OPERATION`; a skill não concede autoridade. O corpus atual é documental: não

execute Cargo, provider live ou produção. 6. Atualize os registros afetados e peça revisão fria dos bytes finais.

<a id="s06"></a>
## S06 — Paradas e escalonamento

Pare se target/feature resolvido, consumidor externo, provider real, persistência
ou execução de CI for necessário para a afirmação. Pare também se fixture contradizer
o contrato ou sequência deixar cache/status parcialmente alterados sem recuperação
definida.

Escalone pelo CODEOWNERS confirmado: alteração de `corelink-byok` usa
`/crates/corelink-byok/ @gmhelmold`; `corelink-ops` não tem regra específica e cai
em `* @gmhelmold`. Não contate implicitamente nem assuma disponibilidade/autorização.
Handoff deve registrar contratos e RELs afetados, risco/estado parcial, owner/rota
verificados, pergunta objetiva, baseline e evidência necessária. Se owner, escopo
ou autoridade não puder ser verificado, pare com `UNKNOWN` e encaminhe ao lead.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue objetivo e baseline; paths; contratos/API, INV, FLOW, REL e PROC afetados;
gates selecionados e resultados realmente executados; risco/estado parcial; unknowns;
owner/rota e próximo responsável. Para escalonamento, inclua RELs/contratos, risco,
responsável verificado e evidência solicitada. Não chame declaração, fake, checker,
comentário de teste ou comando candidato de execução/aprovação.

[Voltar ao início](#s01)
