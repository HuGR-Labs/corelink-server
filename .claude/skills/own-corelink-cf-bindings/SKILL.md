---
name: own-corelink-cf-bindings
description: >-
  Assuma ownership de corelink-cf-bindings para adapters Cloudflare wasm32 e
  stubs nativos; não execute operações remotas Cloudflare.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-cf-bindings"
  manifest: "crates/corelink-cf-bindings/Cargo.toml"
  source-commit: "cca798ff5bc2df660ecf2570ed243eb9775ff3d0"
  evidence-set: "cf-bindings-pilot-source-20260920"
---

# Ownership — corelink-cf-bindings

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) · [Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Alterar adapter R2, D1, KV, DO, stub ou target wasm | Operar binding, bucket, D1, KV, DO ou deploy Cloudflare |
| Diagnosticar isolamento de tenant, audit fence ou `WasmOnly` | Alterar semântica de `corelink-worker` sem owner correspondente |
| Mudar API reexportada ou compatibilidade dual-target | Tratar build nativo como prova de binding remoto |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** wrappers CF, validação de escopo, audit hooks, stubs nativos e reexports.

**Contrato público:** tipos `Cf*Real`, adapters `Cf*Adapter` wasm, tipos de escopo/erro, aliases de audit, trait impls e prefixos diagnósticos.

**Composition roots:** `corelink-clerk-cf::prod_wiring::build_real_bindings`/`health::main`; `corelink-container` e o materializador billing são consumidores condicionais, não donos desta implementação.

**Operador runtime:** `system:Cloudflare Workers` via os manifests Wrangler; não verificado como deploy.

**Aprovador/revisão:** `@gmhelmold` é a rota default em `.github/CODEOWNERS`; CODEOWNERS solicita revisão, mas o arquivo registra que não é merge gate neste plano.

**Escalonamento:** owners de consumidor, `corelink-worker`/`corelink-cas`, segurança e operação Cloudflare; rota humana verificada é `@gmhelmold`, e a rota operacional Cloudflare permanece UNKNOWN.

**Fora do território:** binding declarado em Wrangler, operação Cloudflare, derivação canônica do tenant e semântica interna dos traits. Esta skill não autoriza segredo, rede, custo ou escrita remota.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| Qual adapter/tipo/erro muda? | [Referência](../../../docs/ownership/crates/corelink-cf-bindings/REFERENCE.md#r03) |
| Quem consome ou reexporta a superfície? | [Relações](../../../docs/ownership/crates/corelink-cf-bindings/BLAST_RADIUS.md#b03) |
| Qual validação é segura? | [Manutenção](../../../docs/ownership/crates/corelink-cf-bindings/MAINTENANCE.md#m02) |
| Qual contrato sistêmico consultar? | [Credential handling OKF](../../../docs/knowledge/security/credential-handling.md); depois roteie pelo [okf-context](../../../.claude/skills/okf-context/SKILL.md) |
| Como separar build de alcance? | [Built-not-wired](../../../.claude/skills/built-not-wired/SKILL.md) |

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação e evidência | Parar quando |
|---|---|---|
| Alterar mutação R2/D1/KV/DO | Preservar escopo e audit antes do backend; testar stub/fake | Exigir binding ou dado real |
| Alterar target ou `worker::*` | Separar wasm real de stub nativo; registrar target | Build não prova runtime |
| Mudar erro/reexport | Localizar consumidores e compatibilidade | Contrato do consumidor é incerto |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme manifesto, baseline, target e consumidor; use os registros REL-CF com âncora própria em [B03](../../../docs/ownership/crates/corelink-cf-bindings/BLAST_RADIUS.md#b03).
2. Trace tipo público até trait, escopo, audit e backend; atualize a relação existente quando sua ativação ou falha mudar, sem inferir aresta não observada.
3. Preserve ordem validação → audit → backend ou stub.
4. Defina teste local de erro/fake antes de alterar.
5. Não use credencial ou binding real para certificar documentação.
6. Atualize os três documentos e solicite cold review dos bytes alterados; revisão, rota de escalonamento e execução ficam explícitas como estados separados.

<a id="s06"></a>
## S06 — Condições de parada

Pare com credencial, `wrangler`, R2, D1, KV, Durable Object, rede, tenant, deploy ou custo. Pare também se a mudança tornar o stub nativo um falso positivo, remover a comparação de escopo ou permitir mutação depois de falha do audit hook.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue baseline, target, adapter, consumidor, contrato, comando e resultado real. Separe implementação wasm, build selecionado e runtime observado. Aprovação exige quatro cold reviews independentes; nenhuma aprovação certifica operação Cloudflare.

[Voltar ao início](#s01)
