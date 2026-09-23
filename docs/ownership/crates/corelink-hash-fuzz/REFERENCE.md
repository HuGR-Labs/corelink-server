---
schema: corelink-ownership/1.1
document: reference
package: corelink-hash-fuzz
manifest: crates/corelink-hash/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: hash-fuzz-source-20260921
---

# corelink-hash-fuzz — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) ·
[Contratos](#r04) · [Estado e invariantes](#r05) · [Configuração](#r06) ·
[Falhas](#r07) · [Evidências](#r08).

<a id="r01"></a>
## R01 — Identidade e função

Package Cargo independente com dois bins cargo-fuzz para exercitar entrada do parser de digest e construção de VerifiedBody. Esta referência separa intenção do código e declaração de CI de qualquer execução observada.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | corelink-hash-fuzz / crates/corelink-hash/fuzz/Cargo.toml |
| Targets | Bins digest_parse e verify_body; ambos test=false, doc=false, bench=false |
| Papel | Harness auxiliar; não implementa digest, storage ou produto |
| Publish / licença | publish=false; UNLICENSED declarados |
| Workspaces | [workspace] próprio; root exclui crates/corelink-hash/fuzz |
| Perfil / features | S; nenhuma feature do package declarada |
| Evidência | Fonte package fixada em 1177dad2; integração de ownership parte de 8cdc0282 |
| implemented | sim; fonte dos dois targets e manifesto |
| wired | declarado; manifesto/workflows/scripts apontam seleção, sem execução |
| runtime_verified | UNKNOWN; snapshot histórico falhou antes dos targets e nenhum target foi observado |
| Snapshot | um job falhou antes dos targets; não é prova de runtime |

**OKF canônico:** [okf-context](../../../../.claude/skills/okf-context/SKILL.md) usa a tag `cas` e os arquivos `crates/corelink-hash/src/{lib.rs,digest.rs,verified_body.rs}`: `python3 scripts/okf_context.py --tag cas --full` e consultas `--file` nesses arquivos. O manifesto fuzz pode retornar no-concepts direto; isso não é ausência arquitetural. **Alcance:** [built-not-wired](../../../../.claude/skills/built-not-wired/SKILL.md) separa built, wired e runtime_verified.

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Dono da implementação | Dono do contrato | Estado / rota |
|---|---|---|---|
| Dois harnesses | corelink-hash-fuzz | Oráculo específico do harness | Package verificado; pessoa/equipe nominal UNKNOWN |
| Digest e VerifiedBody | corelink-hash | corelink-hash | Relação compartilhada REL-001 |
| Invocação e runner | Workflows/scripts do repositório | Integração do repositório | Execução real não observada |

| Camada de ownership | Dono/estado verificado | Limite |
|---|---|---|
| Implementação | corelink-hash-fuzz (package) | targets/manifesto |
| Contrato público | corelink-hash (package) | Digest/VerifiedBody |
| Composition root | N/A | não há composição de produto |
| Operador de runtime | UNKNOWN | runner/target não observado |
| Autoridade de revisão | UNKNOWN | cold reviewer independente exigido |

Este package não faz chamadas de serviço, tenant, auth, storage, banco, provider ou escrita de objetos; harnesses mantêm dados em memória. Rota de escalonamento/aprovação: issue/PR do repositório → owner do package afetado → cold reviewer independente. Owner nominal da rota: REQUIRED; identidade atual UNKNOWN; atribuição verificada é pré-condição de aprovação.

<a id="r03"></a>
## R03 — Mapa da implementação

| Entrada | Comportamento observado no fonte | Prova / limite |
|---|---|---|
| fuzz_targets/digest_parse.rs | Recebe &[u8]; bytes não UTF-8 retornam cedo; demais chamam Digest::from_hex; resultado é descartado | S03; não mede cobertura ou execução |
| fuzz_targets/verify_body.rs | Ignora input <32 bytes; primeiros 32 viram digest hex; restante é corpo em Bytes; chama compute/verificação/construtor e assertions | S04/S06/S07; corpo não é streaming |
| Cargo.toml | Metadata cargo-fuzz; dois bin targets; harness declara unsafe_code=forbid; lints Clippy restritivos | S01; boundary própria não prova lint/build/provider |
| Cargo.lock | Declara corelink-hash-fuzz, corelink-hash, bytes, libfuzzer-sys e lock transitivo | S02; lock não é resolução/runtime medido |

**Inventário semântico:** duas entradas próprias; não há src/lib.rs, API pública, storage adapter ou corpus rastreado nesta árvore. O description do manifesto diz “streaming”; verify_body.rs copia o corpo inteiro e não implementa streaming. A alegação de mismatch constante no tempo descreve o contrato do hash; o harness não mede tempo.

<a id="r04"></a>
## R04 — Contratos e superfícies chamadas

Não há API pública declarada por este package. Os contratos abaixo são oráculos internos dos bins, não estabilidade de biblioteca:

| Target | Entrada e chamada | Predicado codificado / limite |
|---|---|---|
| digest_parse | Bytes arbitrários que passam por from_utf8; Digest::from_hex(&str) | Processo não deve panic para inputs UTF-8 recebidos; inputs não UTF-8 não alcançam parser; Result não é afirmado |
| verify_body | Prefixo de 32 bytes é claim; sufixo é corpo; Digest::{from_hex,compute,verify_constant_time} e VerifiedBody::{new,body} | verify_constant_time(computed) deve ser true; igualdade comum decide o oráculo; match exige Ok e length igual; mismatch exige Err |

O mismatch branch testa somente is_err, não variante específica nem duração. O fluxo não percorre writer, envelope persistido, rota HTTP ou serviço. Compatibilidade externa vem de corelink-hash e consumidores, não do nome do fuzzer.

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado | Owner / chave | Persistência e efeito |
|---|---|---|
| Buffer de entrada | Harness/libFuzzer por invocação | Memória temporária; sem tenant ou identidade |
| Bytes do corpo | verify_body, sufixo após 32 bytes | Copiados para Bytes; sem write durável |
| Corpus/crash | Diretórios fuzz/corpus, fuzz/artifacts, fuzz/coverage ignorados pelo Git | Podem existir localmente; nenhuma cópia rastreada nesta árvore |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [FLOW-001](#flow-001).

<a id="inv-001"></a>
### INV-001 — Parser recebe somente entrada UTF-8

**Regra:** para bytes sem conversão UTF-8, digest_parse retorna antes de Digest::from_hex.
**Imposição:** std::str::from_utf8(data) em S03.
**Violação / prova:** um call path do parser para bytes não UTF-8 contradiz a condição; revisão estática, não fuzz executado. Entradas UTF-8 podem produzir Err; o alvo não inspeciona o valor.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — verify_body constrói o corpo do sufixo de 32 bytes

**Regra:** quando data.len() >= DIGEST_LEN, claim usa exatamente o prefixo [0..32], corpo usa o restante e falha de match deve ser Err.
**Imposição:** S04; VerifiedBody::new computa digest e usa comparação constante em S07.
**Violação / prova:** claim/body invertidos ou sucesso no mismatch violam o oráculo; source trace conferido, execução desconhecida.

[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Fluxo do alvo de corpo

1. O engine entrega bytes ao closure.
2. Com menos de DIGEST_LEN, o closure retorna.
3. Os 32 bytes iniciais são copiados para claim e hexificados.
4. O restante vira entrada de Digest::compute.
5. O alvo compara computed consigo e afirma o resultado constante.
6. VerifiedBody::new recebe uma cópia do corpo e a claim.
7. Match exige Ok e tamanho igual; mismatch exige somente Err.

Input é sintético do engine nesta intenção de código. Tempo e alvo executado são desconhecidos.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Nome / fonte | Default declarado | Leitura / condição | Efeito / limite |
|---|---|---|---|
| digest_parse | Bin em fuzz_targets/digest_parse.rs | cargo-fuzz seleciona nome | test/doc/bench=false; nenhuma execução implícita |
| verify_body | Bin em fuzz_targets/verify_body.rs | cargo-fuzz seleciona nome | Mesmo status; input depende do engine |
| libfuzzer-sys = 0.4 | Versão compatível declarada | Dependência normal | Versão lockada consta em S02; não medida por resolver |
| corelink-hash = path .. | Sem features explícitas | Dependência normal para ambos os bins | Contrato produzido pelo package pai |
| bytes = 1 | Versão compatível declarada | Usada em verify_body | Cópia completa do corpo |
| Ambiente/feature | Nenhuma env var/feature do package vista nos arquivos inspecionados | Seleção externa via CLI/workflow | Cargo metadata/feature resolver não executados |

O workflow de PR declara 60 s por alvo; nightly.yml declara ambos por 3600 s. S14 registra job antigo que falhou antes dos comandos PR, ambos skipped. Não há evidência de execução de target neste pin nem limite de memória por alvo demonstrado no código.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Sinal | Causa local | Estado após falha | Diagnóstico |
|---|---|---|---|
| Crash/panic do bin | Assertion/expect falha ou código chamado panica | Processo de fuzz falha; pode gerar input de crash | Preservar artifact local; PROC-003 |
| Bytes não UTF-8 / menos de 32 bytes | Guardas de entrada | Alvo retorna sem chamada correspondente | Não contar como cobertura de parser/corpo |
| from_hex retorna erro | Parser de digest | Digest target descarta Result; body tem retorno cedo se erro | Não afirmar que erros são afirmados |
| Workflow/job falha | Build, timeout ou alvo emite falha | Resultado do gate depende do job e do run | S14 é falha pré-target; não prova comportamento fuzz |

O harness não emite logs de produto nem observabilidade de serviço. Tempo limite, memória do engine, execução observada e taxa de cobertura não foram medidos.

<a id="r08"></a>
## R08 — Verificação e fontes

Fontes lidas no pin imutável 1177dad2ca2a9f21c29b5a118aa7944b77147798; as linhas SOURCE provam leitura. Baseline de integração de ownership: 8cdc02828132b9b6f03a3b57117b8140325f6762. As quatro entradas do package não diferem entre esses trees; isso não reconcilia outros arquivos.

| ID | Path / símbolo | Blob no source pin | Resultado observado |
|---|---|---|---|
| S01 | [Manifesto](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-hash/fuzz/Cargo.toml), package/deps/targets | b9f559c2b44629cb7c1d9a18ddc95b2aa38ecbb2 | SOURCE; declara duas bins e três deps normais |
| S02 | [Lockfile](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-hash/fuzz/Cargo.lock) | 1691ff5a7da00064f66b71ee811f3b4f51fbd0bd | SOURCE; conteúdo lido, resolução não executada |
| S03 | [Parser target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-hash/fuzz/fuzz_targets/digest_parse.rs), closure | 559311a0909fc1143d622f90995d0dcda969dad7 | SOURCE; filtro UTF-8 seguido de chamada |
| S04 | [Body target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-hash/fuzz/fuzz_targets/verify_body.rs), closure | 6585429b6a52ab1c8e924bd2cd9d0c59fb5aa631 | SOURCE; cópia integral e oráculos descritos |
| S05 | [Workspace](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/Cargo.toml), exclude | 801ee986c93374461d468ed6e643345e8c2ed8f1 | SOURCE; caminho do fuzzer excluído |
| S06 | [Digest](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-hash/src/digest.rs), from_hex/compute/verify | 40aea50c8127ada98133e3719171b65f047f2bcd | SOURCE; contrato provider, sem execução neste pacote |
| S07 | [VerifiedBody](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-hash/src/verified_body.rs), new/body | d387eca974b5314dad46b6e92b1623cb8b3e10d5 | SOURCE; corpo validado e cópia observados |
| S08 | [Hash exports](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-hash/src/lib.rs) | 403f7e2e4ef8c7bb30a47e7d7eee3ce94a785730 | SOURCE; exports públicos confirmados |
| S09 | [Workflow hash](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/.github/workflows/corelink-hash.yml), smoke/nightly | 2338eadfd8a64673bb295ec63cd921c11eabaa53 | SOURCE; YAML apenas |
| S10 | [Workflow nightly](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/.github/workflows/nightly.yml), matrix | 78362efe867634b7cdd028f98f5b486ffb4943c5 | SOURCE; YAML apenas |
| S11 | [Script fuzz-all](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/scripts/fuzz-all.sh), target list | e4dd689534c76c84554f54019f588fd440c11e59 | SOURCE; comando não executado |
| S12 | [Ignore rules](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-hash/fuzz/.gitignore) | 1a45eee7760d240bfa8ac1989088e0349169f264 | SOURCE; target/corpus/artifacts/coverage ignorados |
| S13 | [Manifesto provider](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-hash/Cargo.toml), package corelink-hash | bb0642df8d4a80b95f48eefd8d4339f851a4c8bd | SOURCE; endpoint provider confirmado |
| S14 | [Snapshot de steps](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/reports/perf/b152-actions-2026-09-06.json), run 33360411082/job 99393678387 | 61b4a00d63910bd687852daf55290bfe64c9edfd | OBSERVED_RUNTIME; job CI falhou na preparação nightly e os dois steps fuzz ficaram skipped |

**Não executado nesta autoria:** Cargo metadata/resolve/tree, build, testes, Clippy, fuzz, CI/GitHub, provider, produção ou storage. S14 é snapshot arquivado do controle CI, não execução dos targets; fonte selada também contém uma afirmação histórica de fuzz verde que não foi revalidada. Não observados: corpus rastreado, seleção efetiva de features, execução dos targets neste pin, crash artifacts, cobertura, owner nominal e runtime de produto. Continuar em [Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).

[Voltar ao início](#r01)
