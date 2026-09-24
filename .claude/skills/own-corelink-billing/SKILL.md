---
name: own-corelink-billing
description: >-
  Assuma ownership de corelink-billing ao alterar a fachada canônica de cobrança,
  módulos internos de abuse/quota/replay ou seus contratos; não use para operar Stripe,
  D1, R2, Cloudflare ou implementar crates somente reexportadas.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-billing"
  manifest: "crates/corelink-billing/Cargo.toml"
  source-commit: "cca798ff5bc2df660ecf2570ed243eb9775ff3d0"
  evidence-set: "billing-pilot-source-20260919"
---

# Ownership — corelink-billing

Candidata de autoria; não aprovada. O source pin é `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.
Os procedimentos requeridos permanecem bloqueados e não executados até haver saída registrada
para esse pin. O fake de abuso pode deixar o limiter atualizado quando o audit posterior falha.

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Alterar `corelink_billing::*`, reexports, quota, abuse ou replay | Operar Stripe/D1/R2/Cloudflare, cobrar cliente ou mudar um crate reexportado |
| Diagnosticar falha de decisão, auditoria, isolamento ou migração interna | Inferir produção a partir de fake, teste ou `cargo tree` |
| Mudar schema embutido, limite, taxonomia, autorização ou idempotência | Migrar consumidores sem owner do servidor e do contrato de origem |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** `abuse`, `replay`, `quota::{core,cas,fsm}` e a composição de módulos.
**Contratos públicos sob responsabilidade:** API-001 a API-016 da referência.
**Fora do território:** implementação de aggregator, emit, reconcile, Stripe schema/real,
materializer, tier, rate headers e ratelimit; esta crate só fornece caminhos canônicos para eles.
**Escalonamento:** `corelink-server` é o consumidor Cargo resolvido; a rota não foi traçada.
`@gmhelmold` é rota de revisão via CODEOWNERS,
não owner operacional. Adapters e operadores detêm efeitos remotos; produção, gastos,
cobranças e dados reais exigem escalonamento explícito.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| O que é implementado e o que é alias? | [Referência](../../../docs/ownership/crates/corelink-billing/REFERENCE.md#r02) |
| O que pode propagar para dados ou Stripe? | [Relações](../../../docs/ownership/crates/corelink-billing/BLAST_RADIUS.md#b03) |
| Como diagnosticar ou validar? | [Procedimentos](../../../docs/ownership/crates/corelink-billing/MAINTENANCE.md#m02) |
| Qual contexto de domínio usar? | [Conhecimento de billing](../../../docs/knowledge/crates/billing-commerce.md) |
| Como separar fake, alias e wiring? | [built-not-wired](../built-not-wired/SKILL.md) |
| Qual contexto transversal aplicar? | [okf-context](../okf-context/SKILL.md) |

Carregue apenas o trecho relevante. A documentação de conhecimento não substitui código,
contrato ou evidência atual.

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação e evidência | Parar quando |
|---|---|---|
| Alterar uma reexportação ou símbolo público | Mapear caminho antigo/canônico, REL e build do consumer | Algum caminho público ou crate de origem não foi validado |
| Alterar decisão quota, abuse ou replay | Identificar INV, teste negativo e estado persistido/fake | Predicado, compatibilidade ou recuperação não estiverem definidos |
| Ação alcança Stripe, D1, R2 ou Worker | Limitar-se a diagnóstico local; escalar ao operador | Exigir credencial, dado real, custo, deploy ou escrita remota |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme manifesto, baseline, target e se o módulo é interno ou reexportado.
2. Leia API, INV, REL e procedimento aplicáveis.
3. Trace consumidor `corelink-server` e o owner da implementação de origem quando houver alias.
4. Defina validação negativa e recuperação antes de mudar schema, wire, decisão ou estado.
5. Execute somente testes locais autorizados e guarde comando e saída completa.
6. Atualize os artefatos e peça cold review dos bytes finais afetados.

<a id="s06"></a>
## S06 — Condições de parada

Pare e escale quando houver conflito entre o caminho canônico e o crate de origem; quando uma
migração D1, evento Stripe, objeto R2 ou operação Cloudflare for necessária; quando um efeito
financeiro, tenant ou autorização não puder ser reproduzido em isolamento; ou quando o rollback
de dados não estiver especificado. Não substitua prova de wiring por reexport ou compilação.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue objetivo, baseline, módulos internos versus aliases, API/INV/REL/PROC afetados, comandos
e resultados reais, estado final observado, limites e owner das pendências. Para aprovação,
registre quatro vereditos independentes. Não alegue operação em produção só porque há fake ou teste.

[Voltar ao início](#s01)
