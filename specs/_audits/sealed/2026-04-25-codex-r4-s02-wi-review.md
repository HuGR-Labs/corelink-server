# Codex R4 — Lote 10.2 S-02 WI Review

## Veredito Geral
`NO-SHIP` para implementação. O lote tem aparência de completude, mas dois WIs (`WI-S02-002` e `WI-S02-005`) conflitam com fontes canônicas de protocolo/arquitetura, e os outros três ainda têm gaps reais em contrato FFI, prova estatística, rigor de ship-gate e custo operacional. Se código for escrito a partir desta base, o risco não é só retrabalho: é cristalizar drift entre arquitetura, protocolo e comportamento de produção.

## Per-WI Findings

### WI-S02-002
**Score: 4/10**

A estrutura está lá, mas a especificação não está pronta para virar código sem rework. O documento mistura extensão proprietária com claim de conformidade REAPI, erra a superfície de autorização de `FindMissingBlobs` e não fecha uma semântica coerente de erro com os outros WIs do sprint.

- `specs/04_sprints/S02/work_items/WI-S02-002-getblob-findmissing.md:150-153` defere `BatchReadBlobs`, mas `specs/03_architecture/remote_cache_product_profile.md:405-413` marca `ContentAddressableStorage.BatchReadBlobs` como obrigatório. Enquanto isso não for resolvido via ADR, chamar `GetBlob` de “REAPI v2 conformance” é drift protocolar.
- `WI-S02-002:173-198` modela `FindMissingBlobs` sob `cache:r`, mas `specs/03_architecture/auth_model.md:176-180` define scope dedicado `cache-find-missing`.
- `WI-S02-002:107-110` diz que tentativa cross-tenant rejeita o request inteiro com `403`, mas `WI-S02-002:189-194` diz que `GetBlob` cross-tenant retorna `404`; isso ainda diverge do `403` explícito em `WI-S02-001`.
- `WI-S02-002:375-376` erra a conta em ordem de grandeza: `365B / 1M = 365.000`, não `365`.
- Invariantes, segurança e PRR estão formalmente presentes, mas muito mais rasos que o baseline `WI-S02-001`; falta mapeamento TLA+ ↔ código/teste realmente operacional.

### WI-S02-003
**Score: 5/10**

A decisão macro faz sentido: separar verify client-side em um crate reutilizável e sem dependência de Worker internals é o boundary correto. O problema é que o documento promete um contrato ABI/FFI que a API especificada não consegue cumprir, e descreve um comportamento de stream verify que não é tecnicamente alcançável com digest de blob completo.

- `specs/04_sprints/S02/work_items/WI-S02-003-corelink-client-verify-crate.md:50-90` e `:143-146` expõem `Result`, trait bounds, `impl Stream`, `AsyncRead` genérico e tipos Rust-only, enquanto o mesmo WI afirma `cdylib` + C ABI estável. `cbindgen` sozinho não transforma isso em superfície C utilizável.
- `WI-S02-003:172-175` diz que wrappers FFI ficam para S-15, mas `:145-146` e `:235-239` tornam C ABI/cbindgen um acceptance criterion agora. O boundary “crate Rust hoje” vs “FFI surface depois” está mal cortado.
- `WI-S02-003:111-112` e `:257-259` falam em fail-fast mid-stream; com verificação contra digest final do blob, a conclusão só existe em EOF, salvo se o protocolo trouxer provas por chunk/Merkle.
- `WI-S02-003:56-61` deixa `VerifyConfig` público, enquanto `:155-157` e `:185` exigem builder/private fields. O Gherkin também assume introspecção interna em `verifier.config.enabled`.
- `VerifyDisabled` aparece como erro em `:88-89`, mas a narrativa trata opt-out como bypass permitido com warning. O caller contract fica incoerente.

### WI-S02-004
**Score: 6/10**

O direcionamento técnico é defensável: middleware de timing padding é a abstração certa quando D1/R2 não podem ser tornados constant-time. O documento ainda superestima o que sua estatística prova e não está semanticamente alinhado com o restante de S-02.

- `specs/04_sprints/S02/work_items/WI-S02-004-constant-time-middleware.md:77-82` e `:104-105` tratam `p > 0.05` como prova de indistinguibilidade. Estatisticamente isso é só falha em rejeitar a hipótese nula, não prova de equivalência; falta power/effect-size ou teste de equivalência explícito.
- `WI-S02-004` gira em torno de 404 vs 403 (`:48-82`, `:144-160`), mas `WI-S02-002` mascara `GetBlob` cross-tenant como `404` e `WI-S02-001` usa `403`. O próprio sprint não concorda sobre qual superfície observável o atacante verá.
- `WI-S02-004:147` e `:257-259` justificam “deterministic jitter seeded per request_id” como anti-correlação. Se `request_id` for previsível/observável, isso deixa de ser ruído útil; a argumentação de ameaça está frouxa.
- `WI-S02-004:313-314` mede “≤ 5ms variance”, mas o custo real ao usuário é o padding absoluto perto de 200ms. A spec monitora leak diferencial, não o budget total de latência que está gastando.
- `WI-S02-004:384` deixa custo como `$X TCO`, o que está abaixo da barra HIGH_RISK para uma seção explicitamente exigida como numérica.

