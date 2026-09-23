# Cold review independente do padrão CO-1 — bytes atuais — 2026-09-23

**Veredito: FIX_FIRST. Freeze: BLOCKED.** O contrato cobre os requisitos centrais, mas dois caminhos normativos ainda divergem da implementação e a calibração exigida pelo próprio G0 não foi demonstrada nos mesmos bytes. Este parecer não aprova os quatro artefatos de nenhum package nem a publicação de issues.

## Entrada e método

- Revisor: agente independente `/root/cold_review_standard_current`, em contexto novo, sem participação na autoria dos arquivos avaliados. Checkout compartilhado na branch `codex/corelink-ownership-campaign`, `HEAD` observado `3df52eb71acdd4e00084d42f0b64191674bf42eb`. Outros agentes editavam documentos de packages; esta revisão não os aprova.
- Contrato examinado: `docs/ownership/STANDARD.md`, SHA-256 `0938c5fe44f2b60d75956a47eaee4c0f12601b97cbc4573989203f8a415d076f`. Apoio: `COMMON.md` (`2519e39ee38cc59f727f8750599fa0fc1d292872a0838cf90548d28511166df2`), `VALIDATION-MATRIX.md` (`f87a63695d95bc9689fbf54cfeb575e7a5f06cf78e7331935754242dd008a46b`), quatro templates, `COLD_REVIEW.md`, schema, ferramentas, testes, registry/índice e medições dos cinco pilotos. Os hashes identificam os bytes examinados, não uma aprovação por si.
- Revisão por leitura direta das fontes e dos mecanismos, sem adotar conclusões de reviews anteriores. `python3 -m unittest discover -s tests -q` em `docs/ownership`: **137 testes, OK**. Testes de fixtures comprovam o comportamento dos helpers, não completude semântica nem cold review real.

## Requisitos conferidos

| Área | Resultado | Evidência direta |
|---|---|---|
| Unidade e escopo | PASS como regra | `STANDARD.md` §§1–2 usa package Cargo, classifica independentes e distingue implementação de reexport; `prepare_census.py` possui classificação explícita. |
| Quatro entregas, navegação, limites | PASS como regra e checker estrutural | `STANDARD.md` §§2–3, templates S/R/B/M e `check_docs.py` definem seções, âncoras, tetos simultâneos e overflow bloqueante. A semântica permanece manual, conforme `VALIDATION-MATRIX.md`. |
| Evidência e blast radius | PASS como regra | `STANDARD.md` §§4, 6–7 e `COMMON.md` distinguem SOURCE/RESOLVED/execução/runtime, três direções de relação e consumidores fora de Cargo; schema tem identidade compartilhada e `ownership_gate.py` concilia peers. |
| Manutenção e autoridade | PASS como regra | `STANDARD.md` §§5, 8–9 e templates exigem modos por PROC, paradas e quatro vereditos independentes; `publication_gate.py` é somente preflight de leitura. |
| Cinco axiomas e emissão | PASS como regra/template | `STANDARD.md` §11 e `ISSUE.md.tmpl` têm Success, Completeness, Quality, DoD, Invariants, marcador e deduplicação. A emissão real depende de evidência externa e permanece separada. |
| Aprovação/invalidação/índice | **FIX_FIRST** | Achados CR-STD-01 e CR-STD-02 abaixo. |
| Calibração de capacidade | **BLOCKED** | Achado CR-STD-03 abaixo. |

## Achados obrigatórios

### CR-STD-01 — exceção metadata-only sem representação executável

`STANDARD.md` §9.3 (linhas 388–398) permite preservar a revisão semântica após uma normalização de frontmatter e exige `semantic_review`, `metadata_readback` e os dois hashes. `schemas/package-record.schema.json` não possui esses campos e recusa propriedades adicionais. `ownership_gate.py` compara `artifact.sha256` ao arquivo inteiro (linhas 156–163), portanto rejeita precisamente a alteração que o contrato permite. Os 137 testes não exercitam essa transição. **Consequência:** duas regras incompatíveis para a mesma aprovação; um operador teria de ignorar o contrato ou contornar o gate. **Correção:** implementar os dois estados/hashes e um teste de metadata-only positivo mais um teste de mudança semântica negativa, ou remover a exceção normativa e exigir novo review para qualquer byte alterado. Reavaliar o texto e o schema nos hashes finais.

### CR-STD-02 — índice prometido como estado de review/publicação, mas permanentemente não informado

`STANDARD.md` §4.1 (linhas 138–142) e §10 exigem índice/registry gerados que distingam revisão e publicação; `COMMON.md` §integração diz que os registros por package alimentam a agregação. Porém `generate_registry.py` linhas 97–118 fixa cada `cold_review` em `UNVERIFIED`, cada `publication` em `NOT_PUBLISHED` e `publication_count` em zero, sem entrada para records ou ledger. `ownership_gate.py` possui um agregador separado, mas não produz o `registry.json` atual. Não existe `docs/ownership/records/` no checkout avaliado. **Consequência:** mesmo uma revisão futura válida não poderá ser refletida no índice canônico pelo caminho documentado. **Correção:** definir e testar uma agregação única que consuma records/ledger verificados e preserve estados separados; demonstrá-la em pelo menos um piloto real, com `UNVERIFIED` para evidência ausente. Não converter `EVIDENCE_CONSISTENT` em aprovação automática.

### CR-STD-03 — calibração G0 ainda não está nos bytes atuais

`STANDARD.md` §12 G0 (linhas 470–480) exige, para cada um dos cinco pilotos, perfil, linhas/palavras/bytes, população de relações/módulos/procedimentos, overflow e decisão de capacidade. `PILOT-CALIBRATION-READBACK-20260923-CURRENT.md` registra apenas perfil e tamanhos. `PILOT-CALIBRATION-20260922.md` tem as populações, mas está pinado em `1f0786f...` e em bytes anteriores. Medição independente de `wc -l -w -c` encontrou o manual atual de `corelink-server` em **237 / 1.915 / 15.125**, enquanto a tabela de 2026-09-23 declara **237 / 1.886 / 14.738**. Os cinco conjuntos ainda cabem nos tetos declarados, mas a tabela que se anuncia como current-byte já não coincide integralmente com o checkout e não demonstra a população material atual. **Correção:** repetir a medição depois de estabilizar os pilotos e registrar população, justificativa S/H, overflow e decisão nos mesmos hashes; corrigir a linha do server e quaisquer outras alterações posteriores. Isso é evidência de capacidade, não aprovação dos pilotos.

## Decisão e encerramento deste review

O texto fornece uma base aproveitável para o padrão e os 137 testes passaram, mas **não congelar os bytes `0938c5fe…`**. Fechar CR-STD-01/02 com mecanismos e testes correspondentes; fechar CR-STD-03 com uma calibração conjunta dos cinco pilotos. Depois, submeter os novos bytes do padrão e dos componentes normativos afetados a readback/cold review independente. A integração em `main`, os registros reais por package, o censo atual e a publicação de issues são gates de adoção separados; este parecer não os atesta.
