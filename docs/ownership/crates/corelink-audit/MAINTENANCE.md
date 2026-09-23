---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-audit
manifest: crates/corelink-audit/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: audit-static-graph-20260920
---

# corelink-audit — manutenção

Procedimentos candidatos limitados a fonte e checagens documentais. Não são autorização
para deploy, rede, operação de dados ou declaração de revisão independente.

[Baseline](#m01) · [Taxonomia](#m02) · [Hash](#m03) · [PII](#m04) ·
[Integração](#m05) · [Registro](#m06).

<a id="m01"></a>
## M01 — Confirmar baseline

Modo: `STATIC_LOCAL`. Predicado: SHA e manifesto são os desta edição. Ação: comparar HEAD,
branch e `crates/corelink-audit/Cargo.toml` antes de interpretar fonte. Parada: baseline,
árvore ou manifesto divergente. Recuperação: não editar; reconciliar o novo recorte. Evidência:
SHA observado, branch e caminhos lidos.

<a id="m02"></a>
## M02 — Alterar taxonomia/envelope

Modo: `STATIC_LOCAL`. Predicado: `AuthEventType`, `AuthEventData` ou campo CloudEvents mudou.
Ação: mapear variante, serialização, examples e consumers conhecidos. Parada: leitor ou regra de
compatibilidade ausente. Recuperação: manter a mudança bloqueada e pedir decisão de versão.
Evidência: diff, `events.rs`, vetores e lista de consumers estáticos.

<a id="m03"></a>
## M03 — Alterar JCS ou chain hash

Modo: `STATIC_LOCAL`. Predicado: `compute_content_hash`, `ContentHash`, `ChainHash` ou link
muda. Ação: comparar canonical vectors e a separação JCS/link. Parada: bytes históricos,
leitor N-1 ou migração não definidos. Recuperação: não prometer rollback de dados; encaminhar
ao owner do writer/reader. Evidência: símbolos, vetores e plano de compatibilidade.

<a id="m04"></a>
## M04 — Alterar PII ou retention

Modo: `STATIC_LOCAL`. Predicado: newtype, macro, payload ou mapeamento tier/hint muda. Ação:
revisar todas as variantes afetadas e `src/redact.rs`/`retention.rs`. Parada: PII bruto,
semântica do tier ou worker desconhecidos. Recuperação: remover a rota insegura ou escalar a
política. Evidência: diff, tipos, macro, teste de redaction e classificação declarada.

<a id="m05"></a>
## M05 — Alterar emissão ou consumer

Modo: `STATIC_LOCAL`. Predicado: trait, outbox, métrica, reexport ou consumer muda. Ação:
traçar implementador, imports e entrypoint estático. Parada: alegação requer sink durável,
SIEM, runtime ou atomicidade não visível na fonte. Recuperação: separar contrato local da
integração e encaminhar ao adapter. Evidência: implementações, manifestos e busca reversa.

<a id="m06"></a>
## M06 — Registrar e escalar

Modo: `STATIC_LOCAL`. Predicado: análise ou mudança terminou. Ação: registrar baseline,
arquivos, relações, comandos documentais, resultados e lacunas. Parada: nenhum resultado de
checker equivale a aprovação, runtime ou cold review. Recuperação: marcar desconhecido e abrir
decisão com fonte/consumer/owner necessário. Evidência: diff, checker, `git diff --check` e SHA.

[Referência](REFERENCE.md#r01) · [Impactos](BLAST_RADIUS.md#b01) · [Início](#m01)