### WI-S02-005
**Score: 2/10**

Este é o WI mais fraco do lote e o principal bloqueador de integridade spec-first. O problema não é só falta de detalhe: ele contradiz frontalmente documentos canônicos sobre o que pode morar em KV e sobre o modelo de consistência de KV.

- `specs/04_sprints/S02/work_items/WI-S02-005-negative-cache-kv.md:47-76` e `:136-145` fazem KV-backed negative cache o design central, mas `specs/03_architecture/remote_cache_product_profile.md:390-391` diz que `FindMissingBlobs` deve sempre fazer fallback a R2 HEAD em miss de KV e que negative entries não devem ser cacheadas em KV.
- `WI-S02-005:217-218` assume `p99 stale window ≤ 5s` e fala em “KV strong consistency CF”, enquanto `specs/03_architecture/storage_semantics_matrix.md:72-76` e `specs/03_architecture/failure_modes.md:125` modelam KV como eventual/stale-prone.
- `WI-S02-005:67-71` e `:93` criam `MissReason::CrossTenantMasked`, mas nenhum lookup tenant-local consegue preencher isso sem uma checagem global de existência. Isso colide com `CTRL-ISO-005` e com a postura tenant-local do modelo de segurança.
- `WI-S02-005:223-226` leva tombstone para `410 Gone`, divergindo de `WI-S02-001` e do sprint contract, que usam `404` para soft-delete.
- O princípio canônico “KV never-source-of-truth” não sobe a DoD/PRR; a propriedade de resiliência mais importante fica fora do ship gate.

### WI-S02-006
**Score: 4/10**

Como ideia de ship gate, o WI está correto: S-02 precisa de um gate duro de evidência antes de selar. Como documento HIGH_RISK, ele herda inconsistências upstream, exagera claims estatísticos e deixa frouxas justamente as seções que deveriam ser mais concretas.

- `specs/04_sprints/S02/work_items/WI-S02-006-property-tests-rb-prr.md:61-63`, `:100-103` e `:217-223` oscilam entre 11 e 13 sign-offs; o sprint contract também já tem aritmética inconsistente. Ship gate não pode ser ambíguo sobre autoridade de seal.
- O filename promete “round-robin”, mas o corpo nunca operacionaliza round-robin de tenants, scheduler ou argumento de cobertura. Hoje isso parece rótulo cargo-cult, não design de teste.
- `WI-S02-006:70-74` converte 100k iterações em “99.999%+ confidence” sem modelo, intervalo de confiança ou distribuição adversarial definida. Isso não é linguagem estatística aceitável para gate HIGH_RISK.
- `WI-S02-006:202-206` e `:237-239` dependem de corrupção direta em R2, mas o WI não define isolamento de bucket, cleanup nem abort criteria; para CI/staging repetível, isso está solto demais.
- `## 15. Chaos Experiments` colapsa para “Already covered”, abaixo da rubrica pedida e abaixo do nível do `WI-S02-001`.

## Cross-WI Consistency

- As dependências macro estão razoavelmente claras: `WI-S02-002` depende de `WI-S02-001`, `WI-S02-004` depende de `WI-S02-001/002`, e `WI-S02-006` referencia corretamente `WI-S02-001..005 SEALED` como hard blocker (`WI-S02-006:339-341`).
- A semântica de erro não está coerente: `WI-S02-001` usa `403` para cross-tenant read (`WI-S02-001:200-207`), `WI-S02-002` mascara `GetBlob` como `404` (`WI-S02-002:189-194`) e `WI-S02-004` assume uma fronteira 404/403 uniforme para a mitigação estatística.
- O reuse de `corelink-hash` em `WI-S02-003` está consistente. `corelink-keys` não aparece nos cinco WIs, o que é aceitável para CAS read path, mas deveria estar explicitamente fora de relevância, não apenas ausente.
- `WI-S02-005` contradiz fontes canônicas herdadas (`REMOTE-CACHE-PRODUCT-PROFILE`, `RESILIENCE-PATTERNS`, `STORAGE-SEMANTICS-MATRIX`) sem ADR corretiva; isso contamina `WI-S02-006`, que depois tenta provar uma arquitetura que o product profile proíbe.
- ADR-0023 está corretamente mencionado em `WI-S02-004 §9.7/§13` e whitelisted em `scripts/validate_references.py:167`. `python3 scripts/validate_references.py --json` retornou zero dangling refs, então o problema aqui é semântico, não referencial.
- Em vários WIs, seções HIGH_RISK existem só estruturalmente: customer impact colapsa para um JTBD, chaos experiments não trazem hipótese/procedimento/abort, rollback não traz RTO/RPO, e cost analysis não desce para Workers/R2/D1/KV por request.

## Technical Accuracy Issues

