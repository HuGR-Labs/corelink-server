---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-bazel-bridge
manifest: crates/corelink-bazel-bridge/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: bazel-bridge-static-20260920
---

# corelink-bazel-bridge — manutenção

Procedimentos de manutenção são source/static-only. Nenhum procedimento autoriza rede, deploy, runtime, testes Cargo ou alteração de contratos fora do package.

[Modo](#m01) · [Baseline](#m02) · [URI/digest](#m03) · [Delegação](#m04) · [Consumidor](#m05) · [Recuperação](#m06).

<a id="m01"></a>
## M01 — Modo e limite

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Estático | A tarefa não requer prova operacional | Ler manifesto e fontes fixadas | Paths em R01/R03 | Se pedir runtime, registrar lacuna e escalar |
| Contrato | API pública ou constante mudou | Traçar R04–R06 e B01–B05 | Diff e relações afetadas | Se depender de outro owner, congelar interface |

<a id="m02"></a>
## M02 — Baseline e escopo

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Estático | Antes de qualquer edição | Confirmar HEAD, branch e manifesto | `git rev-parse`, branch, `Cargo.toml` | Se divergirem, parar e restaurar checkout correto |
| Documental | Só os artefatos de ownership são autorizados | Conferir `git diff --name-only` | Lista de paths | Se houver código/Cargo, reverter o plano e escalar |

<a id="m03"></a>
## M03 — Alteração de URI ou digest

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Contrato | Path, método, JSON ou `Digest` muda | Tratar `uri.rs` como parser e revisar rotas Axum do container | R03/R04 e `part-00.rs` | Se verb/path montado mudar, passar ao container |
| Integridade estática | CAS write muda | Preservar tamanho no adapter e chamada `verify_sha256` no boundary do container | R05, `adapter.rs`, `digest.rs`, `part-00-01.rs` | Se uma camada omitir seu check, bloquear mudança |

<a id="m04"></a>
## M04 — Alteração de delegação ou erro

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Contrato | Traits CAS/AC, adapter ou erro mudam | Enumerar handlers e comparar sugestão `http_status` com mapper ativo do container | B03, B04, R07 e `error.rs` | Se o status HTTP do container não for revisado, congelar mudança |
| Find-missing | Lote ou cap muda | Preservar cap, ordem e falha propagada | `find_missing.rs`, R05 | Se cardinalidade divergir, rejeitar resultado e investigar |

<a id="m05"></a>
## M05 — Coordenação com consumidor

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Grafo estático | Alteração toca API pública | Revisar consumidor direto conhecido | B02 e manifest do container | Se consumidor adicional surgir, adicioná-lo ao escopo |
| Limite | Teto de blob muda | Coordenar `corelink-hash` e container; mantê-lo distinto de SHA-256 REAPI | B05 | Se compatibilidade de request não for conhecida, não inferir |

<a id="m06"></a>
## M06 — Stop, recovery e evidência final

| Modo | Predicado | Ação | Evidência | Pare / recovery |
|---|---|---|---|---|
| Entrega | Documentação pronta | Rodar checker documental e `git diff --check` | Exit status e diff limpo | Se falhar, corrigir somente o artefato autorizado |
| Lacuna | Cliente, R2, D1, edge ou runtime é solicitado | Declarar desconhecido com owner/ação seguinte | R08 e B06 | Parar; fonte estática não certifica operação |
