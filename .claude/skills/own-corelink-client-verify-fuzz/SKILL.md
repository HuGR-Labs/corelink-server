---
name: own-corelink-client-verify-fuzz
description: >-
  Use for static ownership work on the corelink-client-verify-fuzz manifest,
  verify_sync and verify_ffi harnesses, or their source relations. Separate
  fuzz intent and configured CI from execution, reachability, and coverage.
  Do not use for library implementation, SDK wrappers, workflow changes, or
  runtime/provider operations.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-client-verify-fuzz"
  manifest: "crates/corelink-client-verify/fuzz/Cargo.toml"
  source-commit: "1177dad2ca2a9f21c29b5a118aa7944b77147798"
  evidence-set: "client-verify-fuzz-source-static-20260921"
---

# Ownership — corelink-client-verify-fuzz

[Acionamento](#s01) · [Autoridade](#s02) · [Leitura](#s03) ·
[Decisões](#s04) · [Fluxo](#s05) · [Paradas](#s06) · [Saída](#s07).

<a id="s01"></a>
## S01 — Acionamento

| Use quando | Não use quando |
|---|---|
| Revisar `fuzz/Cargo.toml`, `verify_sync.rs` ou `verify_ffi.rs`, ou determinar o alcance declarado de seus targets. | Alterar `corelink-client-verify`, SDKs, CI, provider, deployment ou fazer afirmação de execução/cobertura. |

<a id="s02"></a>
## S02 — Território e autoridade

**Implementação própria:** package independente `corelink-client-verify-fuzz`; os dois harnesses em `fuzz_targets/`.
**Contrato usado, não owned:** API sync e módulo FFI de `corelink-client-verify`.
**Composition root estático:** `.github/workflows/corelink-client-verify.yml`; não prova que o workflow rodou.
**Pessoa/equipe de implementação, operador nominal, aprovador e rota de escalonamento:** UNKNOWN; não inventar assignee nem autorização.
Esta skill não autoriza Cargo/Rust/fuzz, CI, produção, rede, credenciais ou alterações fora dos quatro artefatos.

<a id="s03"></a>
## S03 — Roteamento de leitura

| Pergunta | Ler diretamente |
|---|---|
| O que os targets declaram e chamam? | [Referência](../../../docs/ownership/crates/corelink-client-verify-fuzz/REFERENCE.md#r01) |
| Quais fronteiras e efeitos são sustentados? | [Relações](../../../docs/ownership/crates/corelink-client-verify-fuzz/BLAST_RADIUS.md#b03) |
| Como validar só a documentação? | [Procedimento](../../../docs/ownership/crates/corelink-client-verify-fuzz/MAINTENANCE.md#proc-001) |
| Como evitar inferência de execução? | [Built-not-wired](../../../.claude/skills/built-not-wired/SKILL.md) |

Carregue somente as seções pertinentes. Use a rota OKF apropriada quando a pergunta exceder a evidência estática; esta skill não atribui um responsável inexistente.

<a id="s04"></a>
## S04 — Decisões e invariantes

| Condição concreta | Ação e evidência | Parar quando |
|---|---|---|
| Manifesto, target, feature ou chamada muda | Conferir identidade e [axiomas](../../../docs/ownership/crates/corelink-client-verify-fuzz/REFERENCE.md#r05), atualizar RELs exatos e a matriz M04. | Resolução, execução ou cobertura for exigida sem evidência independente. |
| A tarefa toca `unsafe` | Distinguir o `allow` do harness da negação da library e exceção local `ffi`; revisar [REL-006](../../../docs/ownership/crates/corelink-client-verify-fuzz/BLAST_RADIUS.md#rel-006). | Dono da revisão de segurança ou contrato C não estiver confirmado. |

<a id="s05"></a>
## S05 — Fluxo de trabalho

1. Confirme baseline, package name, source pin e os quatro paths autorizados.
2. Leia a relação, chamada e invariante que a mudança toca; não infira fluxo por pasta.
3. Separe dependência Cargo, bytes passados e direção de impacto.
4. Use M04 apenas para checagens documentais neste escopo; comandos futuros de Cargo/fuzz exigem autorização e contexto próprios.
5. Registre resultados realmente observados e UNKNOWNs; solicite revisão independente dos hashes finais.

<a id="s06"></a>
## S06 — Condições de parada

Pare se baseline ou fonte mudar, se precisar de resolução Cargo, build, fuzz, execução CI, credenciais ou estado de runtime, ou se owner/escalonamento for necessário e não estiver verificado. Preserve os dados da tarefa e encaminhe a pergunta pela rota de governança do repositório; não escolha um responsável por suposição.

<a id="s07"></a>
## S07 — Evidência e saída

Entregue baseline e source pin; objetivo; API/INV/REL/PROC afetados; quatro paths e hashes; comandos documentais e seus resultados reais; unknowns; e estado da revisão independente. Uma declaração no manifesto ou workflow não significa execução, alcance de runtime, cobertura, aprovação ou deployment.

[Voltar ao início](#s01)