- `GetBlob` está tratado como “REAPI v2” em `WI-S02-002` e no sprint contract, mas a fonte canônica do produto exige `BatchReadBlobs` como método CAS obrigatório (`remote_cache_product_profile.md:407-413`). Sem ADR de supersedence, isso é drift protocolar.
- A autorização de `FindMissingBlobs` está errada em contrato: o auth model define `cache-find-missing` (`auth_model.md:176-180`), não apenas `cache-r`.
- A separação client-side verify vs server-side faz sentido em arquitetura, mas `WI-S02-003` não especifica uma API no_std-friendly nem FFI-safe; ele acerta a fronteira e erra o contrato público.
- A estratégia de timing padding via middleware em `WI-S02-004` é defensável; o problema está no modelo de prova e na definição frouxa das classes observáveis que devem ser equalizadas.
- A estratégia de TTL/invalidation em `WI-S02-005` não é segura como está porque assume propagação rápida/forte em KV e usa KV para verdade negativa onde o product profile proíbe isso.
- `WI-S02-006` não demonstra que “round-robin entre tenants” é útil, porque nunca define round-robin de verdade; hoje a expressão adiciona branding, não cobertura.
- A aritmética de custo está materialmente errada em `WI-S02-002` (`365B` D1 reads não são `$365/yr`) e placeholder em `WI-S02-004` (`$X TCO`), o que falha a barra pedida para produção.

## Missing Gaps for Production

- `BatchReadBlobs` é o maior gap protocolar. Se S-02 quer um `GetBlob` proprietária, isso precisa de ADR explícita e delta formal contra o product profile antes de qualquer implementação.
- Backpressure/cancelamento continuam subespecificados. `WI-S02-001` fala em memória bounded, mas os novos WIs não definem slow-consumer behavior, propagation de cancel e limites de paralelismo D1 sob overload.
- Circuit breaker segue só como nota forward-looking. Para falhas de D1/R2/KV, falta contrato explícito de fail-closed/fallback/open-reset thresholds.
- O fallback quando KV falha não subiu a acceptance/DoD hard gate. `WI-S02-005` menciona graceful fall-through em chaos, mas não como invariante com impacto numérico de SLO.
- TLS pinning client-side está ausente. Isso provavelmente é aceitável para Python/Go/JS/WASM, mas como `WI-S02-003` usa tampering em trânsito como parte do threat model, a ausência deveria ser uma rejeição explícita, não um silêncio.
- WebSocket support não é gap aqui. Para CAS read path REAPI/HTTP, deixar WebSocket fora do escopo está correto.

## Comparison vs WI-S02-001 Template

Os novos WIs não mantêm o nível do baseline `WI-S02-001`. `WI-S02-001` é mais longo, mas principalmente mais apertado: invariantes mapeiam melhor para controles/testes, STRIDE/LINDDUN está menos comprimido, PRR/sign-off é mais explícito, e o documento evita contradições graves com fontes canônicas herdadas. `WI-S02-003` e `WI-S02-004` chegam mais perto do baseline estruturalmente; o conjunto, porém, degrada porque `WI-S02-002` e `WI-S02-005` introduzem drift protocolar/arquitetural que o baseline não tinha.

## Recommendations

- `P0` Realinhar S-02 com as fontes canônicas: decidir se o produto suporta `BatchReadBlobs` obrigatório ou se `GetBlob` é extensão proprietária. Sem isso, reescrever `WI-S02-002` e o sprint contract.
- `P0` Reescrever `WI-S02-005` contra `REG-NEGATIVE-001/002`, `FM-054` e `PAT-KV-TTL-001`: KV never-source-of-truth, sem verdade negativa em KV, fallback obrigatório, suposições de consistência corrigidas.
- `P0` Unificar a semântica cross-tenant/tombstone entre `WI-S02-001/002/004/005` antes de implementar qualquer handler ou benchmark.
- `P1` Refatorar `WI-S02-003` para separar claramente “crate Rust API” de “FFI surface futura”: ou entrega crate Rust puro agora, ou define wrappers `extern "C"` e tipos FFI-safe depois em S-15.
- `P1` Reespecificar `WI-S02-004` evidence model: equivalence bounds/power analysis, classes observáveis pelo atacante e budget absoluto de latência, não só diff p99.
- `P1` Reescrever `WI-S02-006` como ship gate de verdade: 3 chaos experiments com abort criteria, round-robin definido se o termo permanecer, sign-off count fixo e sem overclaim estatístico.
- `P2` Corrigir cost analysis em todos os WIs para breakdown por request de Workers/R2/D1/KV e estimativas 12m numericamente corretas.
- `P2` Tornar backpressure, cancelamento e circuit breaker explícitos nos paths de read/batch, em vez de deixá-los como notas forward-looking.

## Final Verdict
**NO-GO**

S-02 não deve ser selado para implementação a partir destes cinco WIs no estado atual. O pacote parece completo na superfície, mas dois documentos (`WI-S02-002`, `WI-S02-005`) entram em conflito com fontes canônicas de protocolo/arquitetura, e os outros três ainda precisam de endurecimento real em contrato FFI, prova estatística e rigor operacional de ship gate. Se a implementação começar agora, a probabilidade alta é produzir código “conforme o WI” e não “conforme a arquitetura”.
