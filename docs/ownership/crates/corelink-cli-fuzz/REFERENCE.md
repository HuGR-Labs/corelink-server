---
schema: corelink-ownership/1.1
document: reference
package: corelink-cli-fuzz
manifest: tools/cli/fuzz/Cargo.toml
source_commit: 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018
profile: "S"
state: draft
evidence_set: corelink-cli-fuzz-structural-normalization-20260921
---

# corelink-cli-fuzz — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) ·
[Contratos](#r04) · [Estado e invariantes](#r05) · [Configuração](#r06) ·
[Falhas](#r07) · [Verificação](#r08).

<a id="r01"></a>
## R01 — Identidade e função

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005) · [FLOW-001](#flow-001) · [FLOW-002](#flow-002) · [FLOW-003](#flow-003)

`corelink-cli-fuzz` é um package Cargo independente em `tools/cli/fuzz`, excluído
do workspace raiz por possuir `[workspace]` próprio. Ele contém cinco binaries de
harness que exercitam funções puras expostas por `corelink-cli`; não é a CLI
distribuída nem um serviço. A fonte foi lida no commit `38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018`; Cargo, fuzz e
runtime não foram executados neste trabalho.

| Campo | Valor verificado |
|---|---|
| Package / manifesto | `corelink-cli-fuzz` / `tools/cli/fuzz/Cargo.toml` |
| Targets | `cli_input`, `config_toml`, `json_deserialize`, `auth_resolution`, `secret_redaction_check` |
| Papel | Harness independente, cinco bins, `publish=false`, edition 2021 |
| Dependência local | `corelink-cli = { path = ".." }` |
| Implementação / wiring / runtime | harness e chamadas implementados / seleção por script-workflow parcial / runtime não observado |

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Dono da implementação | Dono do contrato | Operação / escalonamento |
|---|---|---|---|
| Cinco harnesses e manifesto | `corelink-cli-fuzz` | package fuzz e contrato WI-S15-006 | owner do package, rota nominal não verificada |
| `fuzz_api`, `CorelinkConfig`, auth e erros | `corelink-cli` em `tools/cli` | `corelink-cli` e specs S15/CTRL-CRED-001 | coordenar mudança no pai |
| CI, corpus e execução cargo-fuzz | workflow/script que selecionar o target | manutenção CI, ainda não atribuída nesta fonte | autorização de runner necessária |

**Não faz:** implementação da CLI, validação criptográfica server-side, rede,
filesystem persistente durante os helpers, deploy, release, provider ou cliente.
`corelink-cli::auth::resolve_pat` e `config::load/save` têm I/O, mas os targets
usam `validate_pat_shape` e `fuzz_api` filesystem-free. Conceitos OKF aplicáveis:
disciplina de onboarding/evidence e distinção implementation/wiring/runtime; o
perfil OKF em `docs/internal/okf-wiki/` permanece canônico.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo / entradas | Papel e dado sob ownership | Natureza | Contratos / evidências |
|---|---|---|---|
| `fuzz_targets/cli_input.rs` | bytes arbitrários divididos deterministicamente em `key`/`value`; aplica config e valida PAT | harness próprio, não parser de argv | API-002/API-003; blob `86f04eb5` |
| `fuzz_targets/config_toml.rs` | bytes TOML, probes dotted-key, serialização | harness próprio | API-001/API-002; blob `1c4453b2` |
| `fuzz_targets/json_deserialize.rs` | UTF-8 JSON para `CorelinkConfig` e `serde_json::Value` | harness próprio; não testa formatter/schema de saída | API-005; blob `dd9b5892` |
| `fuzz_targets/auth_resolution.rs` | bytes UTF-8 e fallback vazio para forma PAT | harness próprio | API-003; blob `eb6d2e2d` |
| `fuzz_targets/secret_redaction_check.rs` | captura somente `ConfigError` de TOML/key dispatch e conta PAT-shaped | harness próprio, não stderr geral | API-004; blob `bf86d9a6` |
| `Cargo.toml` / `Cargo.lock` | bins, deps, lints e resolução pinada; lockfile registra `corelink-cli` 0.1.2 no source pin atual | declaração/wiring | REL-001/REL-008; blobs `239da43b`/lockfile |

**Inventário:** cinco fontes de target, um manifesto, um lockfile, uma dependência
local parent e as dependências declaradas `libfuzzer-sys`, `arbitrary`, `toml` e
`serde_json`.
Embora `arbitrary` esteja declarado, nenhum dos cinco sources importa `arbitrary`
ou deriva `Arbitrary`; a geração é a fatia `&[u8]` fornecida pelo libFuzzer.
Não há corpus ou artifacts versionados comprovados neste package.

<a id="r04"></a>
## R04 — Contratos públicos

**Índice:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) ·
[API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006).

<a id="api-001"></a>
### API-001 — parse_config_toml

**Símbolos exatos:** `corelink_cli::fuzz_api::parse_config_toml(data: &[u8]) -> Result<CorelinkConfig, ConfigError>`.
**Entrada / pré-condição:** bytes arbitrários; TOML válido precisa ser UTF-8.
**Saída / efeitos:** `CorelinkConfig` deserializado; sem filesystem no helper.
**Erros / compatibilidade:** bytes não UTF-8 viram `ConfigError::UnknownKey("<non-utf8>")`; TOML inválido vira `ConfigError::Parse`.
**Vínculos e prova:** INV-001/003, FLOW-002, [REL-002](BLAST_RADIUS.md#rel-002),
`tools/cli/src/lib.rs:61-69`.

[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — apply_config_key

**Símbolos exatos:** `corelink_cli::fuzz_api::apply_config_key(cfg: &mut CorelinkConfig, key: &str, value: &str) -> Result<(), ConfigError>`.
**Entrada / pré-condição:** config em memória, caminho dotted e valor textual.
**Saída / efeitos:** muta campos fixos ou tabela `observability`; rejeita key/type inválido.
**Erros / compatibilidade:** `ConfigError::UnknownKey`; a implementação é `config::apply_key_to_cfg`.
**Vínculos e prova:** INV-001/005, FLOW-002, [REL-003](BLAST_RADIUS.md#rel-003),
`tools/cli/src/lib.rs:72-80`, `config.rs:228-247`.

[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — validate_pat_shape

**Símbolos exatos:** `corelink_cli::fuzz_api::validate_pat_shape(pat: &str) -> Result<(), CliError>`.
**Entrada / pré-condição:** string candidata a PAT.
**Saída / efeitos:** aceita somente forma validada por `corelink_pat::parse_plaintext`; não faz verificação criptográfica ou rede.
**Erros / compatibilidade:** parser falho é `CliError::PatMalformed`; bytes inválidos só chegam como string UTF-8 do harness.
**Vínculos e prova:** INV-002, [REL-004](BLAST_RADIUS.md#rel-004),
`tools/cli/src/auth.rs:35-40` e `lib.rs:84-87`.

[Índice de contratos](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — count_pat_leaks

**Símbolos exatos:** `corelink_cli::fuzz_api::count_pat_leaks(haystack: &str) -> usize`.

**Entrada / pré-condição:** texto de diagnóstico; pode conter Unicode.

**Saída / efeitos:** percorre ocorrências `corelink_`, tenta comprimentos 95/96 sem cortar fronteira UTF-8 e conta matches aceitos por API-003.

**Erros / compatibilidade:** não retorna `Result`; o target exige zero. O
consumer só entrega mensagens `Display` de `ConfigError` vindas de
`parse_config_toml` e `apply_config_key`; não captura `CliError`,
`resolve_pat`, filesystem, rede, formatter ou stderr da CLI. A contagem não
prova que todo segredo possível foi detectado.
**Vínculos e prova:** INV-004, FLOW-003, [REL-005](BLAST_RADIUS.md#rel-005),
`tools/cli/src/lib.rs:96-122`.

[Índice de contratos](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — CorelinkConfig serializável

**Símbolos exatos:** `config::CorelinkConfig`, `AuthConfig`, `DefaultsConfig`, `TelemetryConfig`; campos `auth`, `defaults`, `telemetry`, `observability`.

**Entrada / pré-condição:** serde TOML/JSON; `TelemetryConfig::default` usa `enabled=false` e UUID v4 não determinístico.

**Saída / efeitos:** tipo não exaustivo; cada `CorelinkConfig::default()` gera novo UUID, portanto defaults não são determinísticos; round-trip preserva uma configuração.

**Erros / compatibilidade:** `toml`/`serde_json`; JSON não é API schema versionado. `json_deserialize` aceita JSON e serializa valores locais;
não chama formatter nem prova o schema de `--output=json`.
**Vínculos e prova:** INV-005, [REL-006](BLAST_RADIUS.md#rel-006),
`tools/cli/src/config.rs:24-59`; o `serde_json::Value` parseado no target é
descartado e não representa uma saída publicada.

[Índice de contratos](#r04)

<a id="api-006"></a>
[↩](#r01)
### API-006 — target entrypoints

**Símbolos exatos:** cinco invocações `fuzz_target!` em `cli_input`, `config_toml`, `json_deserialize`, `auth_resolution`, `secret_redaction_check`.
**Entrada / pré-condição:** libFuzzer fornece `&[u8]`; os bins têm `test=false`, `doc=false`, `bench=false`.
**Saída / efeitos:** assertions podem abortar o processo; não há output/runtime observado nesta revisão.
**Erros / compatibilidade:** cada assertion é um oráculo do harness, não uma prova da implementação pai.
**Vínculos e prova:** FLOW-001–003, [REL-001](BLAST_RADIUS.md#rel-001),
[REL-002](BLAST_RADIUS.md#rel-002), [REL-003](BLAST_RADIUS.md#rel-003),
[REL-004](BLAST_RADIUS.md#rel-004), [REL-005](BLAST_RADIUS.md#rel-005),
[REL-006](BLAST_RADIUS.md#rel-006),
manifesto linhas 30–64.

[Índice de contratos](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado / recurso | Chave e owner | Persistência / vida útil | Escrita / leitura / durabilidade |
|---|---|---|---|
| input libFuzzer | processo do target / harness | memória durante uma iteração | leitura pelos slices; sem persistência comprovada |
| `CorelinkConfig` | objeto local / `corelink-cli` | vida da iteração | mutação por API-002, serialização opcional |
| captured error text | `String` local / redaction target | FLOW-003 | Display em memória, varredura API-004 |
| corpus/artifact | diretório cargo-fuzz se configurado / operador CI | desconhecido nesta fonte | não há corpus versionado observado |

**Índice:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) ·
[INV-004](#inv-004) · [INV-005](#inv-005) · [FLOW-001](#flow-001) · [FLOW-002](#flow-002) · [FLOW-003](#flow-003).

<a id="inv-001"></a>
### INV-001 — entradas arbitrárias não causam panic no harness

**Regra:** para cada input `&[u8]` fornecido a um target, o caminho deve retornar
ou cumprir o oráculo sem panic não esperado. **Imposição:** guards UTF-8,
`Result` ignorado intencionalmente e o abortamento normal de libFuzzer; não há
uma assertion explícita de “no panic” nos cinco sources. As assertions explícitas
atuais cobrem apenas determinismo em `cli_input`/`auth_resolution` e leak count
em `secret_redaction_check`. **Violação / prova:** crash, abort ou timeout
reproduzível; nenhuma execução foi feita nesta autoria, portanto estado atual é
desconhecido.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — validação PAT é determinística

**Regra:** duas chamadas `validate_pat_shape(value).is_ok()` para o mesmo `value` produzem o mesmo booleano.
**Imposição:** chamadas duplicadas em `cli_input.rs:49-50` e `auth_resolution.rs:26-27`.
**Violação / prova:** resultados divergentes no mesmo processo; teste/fuzz não executado neste commit.

[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — input não UTF-8 é encerrado sem parser de string

**Regra:** bytes não UTF-8 não devem ser convertidos por slicing inseguro no target; o fluxo retorna ou usa fallback vazio.
**Imposição:** `str::from_utf8` guard em `cli_input`, `json_deserialize` e
`auth_resolution`; fallback `""` em auth. `cli_input` não modela argv, flags,
subcomandos nem usa `arbitrary::Arbitrary`: ele escolhe um comprimento a partir
do primeiro byte e passa duas strings ao dispatch de configuração.
**Violação / prova:** panic ou chamada com string corrompida; verificação é estática até execução isolada.

[Índice de estado](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — redaction target não aceita PAT-shaped leak

**Regra:** `count_pat_leaks(captured) == 0` após os erros Display coletados.
**Imposição:** `assert_eq!` em `secret_redaction_check.rs:49-52` e scanner em API-004.
**Violação / prova:** contador positivo; a cobertura depende das mensagens e entradas realmente geradas, ainda não observadas.

[Índice de estado](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — round-trip só é alegado para config parseada com sucesso

**Regra:** quando TOML/JSON produz `CorelinkConfig`, a serialização deve completar sem panic no target.
**Imposição:** branches `if let Ok` e `Result` ignorado nos targets; não há assert de igualdade.
**Violação / prova:** panic ou erro inesperado de serialização; sem execução nesta autoria. `toml` do harness é 0.8, enquanto o pai declara 1.1, devendo ser reconciliado antes de afirmar equivalência.

[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — bytes até target

1. libFuzzer fornece `&[u8]` ao `fuzz_target!`; o crate `arbitrary` declarado não
   é usado.
2. O target aplica guard UTF-8 ou fatias determinísticas; `cli_input` divide os
   bytes em key/value e não reconstitui argv ou subcomandos.
3. Chama API-001–005 conforme o bin; `json_deserialize` também tenta
   `serde_json::Value`, mas descarta esse resultado.
4. Uma assertion pode terminar o processo; essa saída é artefato de falha, não runtime da CLI.

[Índice de estado](#r05)

<a id="flow-002"></a>
[↩](#r01)
### FLOW-002 — config TOML e probes

1. API-001 tenta parsear bytes como UTF-8/TOML.
2. Em sucesso, `value` é string UTF-8 ou vazio.
3. O target aplica seis keys, inclusive `nonexistent.key`.
4. Tenta serializar e reparsear; filesystem não é usado.

[Índice de estado](#r05)

<a id="flow-003"></a>
[↩](#r01)
### FLOW-003 — redaction de mensagens

1. API-001 e API-002 recebem bytes/string do input.
2. Somente os `ConfigError` retornados por esses dois helpers são formatados em
   `captured`; auth, output e stderr da CLI ficam fora do escopo.
3. API-004 procura PATs com prefixo e comprimentos canônicos.
4. O target exige contador zero; falha local deve preservar input e saída.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Nome / fonte | Default efetivo | Quando é lido | Target / condição | Efeito / falha |
|---|---|---|---|---|
| `[workspace]` próprio | independente | resolução Cargo | todo package | não entra automaticamente no workspace raiz |
| `cargo-fuzz=true` | declarado | tooling | package metadata | intenção de harness; não prova instalação/execução |
| cinco `[[bin]]` | exatamente os cinco listados | build/seleção do target | cada path | `test/doc/bench=false`; sem target extra comprovado |
| `unsafe_code=forbid` | forbid no harness | compilação | package | policy do harness, não do pai/deps |
| `corelink-cli` path `..` | pai local | resolução | todos os bins | contrato depende do source parent |
| `libfuzzer-sys=0.4` | declarado direto | build/execução | cinco bins | toolchain e runner precisam ser rechecados |
| `arbitrary=1, features=["derive"]` | feature da dependência; não há uso `Arbitrary` no source | resolução Cargo | manifesto | não confundir com `[features]` do package |
| `toml=0.8` / `serde_json=1` | declarados diretos | target config/json | config/json bins | versões e resolução precisam ser rechecadas antes de execução |

Não há `[features]` próprias no manifesto; `arbitrary` apenas solicita a feature
de dependência `derive`, sem uso observado no source. O comando proposto para futura validação
isolada é `cd tools/cli/fuzz && cargo fuzz run <target> -- -max_total_time=60`,
mas não foi executado.

`scripts/fuzz-all.sh` ainda lista `corelink-cli:<target>` e faz
`pushd crates/${crate}`; esse path é incompatível com `tools/cli/fuzz` e deve
ser corrigido ou excluído antes de ser tratado como wiring.

O rustdoc em `tools/cli/src/lib.rs:16` aponta o caminho antigo
`specs/04_sprints/S15/...`; a fonte existente é
`specs/04_sprints/_sealed/S15/work_items/WI-S15-006-...md`. O WI-S15-006 usa o
path genérico `fuzz/fuzz_targets/`; isso é contexto histórico, não prova do
path efetivo deste package.

<a id="r07"></a>
## R07 — Erros e observabilidade

| Sinal / erro | Causa no contrato | Estado após falha | Diagnóstico / procedimento |
|---|---|---|---|
| `ConfigError::Parse` | TOML inválido em API-001 | iteração continua se branch tratar erro | PROC-003; preservar input |
| `CliError::PatMalformed` | forma PAT rejeitada | resultado local, sem rede | API-003; não registrar segredo |
| `assert_eq` de determinismo | comportamento divergente | processo do target aborta | PROC-004; guardar input/artifact |
| `CTRL-CRED-001 violation` | contador PAT positivo | target aborta; leak pode estar em mensagem | PROC-004 e revisão de redaction |
| timeout/OOM do runner | limite de execução/ambiente | execução interrompida; causa não atribuída | owner do runner, não inferir bug |

Não há métricas, logs de produção, endpoint ou storage próprios. Fuzz output,
corpus e crash artifacts exigem registro de ambiente e redaction; inexistência de
artifact nesta fonte é `unknown`, não sucesso.

<a id="r08"></a>
## R08 — Verificação e evidências

| API / INV / FLOW | Fonte e revisão | Teste / método | Resultado e limite |
|---|---|---|---|
| identidade/targets | manifesto blob `239da43b`, source pin | leitura Git | confirmado estaticamente; Cargo não executado |
| API-001–006 / INV-001–005 | blobs `04c46ce4`, `358f3248`, `2a305fe2`, targets acima | inspeção de símbolos/linhas | implementação e assertions localizadas; comportamento desconhecido |
| wiring script | `scripts/fuzz-all.sh:27-34,48-51` | busca estática | referências `corelink-cli` registradas, mas `crates/${crate}` é stale para `tools/cli/fuzz`; execução não observada |
| parent inverse consumers/tests | `commands/{login,config_cmd,whoami}.rs`, `main.rs`, `src/lib.rs:123-167`, `tests/integration.rs` | busca estática | login/config consumers e testes localizados; nenhuma execução verificada |
| S15 contract inventory | `_spec_contract.md`, `sprint.md`, `WI-S15-006`, `PRR-S15.md` | leitura Git | referências reconciliadas; nenhum gate executado |
| CI/nightly | `.github/workflows/fuzz-nightly.yml` | busca estática | matriz não inclui CLI; nenhum run verificado |
| estrutura documental | checker CO-1 | comando descrito em M04 | resultado será registrado no handoff; não é cold approval |

**Desconhecidos e contradições:** resolução locked, inversas completas, corpus,
targets efetivamente selecionados, execução local/CI, runtime, owner operacional
e compatibilidade TOML 0.8/1.1 não foram observados. Continuar em
[Blast radius](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01).

[Voltar ao início](#r01)
