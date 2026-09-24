---
schema: corelink-ownership/1.1
document: reference
package: e2e-byok-revoke
manifest: tests/e2e-byok-revoke/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-byok-revoke-static-source-20260921
---

# e2e-byok-revoke — referência de ownership

[Identidade](#r01) · [Fronteiras](#r02) · [Implementação](#r03) · [Contratos](#r04) · [Estado](#r05) · [Configuração](#r06) · [Falhas](#r07) · [Verificação](#r08)

<a id="r01"></a>
## R01 — Identidade e função

`tests/e2e-byok-revoke/Cargo.toml` declara o package `e2e-byok-revoke`, library
`src/lib.rs` e oito `[[test]]` explícitos. Seu papel é harness de teste; o nome
Cargo, não o diretório, fixa a identidade. `publish = false`; versão, edition,
rust-version, licença e lints são herdados do workspace. O manifesto declara dois
first-party deps normais, quatro deps normais de workspace e `proptest` como dev
dep. Não há tabela `[features]`, build script, binário, example ou bench declarado.

| Campo | Declaração examinada |
|---|---|
| Package / manifesto | `e2e-byok-revoke` / `tests/e2e-byok-revoke/Cargo.toml` |
| Targets | 1 library + 8 integration tests nomeados |
| Natureza | harness local; nenhum target de produto declarado |
| Implementação / wiring / runtime | SOURCE local; composição de produção e runtime UNKNOWN |

Fonte: `Cargo.toml` `[package]`, `[lib]`, `[dependencies]`, `[dev-dependencies]`, `[[test]]`; revisão em `1177dad2ca2a9f21c29b5a118aa7944b77147798`. O pin histórico `cb94e251c0f17382565bf863f517945cbb2a84d6` permanece equivalente para os arquivos do package examinados; o pin atual também reconcilia o lockfile/workflows. Nenhum comando Cargo foi executado nesta autoria.

<a id="r02"></a>
## R02 — Fronteiras e ownership

| Superfície | Dono da implementação | Dono do contrato consumido | Rota verificada |
|---|---|---|---|
| `src/helpers.rs` e oito alvos | `e2e-byok-revoke` | tipos próprios do harness | regra geral `* @gmhelmold` em `.github/CODEOWNERS` |
| `KmsProvider`, cache e revocation API | `corelink-byok` | `corelink-byok` | regra `/crates/corelink-byok/ @gmhelmold`; [referência](../corelink-byok/REFERENCE.md#r01) |
| `MultiChannelAlerter` | `corelink-ops` | `corelink-ops` | regra geral `* @gmhelmold`; [referência](../corelink-ops/REFERENCE.md#r01) |

O manifesto depende diretamente dos dois packages internos acima. O harness
reimplementa `KillSwitchRunner` e `InMemoryTenantStatusStore`; ele não transfere
implementação nem autoridade operacional de `corelink-byok` ou `corelink-ops`.
Não há composition root de produção neste package. Provider real, persistência,
owner de integração externo e estado do cliente ficam fora desta fronteira.

**Não faz:** KMS real; detector ativo; D1/audit durável; canal customer; rollout.
**Escalonamento:** `@gmhelmold` é a rota observada em `.github/CODEOWNERS`; não é
confirmação de disponibilidade nem autorização de alteração/produção.
**Contexto canônico:** [perfil OKF verificado](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md), somente como rota.

<a id="r03"></a>
## R03 — Mapa da implementação

| Módulo / entradas | Papel local | Natureza | Contratos / evidências |
|---|---|---|---|
| `src/lib.rs` | `forbid(unsafe_code)`, deny docs/debug; declara `helpers` e reexports parciais | raiz local | API-001 |
| `src/helpers.rs` (1 módulo semântico) | fake KMS, sink de eventos, alerter stub, store em memória, bundle, runner e fixtures | owned harness | API-002–API-015; INV-001–004 |
| `tests/happy_revoke_flow.rs`, `tests/recovery_flow.rs` | revoke/restore em fake e detector smoke | test source | FLOW-001/002; NOT_EXECUTED |
| `tests/adversarial_stampede.rs`, `tests/adversarial_race_condition.rs` | cache/race locais | test source | INV-002/003; NOT_EXECUTED |
| `tests/adversarial_transient_api_error.rs`, `tests/prop_fail_closed.rs` | transient e propriedade | test source | INV-003; NOT_EXECUTED |
| `tests/multi_provider_matrix.rs`, `tests/live_provider_gated.rs` | quatro rótulos de provider; corpos “live” usam fake | test source | R06; NOT_EXECUTED |

Inventário estático: 2 arquivos `src/`, 8 paths de alvo, nenhum `build.rs`, binário,
example ou bench declarado. `corelink-byok` e `corelink-ops` são contratos importados;
suas implementações não pertencem a este mapa. Os comentários da descrição do
manifesto e dos testes são intenção textual, não prova de wiring.

<a id="r04"></a>
## R04 — Contratos públicos

Índice: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) · [API-008](#api-008) · [API-009](#api-009) · [API-010](#api-010) · [API-011](#api-011) · [API-012](#api-012) · [API-013](#api-013) · [API-014](#api-014) · [API-015](#api-015)

<a id="api-001"></a>
### API-001 — Root e reexports

**Símbolos exatos:** `pub mod helpers`; root reexports somente `setup_byok_env`, `AuditSink`, `BoundedKmsProvider`, `CustomerAlertSink`, `ExpectedRevocationEvent`, `KmsBehaviour`, `RevokeBundle`, `RevokeError`. `KillSwitchRunner`, `InMemoryTenantStatusStore`, constants e `make_wrapped_for` ficam sob `helpers::`. **Contrato/erro:** rename/remove quebra import; root não reexporta o módulo inteiro. **Imposição/prova:** `src/lib.rs:56–65`; REL-032.

[Índice de contratos](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — `KmsBehaviour`

**Símbolos exatos:** `AlwaysOk`, `AlwaysRevoked`, `AlwaysThrottled`, `AlwaysApiError(u16)`, `Scripted(Vec<KmsAccessStatus>)`; derives `Clone, Debug`; `#[non_exhaustive]`. **Contrato:** configura status fixo/código ou fila; `Scripted` remove primeiro item e fila vazia dá `Ok`. **Erro/efeito:** sem operação provider. **Imposição/prova:** `BoundedKmsProvider::check_access`, `src/helpers.rs:124–143,341–367`; INV-001, REL-002,010–025.

[Índice de contratos](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Controles do fake KMS

**Símbolos/assinaturas:** `new(KmsProviderKind,&str,FipsLevel,KmsBehaviour)->Self`; `provider_kind(&self)->KmsProviderKind`; `region(&self)->&str`; `fips_level(&self)->FipsLevel`; `set_behaviour(&self,KmsBehaviour)->()`; `set_deny_envelope(&self,bool)->()`; `check_access_count(&self)->usize`; `canonical_key_id(&self)->KmsKeyId`. Os três primeiros são os métodos iniciais da implementação pública `KmsProvider for BoundedKmsProvider`; os demais são controles do fixture. **Contrato:** cria fake/ID, troca mutex ou consulta; poison recupera inner. Campos privados; nenhum adapter real. **Prova:** `src/helpers.rs:158–259`; INV-001, REL-002,010–025.

[Índice de contratos](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — `KmsProvider::wrap_dek` fake

**Implementação pública:** `#[async_trait] impl KmsProvider for BoundedKmsProvider`; assinatura `async fn wrap_dek(&self,dek:&Dek,key_id:&KmsKeyId,encryption_context:Option<&serde_json::Value>)->Result<WrappedDek,BYOKError>`. **Pré/efeito:** denial falso; ciphertext 32 bytes XOR digest local e context clonada. **Erro:** `CmkRevoked` sob denial. Fake intencional; inseguro para produção. **Prova:** `src/helpers.rs:261–302`; REL-002,018–025.

[Índice de contratos](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — `KmsProvider::unwrap_dek` fake

**Implementação pública intencional:** `#[async_trait] impl KmsProvider for BoundedKmsProvider`; assinatura `async fn unwrap_dek(&self,wrapped:&WrappedDek)->Result<Dek,BYOKError>`. **Pré/efeito:** denial falso e ciphertext length 32; DEK recuperado por XOR local. **Erros:** `CmkRevoked` sob denial; `DekLengthInvalid{got}` noutro comprimento. Fixture, não KMS/crypto-sovereignty. **Prova:** `src/helpers.rs:304–339`; REL-002,018–025.

[Índice de contratos](#r04)

<a id="api-006"></a>
[↩](#r01)
### API-006 — `KmsProvider::check_access` fake

**Implementação pública:** `#[async_trait] impl KmsProvider for BoundedKmsProvider`; assinatura `async fn check_access(&self,key_id:&KmsKeyId)->Result<KmsAccessStatus,BYOKError>`. **Saída/efeito:** registra ID e retorna status fixo/scriptado; fila vazia dá `Ok`; body sempre retorna `Ok(status)` e nunca `Err`; poison recupera mutex. Fake intencional, sem provider real. **Prova:** `src/helpers.rs:341–367`; INV-001, REL-002,018–025.

[Índice de contratos](#r04)

<a id="api-007"></a>
[↩](#r01)
### API-007 — `AuditSink`

**Assinaturas/derives:** `#[derive(Debug,Default,Clone)]`; `new()->Self` chama `Self::default()`; `record(&self,event:RevocationAuditEvent)->()`; `snapshot_event_types(&self)->Vec<String>`; `snapshot(&self)->Vec<RevocationAuditEvent>`; `len(&self)->usize`; `is_empty(&self)->bool`. **Efeito/erro:** append/clones ordenados em `Arc<Mutex<Vec<_>>>`; poison recupera inner; sem `Result`, disco ou outbox. **Prova:** `src/helpers.rs:394–450`; FLOW-001/002, REL-010–017,032.

[Índice de contratos](#r04)

<a id="api-008"></a>
[↩](#r01)
### API-008 — `CustomerAlertSink`

**Assinaturas/impl:** `#[derive(Debug)]`; `new()->Self`; `Default::default()` chama `new()`. Helpers `alert_count(&self)->usize`, `recovery_count(&self)->usize`, `alerts(&self)->Vec<RevocationAlertPayload>`. Implementação pública intencional `#[async_trait] impl CustomerAlerter for CustomerAlertSink`: `async fn alert(&self,payload:RevocationAlertPayload)->Result<(),RevocationError>`; `async fn alert_recovery(&self,provider:KmsProviderKind,kms_key_id:&KmsKeyId,tenant_id_hashed:&str,restored_at_ms:u64)->Result<(),RevocationError>`. **Efeito/erro:** alerter stub primeiro; erro propaga antes do append local. **Prova:** `src/helpers.rs:459–551`; FLOW-001/002, REL-003/009.

[Índice de contratos](#r04)

<a id="api-009"></a>
[↩](#r01)
### API-009 — `InMemoryTenantStatusStore`

**Derives/construct:** `#[derive(Debug,Default)]`; `Default::default()` é caminho direto de construção (não há `new`). Helpers `current(&self,key_arn_or_id:&str)->Option<TenantByokStatus>`, `history(&self)->Vec<(String,TenantByokStatus,u64)>`. Implementação pública intencional `#[async_trait] impl TenantStatusStore for InMemoryTenantStatusStore`: `async fn mark_degraded(&self,kms_key_id:&KmsKeyId,_provider:&str,revoked_at_ms:u64)->Result<usize,RevocationError>`; `async fn restore_active(&self,kms_key_id:&KmsKeyId,restored_at_ms:u64)->Result<usize,RevocationError>`; `async fn current_status(&self,kms_key_id:&KmsKeyId)->Result<Option<TenantByokStatus>,RevocationError>`. **Efeito:** map/history em memória; methods retornam `Ok`, count 1, sem persistência. **Prova:** `src/helpers.rs:563–647`; FLOW-001/002, REL-002,010–025.

[Índice de contratos](#r04)

<a id="api-010"></a>
[↩](#r01)
### API-010 — Bundle e construtor de ambiente

**Símbolos/tipos:** `RevokeBundle` público `#[non_exhaustive]` com `provider:Arc<BoundedKmsProvider>`, `dek_cache:Arc<DekCache>`, `tenant_store:Arc<InMemoryTenantStatusStore>`, `audit:AuditSink`, `alert:Arc<CustomerAlertSink>`, `key_id:KmsKeyId`, `tenant_id_hashed:String`; `setup_byok_env(KmsProviderKind)->RevokeBundle`. **Efeito:** region/FIPS label fixo, AlwaysOk, TTL 300, memory/stub; `expect` pode panic. **Imposição/prova:** `src/helpers.rs:653–730`; INV-002, REL-010–017.

[Índice de contratos](#r04)

<a id="api-011"></a>
[↩](#r01)
### API-011 — `KillSwitchRunner::run`

**Assinatura:** `pub async fn run(bundle:&RevokeBundle)->Result<Option<RevocationAuditEvent>,RevokeError>`. **Efeito:** ciclo local; `Some(event)` em revoke/recovery, `None` em Ok sem degrade/transient; muta cache/status/audit/alert na ordem FLOW. **Erros:** eviction/store/SLA/alert podem propagar sem rollback de efeitos anteriores; fake `check_access` retorna `Ok(status)`. **Prova:** `src/helpers.rs:750–861`; FLOW-001/002, INV-003/004, REL-009–025.

[Índice de contratos](#r04)

<a id="api-012"></a>
[↩](#r01)
### API-012 — `ExpectedRevocationEvent`

**Símbolos:** `CmkRevoked`, `CmkRestored`; derives `Clone, Copy, Debug, PartialEq, Eq`; enum `#[non_exhaustive]`. **Entrada/saída:** discriminadores para testes de `RevocationAuditEvent::event_type`; não produz nem entrega eventos. **Erro/efeito:** nenhum. **Prova:** `src/helpers.rs:75–87`; FLOW-001/002, REL-002.

[Índice de contratos](#r04)

<a id="api-013"></a>
[↩](#r01)
### API-013 — `RevokeError`

**Símbolos:** `Byok(BYOKError)`, `Revocation(RevocationError)`, `SlaViolated{observed_ms,bound_ms}`, `InvariantViolated(String)`; derives `Debug` e `std::error::Error` via `thiserror`. **Saída/efeito:** enum local `#[non_exhaustive]`; `From` para os wrappers. **Unidade:** SLA em ms. **Prova:** `src/helpers.rs:93–118`; API-011/INV-004. A variante `InvariantViolated` não prova construção pelo runner.

[Índice de contratos](#r04)

<a id="api-014"></a>
[↩](#r01)
### API-014 — `make_wrapped_for`

**Símbolos:** `make_wrapped_for(&KmsKeyId,u32)->WrappedDek`. **Entrada/efeito:** key e discriminador; ciphertext fixture 32 bytes com discriminador nos quatro primeiros e context JSON. **Erro:** nenhum; não executa wrap/provider. Sob `helpers::`, não root. **Imposição/prova:** `src/helpers.rs:886–907`; imports locais REL-011,013,014.

[Índice de contratos](#r04)

<a id="api-015"></a>
[↩](#r01)
### API-015 — Constantes de fixture e gate

**Símbolos/assinaturas/valores:** `KILL_SWITCH_SLA_MS:u64=30_000`; `KILL_SWITCH_DETECTION_BOUND_MS:u64=60_000`; `ALL_PROVIDER_KINDS:&[KmsProviderKind]` = `[AwsKms,GcpKms,AzureKeyVault,HashicorpVault]` nesta ordem; `ENV_AWS_TEST_KEY_ARN:&str="AWS_TEST_KEY_ARN"`; `ENV_GCP_TEST_KEY_RESOURCE:&str="GCP_TEST_KEY_RESOURCE"`; `ENV_AZURE_TEST_KEY_RESOURCE:&str="AZURE_TEST_KEY_RESOURCE"`; `ENV_VAULT_TEST_KEY_NAME:&str="VAULT_TEST_KEY_NAME"`. **Contrato:** constantes em `helpers::`, não root; slice é matriz canônica local; nomes de env identificam gates documentais, não credencial/operação live. **Imposição/prova:** `tests/e2e-byok-revoke/src/helpers.rs:64–69,910–924`; R06, REL-010–017.

[Índice de contratos](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Estado, fluxos e invariantes

| Estado/recurso | Chave e owner | Vida útil/durabilidade | Leitura/escrita |
|---|---|---|---|
| Status tenant fixture | `key_arn_or_id`; `InMemoryTenantStatusStore` | map e vetor em memória | `mark_degraded`, `restore_active`, snapshots |
| Auditoria fixture | sink local `AuditSink` | `Arc<Mutex<Vec<_>>>`; sem disco | append após montagem do evento |
| Provider fake | provider kind/key; `BoundedKmsProvider` | mutexes/Vec locais | status, chamada count, denial de envelope |
| Auditoria: wall clock | `now_ms()` | ms Unix epoch por ciclo | `SystemTime`; underflow vira zero; `helpers.rs:868–875` |
| SLA: elapsed clock | `KillSwitchRunner::run` | monotonic local `Instant`; não persistido | `Instant::now()` / `start.elapsed()`; `helpers.rs:759,776` |
| Cache DEK | `DekCache` de `corelink-byok` | helper passa TTL 300 s | runner chama `evict_all_for_key` |

Índice: [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [FLOW-001](#flow-001) · [FLOW-002](#flow-002)

<a id="inv-001"></a>
### INV-001 — Status do provider fake segue comportamento programado

**Regra:** cada chamada adiciona o key ID; variantes fixas retornam status correspondente, script consome o primeiro e depois retorna `Ok`. **Imposição:** `BoundedKmsProvider::check_access`. **Violação/prova:** branch que não registra a chamada ou retorna status diverso; testes de transient/property são fonte candidata, NÃO EXECUTADA. **Estado:** SOURCE local apenas; não vale para adapters reais. Fonte: `src/helpers.rs:341–367`; API-002/006, REL-001.

[Índice de estado](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Setup constrói somente fixtures locais

**Regra:** `setup_byok_env` passa `300` à construção do cache, inicializa fake em `AlwaysOk` e sinks locais; trocar essa linha falsifica a descrição. **Imposição:** `setup_byok_env`; `CustomerAlertSink::new` usa config default/stub assert. **Violação/prova:** provider/store/sink real introduzido sem mudança de contrato. **Estado:** SOURCE; não comprova teto/configuração efetiva de outros owners. Fonte: `src/helpers.rs:470–480,703–730`; API-008/010, REL-001/002.

[Índice de estado](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Revoked/NotFound e transientes divergem no runner local

**Regra:** somente `Revoked|NotFound` seleciona eviction/degradação; `Throttled|ApiError` retornam `Ok(None)` sem audit/alert/status escrito neste arm. **Imposição:** match em `KillSwitchRunner::run`. **Violação/prova:** transiente chega ao braço revoke ou revoke omite passo; asserts em alvos existentes são NOT_EXECUTED. **Estado:** SOURCE_STATIC do runner, não regra de detector de produção. Fonte: `src/helpers.rs:763–859`; FLOW-001, REL-009/010.

[Índice de estado](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Falha posterior não desfaz efeitos locais anteriores

**Regra:** em revoke, SLA falha depois de eviction e `mark_degraded`, antes do append audit/alert; erro de alert ocorre após append audit. Em recovery, `restore_active` precede evento e alert; erro de alert pode deixar status active e audit gravado. **Imposição:** ordem e `?` em `KillSwitchRunner::run`. **Violação/prova:** retorno de erro com estado anterior limpo seria outra semântica. **Estado:** análise SOURCE; testes não foram executados. Fonte: `src/helpers.rs:764–846`; FLOW-001/002, REL-009–013.

[Índice de estado](#r05)

<a id="flow-001"></a>
[↩](#r01)
### FLOW-001 — Um ciclo local de revogação

1. `BoundedKmsProvider::check_access` registra chamada e sempre retorna `Ok(status)`; neste fake não propaga `BYOKError`.
2. `Revoked|NotFound` chama `evict_all_for_key`; falha propaga e rollback não é declarado.
3. `mark_degraded` vem depois; erro pode deixar cache já evicto.
4. Tempo é medido; `duration_ms > 30_000` retorna `SlaViolated` após passos 2–3.
5. Passando SLA, o evento é construído e `AuditSink::record` o apenda.
6. `alert` é chamada por último; erro retorna `RevokeError` com evento/status/cache ainda locais e sem compensação.

Fonte: `src/helpers.rs:758–814`; tempos em `now_ms`; fixtures garantem apenas map/vetor.

[Índice de estado](#r05)

<a id="flow-002"></a>
[↩](#r01)
### FLOW-002 — Recuperação local

1. `Ok` consulta `current_status`; erro retorna antes de mudança de recovery.
2. Só status `DegradedReadOnly` chama `restore_active`; falha propaga.
3. Status active é escrito antes da construção e append do evento restaurado.
4. Audit é apendado antes de `alert_recovery`.
5. Erro de alert retorna com status active e evento já registrados; não há rollback no runner.
6. Sem status degraded, retorna `Ok(None)` sem evento/alerta.

Fonte: `src/helpers.rs:816–849`; nenhuma operação durável ou execução foi observada.

[Índice de estado](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Configuração, targets e features

| Nome/fonte | Default declarado | Leitura | Target/condição | Efeito/limite |
|---|---|---|---|---|
| `PROPTEST_CASES` | parse válido ou `1_000` | corpo `prop_fail_closed` | target `prop_fail_closed` | quantidade fonte, não execução |
| Quatro `*_TEST_*` | sem default; presença testada | corpos `#[ignore]` | `live_provider_gated` | valor ligado a local `_`; não configura KMS real |
| Provider kind → region/FIPS label | AWS `us-east-1/Fips140_3_L1`; GCP `us-central1/Fips140_3_L1`; Azure `eastus/Fips140_3_L1`; Vault `us-east-1/Fips140_2_L1` | `setup_byok_env` por chamada | helper usado pelos alvos | rótulo local; não consulta região/atesta certificação |
| TTL fake | `DekCache::new(300)` | `setup_byok_env` | library usada pelos alvos | fixture local |
| Features/target cfg | sem `[features]` no manifesto | resolução Cargo não coletada | seleção resolvida UNKNOWN | não usar `--all-features` como prova |

| Alvo manifestado | Path | Importa library própria | Evidência declarada |
|---|---|---|---|
| `happy_revoke_flow` | `tests/happy_revoke_flow.rs` | `e2e_byok_revoke` | revoke e detector smoke |
| `recovery_flow` | `tests/recovery_flow.rs` | `e2e_byok_revoke` | restauração/flap |
| `adversarial_stampede` | `tests/adversarial_stampede.rs` | `e2e_byok_revoke` | 10k cache fake |
| `adversarial_race_condition` | `tests/adversarial_race_condition.rs` | `e2e_byok_revoke` | race de fake |
| `adversarial_transient_api_error` | `tests/adversarial_transient_api_error.rs` | `e2e_byok_revoke` | transientes scriptados |
| `prop_fail_closed` | `tests/prop_fail_closed.rs` | `e2e_byok_revoke` | propriedade, default 1k |
| `multi_provider_matrix` | `tests/multi_provider_matrix.rs` | `e2e_byok_revoke` | quatro kinds com fake |
| `live_provider_gated` | `tests/live_provider_gated.rs` | `e2e_byok_revoke` | 1 item normal; 4 corpos ignored fake |

Todos os oito alvos também importam `corelink_byok`; `corelink_ops` é usado pelo
library helper. Os quatro corpos `live_*` verificam presença de env, mas chamam
`setup_byok_env` e `AlwaysRevoked` no `BoundedKmsProvider`; valor de env é unused.
Sem `--ignored`, só o alvo não ignored é candidato à seleção normal. Nenhum alvo
foi executado aqui.

<a id="r07"></a>
## R07 — Falhas e observabilidade

| Erro/sinal | Origem limitada | Estado que pode permanecer | Diagnóstico |
|---|---|---|---|
| `BYOKError` | eviction de cache | o fake `check_access` sempre retorna `Ok(status)`; erro de eviction pode interromper antes do status/audit/alert | `KillSwitchRunner::run`; PROC-002 |
| `RevocationError` | trait store/alerter | store pode ter sido chamada após eviction; depende da implementação | PROC-002; corelink-byok/ops owner |
| `SlaViolated` | comparação estrita `> 30_000` | cache evicto + status degraded; sem audit/alert | `src/helpers.rs:776–782`; FLOW-001 |
| Alert error | `alert` / `alert_recovery` | status e audit local já gravados | `src/helpers.rs:794–812,836–845`; FLOW-001/002 |
| `InvariantViolated` | variante declarada | não há construção no runner observado | `src/helpers.rs:115–117` |

`AuditSink`/alert counters são observabilidade local em vetores. `hash_for_audit`
concatena prefixo `blake3:` aos primeiros até oito bytes; não é hash criptográfico.
O evento define `alerted_at_ms` antes de `alert`, portanto timestamp de campo não
é confirmação de entrega. Não há métrica ou observação de cliente neste package.

<a id="r08"></a>
## R08 — Verificação e evidências

| ID | Fonte na revisão | Método | Resultado e limite |
|---|---|---|---|
| API-001–015, INV-001–004, FLOW-001/002 | `src/lib.rs`, `src/helpers.rs`, SHA do frontmatter | leitura de source | SOURCE_STATIC; runtime UNKNOWN |
| alvo/import inventory | manifest e 8 `tests/*.rs` | busca estática | TEST_SOURCE; NOT_EXECUTED |
| REL-001–040 | manifest, Rust imports, workspace, workflows, lock/SBOM e prose census de B02–B06 | busca estática | declaração/match observado; resolução/runtime UNKNOWN |
| quatro documentos | paths exatos e checker externo | checks S-profile na autoria | estrutural apenas; sem cold approval |

**Axiomas epistemológicos (não são invariantes de implementação):** AX-01 manifest
prova declaração, não seleção; AX-02 assinatura/reexport prova contrato source-visible,
não chamada; AX-03 fake prova somente branches locais, não KMS/crypto/ops; AX-04 teste,
comentário, lock ou SBOM não prova execução; AX-05 OKF é rota, não política copiada.

**Unknowns:** grafo de callers fora do checkout; features/targets resolvidos e artefato
CI selecionado; transitivos ativados; provider real/credenciais/respostas; integração,
persistência/atomicidade, operação/resultado de testes; estado de produção; disponibilidade
e autoridade de owner. A descrição Cargo e comentários de “live” divergem do corpo do teste
que usa fake; esta referência registra a divergência, não a corrige em fontes fora do escopo.

[Impactos](BLAST_RADIUS.md#b01) · [Manutenção](MAINTENANCE.md#m01) · [Skill](../../../../.claude/skills/own-e2e-byok-revoke/SKILL.md#s01)
