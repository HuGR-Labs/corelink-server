# CO-1 — Contrato de ownership por package Cargo

**Versão:** 1.1-candidate · **Estado:** proposta para revisão independente.
**Baseline estudada:** `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.
**Normatividade:** DEVE é requisito; NÃO DEVE é proibição; PODE é alternativa permitida.

## Índice

[1. Unidade e escopo](#co01) · [2. Quatro entregas](#co02) ·
[3. Formato e limites](#co03) · [4. Identidade e evidências](#co04) ·
[5. Skill](#co05) · [6. Referência](#co06) · [7. Blast radius](#co07) ·
[8. Manutenção](#co08) · [9. Cold review](#co09) ·
[10. Anti-drift](#co10) · [11. Issues](#co11) · [12. Gates de adoção](#co12).

<a id="co01"></a>
## 1. Unidade e escopo

**CO-01.1 — Uma issue por package Cargo de primeira parte.** A chave é
`repository_id + manifest_path`, com `package.name` resolvido por Cargo. Um package
com library, binaries, benches e testes recebe uma issue que cobre todos esses targets;
não uma issue por target. Nome de pasta não substitui nome de package.

**CO-01.2 — População principal.** Usar `workspace_members` de
`cargo metadata --locked --offline --no-deps --format-version=1`. O manifesto estudado
contém 95 membros declarados: 75 em `crates/` e 20 em `tools/`, `tests/` e `apps/`.
Essas contagens são baseline do estudo, não constantes a copiar nas skills.

**CO-01.3 — Exceções explícitas.** Conciliar todos os `Cargo.toml` rastreados em Git
com os membros de Cargo. Os nove caminhos `fuzz/` excluídos no manifesto entram como
candidatos a packages independentes; confirmar cada manifesto e então criar sua issue
própria. A issue da crate-mãe referencia o fuzzer, sem duplicar sua documentação.
Outros packages próprios independentes encontrados também entram. Vendors/dependências
terceiras não recebem ownership de primeira parte; cada exclusão tem motivo registrado.
Um manifesto histórico rastreado fora do workspace pode receber a classificação
`archive` somente com caminho, razão e evidência explícitos; `archive` é uma
exclusão de elegibilidade, não uma forma de esconder um package first-party ativo.

**CO-01.4 — Absorções.** Um antigo package que virou módulo não recebe uma issue
fantasma: a crate hospedeira cobre sua implementação; aliases históricos ajudam a busca.
Uma façade que ainda reexporta outra crate não absorve o ownership da implementação.

**CO-01.5 — Relações fora de Rust.** Worker TypeScript, SQL, schemas, workflows,
clientes e outros repositórios podem estar no blast radius. Isso não autoriza modificar
esses sistemas nem confundir sua superfície com uma dependência Cargo.

<a id="co02"></a>
## 2. Quatro entregas e uma fonte por assunto

| Artefato canônico | Pergunta respondida | Não é |
|---|---|---|
| `.claude/skills/<skill_slug>/SKILL.md` | Como assumir ownership e decidir o próximo passo? | Enciclopédia ou concessão de permissões |
| `docs/ownership/crates/<package>/REFERENCE.md` | O que a crate faz e como funciona? | Cópia integral de rustdoc ou histórico de sessões |
| `docs/ownership/crates/<package>/BLAST_RADIUS.md` | O que se relaciona com ela e como uma mudança se propaga? | Apenas `cargo tree` ou uma lista de nomes |
| `docs/ownership/crates/<package>/MAINTENANCE.md` | Como diagnosticar, alterar, validar e recuperar com segurança? | Checklist genérico “rode testes e faça deploy” |

Os três documentos de package são únicos por assunto e package; a skill é a quarta
entrega, roteadora e separada. Não criar `part-01`, apêndices
narrativos paralelos, páginas por dependência ou cópias em vários harnesses para
contornar o limite. Registros atômicos e tabelas permanecem no documento correspondente.

`docs/ownership/index.md` e `registry.json` são índices gerados. Registros de review,
fingerprints e logs são evidência, não um quinto manual nem um lugar para esconder
relações que deveriam estar no blast radius.

A wiki OKF conserva os conceitos transversais e decisões canônicas. Os quatro artefatos
aplicam esses conceitos ao package e apontam para eles. Não redefinem silenciosamente
uma política de domínio. Contradição entre código, wiki e contrato deve ser explicitada
com decisão/issue; não escolher a versão conveniente e declarar consistência.

<a id="co03"></a>
## 3. Formato, navegação e limites rígidos

### 3.1 Perfis e tetos

Cada arquivo deve respeitar **simultaneamente** linhas, palavras e bytes UTF-8.

| Arquivo | Perfil S: linhas / palavras / KiB | Perfil H: linhas / palavras / KiB |
|---|---|---|
| SKILL.md | 180 / 1.200 / 12 | 180 / 1.200 / 12 |
| REFERENCE.md | 400 / 3.000 / 36 | 700 / 5.500 / 64 |
| BLAST_RADIUS.md | 900 / 7.000 / 80 | 1.800 / 14.000 / 160 |
| MAINTENANCE.md | 400 / 3.000 / 36 | 700 / 5.500 / 64 |

S é o padrão. H exige inventário que mostre pelo menos uma destas condições:
mais de 40 relações atômicas de fronteira; mais de 20 módulos semânticos de implementação;
mais de oito procedimentos de manutenção distintos. O revisor confirma a contagem e
recusa fragmentação artificial para obter um teto maior. Nome, importância ou LOC não
bastam para escolher H. Nenhum tamanho mínimo: uma crate pequena deve produzir pouco texto.

**CO-03.1 — Contagem:** palavras = sequências não vazias separadas por whitespace;
linhas = linhas físicas; KiB = 1.024 bytes. Contam frontmatter, links, tabelas, exemplos,
comentários e blocos de código. Não minificar, esconder em HTML ou colar linhas gigantes.
Não usar contagem de tokens dependente de tokenizer como gate canônico.

**CO-03.2 — Overflow:** se a cobertura real não cabe no perfil, a entrega fica
`BLOCKED_CAPACITY`. Não omitir relações, substituir detalhes por um diagrama ou
abrir documentos extras. Medir a população, remover redundância e submeter uma
revisão numérica do contrato compartilhado antes de aprovar esse package.
O mesmo bloqueio vale se uma única ficha não couber: registrar ID, texto completo,
medidas e população de call sites; não fragmentar uma relação só para passar.
Os tetos desta versão ainda precisam do piloto; não se afirma que já cabem em todas as crates.

### 3.2 Regras editoriais testáveis

- Título único, frontmatter, índice clicável e seções fixas dos templates.
- Até três níveis de heading. IDs estáveis explícitos (`r01`, `rel-001`, `inv-001`, `proc-001`).
- Cada seção de nível 2 e cada registro longo tem entrada em seu índice correspondente.
  Da skill até qualquer registro: no máximo três cliques; cada registro retorna ao índice.
- Resumo de entrada: no máximo 120 palavras. Parágrafo explicativo: no máximo 80 palavras.
- Tabelas: até seis colunas; células não escondem ensaios. Relações complexas viram fichas,
  não uma tabela ilegível com 15 colunas.
- Ficha de relação: até 120 palavras e 14 linhas físicas; uma relação por ficha.
- Ficha de contrato público: até 100 palavras; fluxo: até oito passos curtos.
- Procedimento: até 220 palavras, oito passos e 50 linhas. Código: até 25 linhas por bloco.
- Diagramas são opcionais e auxiliares. Nunca substituem a enumeração verificável das relações.
- Exemplos são mínimos, sintéticos e sem credenciais, dados de cliente ou operações reais.

**CO-03.3 — Proibições:** filler, histórico de chat, slogans, “etc.” para fechar uma
população, `TODO` pendente em entrega aprovada, “N/A” sem justificativa, nomes inventados
de responsáveis, comandos ilustrativos apresentados como executados e números copiados
que deveriam vir de gerador. Um `UNKNOWN` explícito não conta como ausência comprovada.

<a id="co04"></a>
## 4. Identidade, autoridade e evidência

### 4.1 Cabeçalho dos documentos

O schema desta revisão é `corelink-ownership/1.1`. Os três documentos usam: `schema`, `document`, `package`, `manifest`, `source_commit`,
`profile`, `state`, `evidence_set`. O `evidence_set` identifica o registro de fontes
inspecionadas. Não embutir o próprio hash ou o futuro commit do documento no seu conteúdo:
isso cria uma referência circular. O review externo guarda seus hashes reais.

`corelink-ownership/1` permanece aceito somente quando o checker é chamado com
`--historical-fixture` para fixtures/evidências históricas das revisões
anteriores. Artefatos atuais, records e issues desta revisão DEVEM usar
`corelink-ownership/1.1`; o checker rejeita schema legado por padrão.

`registry.json` e `index.md` são saídas geradas da população observada, não um
quinto manual: cada linha deve distinguir presença estrutural, revisão fria e
publicação. `STRUCTURAL PASS` nunca é promovido automaticamente a aprovação.
O gerador também reconcilia `package`, `manifest`, `source_commit`, `profile` e
`evidence_set` entre os quatro artefatos; ausência ou divergência é bloqueio
explícito, não uma inferência de identidade. A skill mantém `package`,
`manifest`, `source-commit` e `evidence-set` dentro de `metadata`, mas não
declara `profile`: o perfil S/H é obrigatório nos três documentos e nos seus
registros de revisão, não no frontmatter da skill.

A skill usa o frontmatter Agent Skills (`name`, `description`) e os campos de projeto
como strings em `metadata`. `name` usa o `SKILL_SLUG` de `prepare_census.skill_slug`, coincide com o diretório e respeita 64 caracteres;
`description` fica abaixo de 600 caracteres neste projeto, descrevendo gatilhos específicos.
Não declarar `allowed-tools` amplo nem autorização para produção.

### 4.2 Registro de fonte

Uma afirmação estrutural/operacional tem: ID, caminho, símbolo ou faixa de linhas,
commit/blob de origem, classe de evidência e resultado observado. Fontes transversais
incluem manifests, lockfile, entrypoints, schemas, migrations e configuração de build.
Links navegacionais preferem caminhos relativos; permalinks imutáveis sustentam a evidência.

Classes independentes, não uma escala que autoriza inferências:

| Classe | O que prova | O que não prova |
|---|---|---|
| `SOURCE` | Código/manifesto foi inspecionado nessa revisão | Compilação, execução ou implantação |
| `RESOLVED` | Dependência resolvida para a seleção registrada | Chamada alcançável no runtime |
| `EXECUTED_LOCAL` | Comando executado no ambiente registrado | Equivalência com produção |
| `DEPLOYMENT` | Artefato/configuração implantada identificados | Comportamento de todas as rotas |
| `OBSERVED_RUNTIME` | Comportamento observado com tempo e escopo | Comportamento universal/futuro |

Registrar separadamente `implemented`, `wired` e `runtime_verified`, cada um com
`yes/no/unknown/not_applicable`, evidência e escopo. Stub nativo não prova adapter wasm;
cron verde não prova processamento de dados; reexport não prova chamada nem agendamento.

### 4.3 Ownership real

Distinguir dono da implementação, dono do contrato público, composition root,
operador e aprovador. O dono do contrato pode ser endpoint ou terceiro verificável
(`external:...`, `system:...`, `repo:...`) quando `external_boundary` identifica a
superfície que sustenta o contrato. Registrar pessoa/equipe/rota de escalonamento
apenas se verificada.
Antes de aprovar ownership, uma rota real de escalonamento precisa existir. Uma skill não
substitui CODEOWNERS, autorização de acesso ou revisão humana quando exigida.

<a id="co05"></a>
## 5. Contrato da skill

Seções obrigatórias: S01 Acionamento; S02 Território e autoridade; S03 Roteamento;
S04 Decisões e invariantes; S05 Fluxo; S06 Paradas; S07 Evidência e saída.

A skill DEVE conter gatilhos positivos e negativos. Não ativar toda a frota de ownership
porque a tarefa menciona “CoreLink”. Ao tocar um contrato compartilhado, carregar o owner
primário e os consumidores materialmente afetados, não todas as dependências transitivas.

O roteamento liga diretamente às âncoras dos três documentos e às skills transversais
pertinentes (`okf-context`, `built-not-wired`, revisão). Não carregar todos os manuais
integralmente como passo inicial. Reutilizar guardrails existentes em vez de copiá-los.

Decisões usam a forma `condição → ação → evidência exigida → parar quando`.
A saída é um pacote curto de mudança: objetivo, contratos/RELs afetados, gates selecionados,
resultado real, risco residual e próximo responsável. “Pronto” não pode significar
somente arquivo escrito ou comando retornando zero.

<a id="co06"></a>
## 6. Contrato da referência

Seções obrigatórias: R01 Identidade; R02 Fronteiras; R03 Implementação; R04 Contratos;
R05 Estado/fluxos/invariantes; R06 Configuração/targets; R07 Falhas/observabilidade;
R08 Verificação/evidência.

**Mapa da implementação:** distinguir módulo próprio, módulo absorvido, façade,
reexport, adapter real, fake, geração de código e target auxiliar. Enumerar módulos
semânticos e suas entradas reais; helpers privados só precisam de detalhe quando
sustentam contrato, estado ou risco. Não prometer uma paráfrase de cada linha de Rust.

**Contrato público atômico:** `API-ID; símbolos exatos; entrada/unidades; pré-condição;
saída/pós-condição; erros/efeitos; compatibilidade; INV/REL; evidência`. Uma família
só pode agrupar símbolos com o mesmo contrato: listar seus membros, sem esconder diferenças.
Reexports têm proprietário original e alias público explícitos.

**Invariante:** `INV-ID; predicado falsificável; ponto que o impõe; falha que o viola;
teste ou verificação; estado atual`. “Seguro”, “correto” e “robusto” não são predicados.
Invariantes arquiteturais herdadas ligam à definição canônica no OKF/ADR.

**Estado e fluxo:** proprietário de cada dado, chave de partição, unidade, persistência,
vida útil, fonte de tempo, transição, ponto de durabilidade, efeitos, concorrência e
compensação. Registrar apenas dimensões aplicáveis, justificando ausência quando relevante.

**Configuração:** nome (não valor secreto), origem, default real, target/feature, leitura
em build/boot/request, efeito, validação e modo de falha. Declarar combinações incompatíveis;
não prescrever `--all-features` para um conjunto mutuamente exclusivo.

<a id="co07"></a>
## 7. Contrato de blast radius

Seções obrigatórias: B01 Escopo; B02 Método/população; B03 Relações diretas;
B04 Propagação transitiva; B05 Mudança/impacto/validação; B06 Cobertura/desconhecidos.

### 7.1 O que é uma relação atômica

É uma fronteira com **produtor, consumidor, superfície/contrato, condição de ativação
e mecanismo de falha próprios**. Dois call sites com contrato, condições e falhas
idênticos podem compartilhar ficha, mas todos os call sites permanecem enumerados.
Mudou qualquer dessas propriedades: outra ficha. A unidade não é “toda a relação com Stripe”
e tampouco cada chamada privada interna sem efeito de fronteira.

A ficha possui os campos: `ID/tipo; origem→destino; superfície; ativação; contrato;
estado/efeitos; falha/propagação; limite de propagação; validação; coordenação/evidência`.
A chave compartilhada, os peers e as três direções independentes seguem
[COMMON.md](COMMON.md#relacoes); `REL-001` é somente uma âncora local.
Tipos incluem `dependency`, `reexport`, `runtime-call`, `data`, `event`, `config`,
`build-deploy`, `telemetry`, `test`, `ffi` e `external-contract`.

**Exemplo de insuficiência:** “billing depende de aggregator”.
**Forma exigida:** identificar o `pub use`, seu alvo, nomes expostos, o que a alteração
propaga aos dois caminhos de importação, a condição de compilação, limites e a validação.

### 7.2 Populações que precisam ser conciliadas

| População | Levantamento mínimo |
|---|---|
| Dependências diretas Cargo | Normais, build, dev; path/registry/git; aliases, optional, cfg e features |
| Consumidores Cargo | Relações inversas em todo o workspace, sem assumir um único binário |
| Reexports e interfaces | Traits, tipos, macros, exports FFI e pontos de construção/injeção |
| Runtime e dados | HTTP/gRPC/filas/cron, SQL, tabelas/colunas relevantes, buckets/prefixos, locks, caches |
| Build/operação | Docker, Wrangler, flags, env vars, scripts, migrations, CI e artefatos |
| Consumidores externos | Workers TS, SDKs, CLIs, outros repos e contratos públicos; revisão/versão conhecida |

Cada elemento descoberto deve mapear a um REL ou a uma exclusão justificada.
Dev-dependency não é uma aresta de produção. Contrato compartilhado não é automaticamente
uma chamada. Biblioteca externa é documentada na fronteira utilizada, não recopiada por dentro.
Arestas transitivas de fornecedores terminam na fronteira de responsabilidade documentada.

### 7.3 Três levantamentos distintos

1. **Inventário:** `cargo metadata --no-deps` identifica packages e declarações; não possui
   grafo `resolve` completo. Seu resultado não certifica alcance transitivo.
2. **Resolução:** por build/target/features efetivamente suportados, guardar o comando
   completo e o grafo resolvido. Usar também consultas inversas com escopo workspace.
   `--target all` é uma união de plataformas, não um artefato implantado.
3. **Semântica:** inspecionar fonte, entrypoints, bindings e dados; rastrear o caminho
   causal até o efeito. Cargo não encontra sozinho relações por SQL, configuração ou HTTP.

**Consulta orientadora, não resultado já executado:**

```sh
cargo metadata --locked --offline --no-deps --format-version=1
cargo tree --locked --offline --workspace --invert "$PACKAGE" \
  --target "$TARGET" --edges normal,build
```

`PACKAGE`, `TARGET` e features são resolvidos no pacote de execução. Para o artefato
específico, selecionar também seu package/manifesto e features reais. Separar consulta
workspace de consulta do shipped artifact. Falha de cache offline é `BLOCKED_ENVIRONMENT`,
não permissão para alterar Cargo.lock, baixar dependências ou executar deploy.

### 7.4 Propagação transitiva sem explosão combinatória

Enumerar todos os nós internos alcançados na seleção e suas arestas diretas relevantes;
um caminho testemunha por nó permite navegação. Acrescentar cada caminho causal
materialmente distinto (dados, falha, ativação) que o caminho testemunha não explica.
Não é necessário listar todas as permutações de caminhos equivalentes de um grafo.

Cada linha registra `destino; RELs do caminho; condição; efeito; contenção; validação`.
Diferenciar `potencial por resolução`, `chamada traçada` e `observado`. Uma fronteira
com owner externo tem contrato/versionamento e ponto de parada, não “sem impacto” presumido.

### 7.5 Cobertura honesta

Registrar contagens `descobertos / documentados / excluídos com motivo / desconhecidos`
por população. Exigir `descobertos = documentados + excluídos` e zero desconhecidos
estruturais/materialmente relevantes para aprovação. A igualdade não prova que a
busca descobriu tudo: a cold review refaz o levantamento a partir de fontes independentes.

Uma propriedade de produção não medida pode permanecer desconhecida **se** o documento
não depender dela nem afirmar ativação/completude nessa superfície e houver limite claro.
Isso não permite deixar dependências, contratos ou riscos sem estudar e chamá-los completos.

<a id="co08"></a>
## 8. Contrato do manual de manutenção

Seções obrigatórias: M01 Preparação; M02 Seleção; M03 Procedimentos; M04 Testes;
M05 Recuperação/compatibilidade; M06 Escalonamento/registro.

Cada procedimento possui `PROC-ID; objetivo/gatilho; pré-condições; ambiente/permissões;
entradas; passos/comandos; saída esperada/predicado; falhas/parada; recuperação;
evidência de execução`. Um comando com variável explica como obter/validar seu valor.
Copiar e colar só depois de verificar o ambiente indicado.

Cobrir, quando aplicável: reproduzir falha; alteração interna; alteração de API/trait;
atualização de dependência; mudança de feature; schema/migration; adapter externo;
regeneração; diagnóstico; desativação/roll-forward/rollback. Ausência justificada,
não criação de um runbook de deploy para cada library.

O estado de revisão e o estado de execução são registrados por PROC conforme
[COMMON.md](COMMON.md#procedimentos); diagnóstico local não certifica outra operação.
Cada procedimento tem um modo: `READ_ONLY`, `LOCAL_ISOLATED` ou `AUTHORIZED_OPERATION`.
Nenhum deles concede autorização implícita para produção, cobrança, exclusão, rotação,
ativação de billing ou recursos pagos. Valores secretos nunca entram na documentação.

**Recuperação não é só git revert.** Distinguir código, dados, contratos de wire e
artefatos. Explicar reversibilidade, dados já gravados, compatibilidade N/N-1,
compensação e roll-forward. Para uma library sem estado próprio, indicar a recuperação
no composition root real e a condição em que ela é necessária.

**Testes:** por tipo de mudança, listar package/target/features, comando exato, suite,
predicado e ambiente. Checar contratos por comportamento/estado final, não só status200.
Usar gates já existentes, sem 95 repetições de suites globais. Builds pesados respeitam
a capacidade documentada do host; este pacote não dispara CI nem cria agendamentos.

<a id="co09"></a>
## 9. Cold review e aprovação por artefato

### 9.1 Independência e entrada congelada

O revisor não é o autor do artefato. Para revisão por agente, usar sessão/contexto
novo, com acesso ao checkout e às ferramentas; não reaproveitar a conversa de elaboração.
Diversidade de modelo é opcional; não substitui independência de contexto.

O pacote do revisor contém apenas: issue, versão imutável deste contrato, quatro arquivos,
baseline de código, configuração de build avaliada, fontes e comandos reproduzíveis.
Não enviar conversa do autor, justificativa persuasiva de que “está pronto” ou respostas
esperadas aos desafios de navegação. O autor responde a achados depois do primeiro veredito.

Antes da revisão, congelar hashes dos quatro arquivos, manifesto/lockfile/fontes relevantes
e política de targets/features. O registro JSON de review segue `schemas/package-record.schema.json`, explicado em
[COMMON.md](COMMON.md#registro), fica fora dos artefatos e
referencia esse conjunto. Capturar identidade do revisor, sessão, instante e evidências.

### 9.2 Quatro vereditos, não um carimbo geral

A mesma sessão independente pode revisar os quatro, mas DEVE produzir um veredito
individual para `skill`, `reference`, `blast_radius` e `maintenance`.
Vereditos: `APPROVE`, `FIX_FIRST`, `REJECT`, `BLOCKED`.
`APPROVE` exige zero achados obrigatórios abertos para aquele artefato.

| Artefato | Desafio independente mínimo |
|---|---|
| Skill | 3 tarefas que devem ativar, 2 que não devem; selecionar leitura/gates e recusar 2 ações fora da autoridade |
| Referência | Localizar entrada, contrato, estado e falha no código; verificar cada invariante crítico e suas fontes |
| Blast radius | Refazer censo direto/inverso; buscar dependência por dados/config fora de Cargo; rastrear os caminhos críticos e conferir exclusões |
| Manual | Executar diagnóstico e validação local aplicáveis; exercitar uma falha/parada e recuperação em ambiente isolado |

Em navegação, o revisor parte somente da skill/índices; chega ao registro correto em
até três cliques sem orientação do autor. Ele registra caminho seguido e resposta.
Para alegação operacional que não pode testar no ambiente autorizado, usar `BLOCKED`
ou limitar a alegação a um procedimento não certificado; não converter “não testado” em PASS.

### 9.3 Correção e re-review

Mudança semântica num artefato invalida sua aprovação. Mudança de contrato compartilhado,
fonte, feature/target ou evidência invalida as aprovações que dependem dela. Uma
normalização **metadata-only** de frontmatter — limitada a identidade de package,
manifesto, source pin ou evidence-set, sem alteração no corpo, headings, contratos,
invariantes, relações, procedimentos ou comandos — exige readback desses campos, mas
não descarta a revisão substantiva do corpo. O registro deve separar `semantic_review`
de `metadata_readback` e conservar os hashes de ambos. Para essa exceção,
`semantic_review` aponta para uma cópia imutável dos bytes revisados em evidência
`REVIEW`; `metadata_readback` identifica os bytes atuais, campos alterados, leitor,
sessão, instante e evidência da conferência. O gate compara as duas cópias byte a
byte após remover somente as quatro linhas de frontmatter permitidas. Sem cópia,
readback ou igualdade do restante, a aprovação anterior não vale. Qualquer mudança fora desse
escopo volta a exigir o conjunto exato e nova cold review; não reutilizar um “LGTM” de
SHA anterior para conteúdo semântico alterado.

Não autoaprovar findings, trocar o revisor por um linter nem chamar releitura do autor
de cold review. O hash correto prova identidade de bytes, não independência ou veracidade.
Aprovação de docs também não prova que a funcionalidade de produção está correta.

<a id="co10"></a>
## 10. Anti-drift e atualização sem burocracia inútil

Um registro de evidência por package guarda caminhos/blobs de código, manifests,
lockfile, contratos externos, configuração e os quatro hashes documentais.
Manter também as buscas de descoberta: paths, símbolos/recursos e fingerprint do
inventário de consumidores. **Hash de arquivos já conhecidos não detecta consumidor novo.**

Em uma mudança, os gates verificam: package adicionado/removido/renomeado; diff de
manifests; novos consumidores; source_files OKF; configuração/schema; links/âncoras;
limites e quatro vereditos. Alteração relevante coloca o registro em `STALE` até
reconciliação. Mudança semântica exige review; reemissão de índice idêntico não exige
reler 95 manuais. Uma alteração não relacionada de main não invalida tudo automaticamente.

Estado: `DRAFT → AUTHOR_VALIDATED → COLD_REVIEW → APPROVED`; ramos
`FIX_FIRST/BLOCKED/REJECTED`; após drift, `STALE`. Estado publicado vem do registro de
review e dos checks, não de `state: approved` escrito pelo autor dentro do documento.
O índice populacional lê `records/<package>.json` pelo gate de consistência e
mostra `REVIEW_EVIDENCE_CONSISTENT` somente quando o registro válido existe;
ausência permanece `UNVERIFIED`. Para emissão, lê o ledger e uma cópia do issue
relido com hash explícito, corpo/marcador e identidade conferidos; ausência de
readback não vira `PUBLISHED`. Esses estados registram evidência, sem substituir
o veredito frio nem a conferência humana da independência.

O validador OKF atual não protege automaticamente `docs/ownership`. A integração
explícita com ele é responsabilidade de CO-COMMON. Reutilizar o mecanismo
OKF/source-blobs onde houver compatibilidade comprovada, não presumida. Acrescentar um gate pequeno e
path-scoped onde faltar cobertura; não criar uma plataforma de governança ou crons
novos para conteúdo que só muda com commit. Revisão de runtime tem validade própria,
registrada quando a afirmação depende de estado vivo.

<a id="co11"></a>
## 11. Contrato das issues

Título: `[ownership] <package>: skill, referência, blast radius e manutenção`.
A issue DEVE fixar manifesto, identidade de package, baseline e versão do contrato;
listar os quatro destinos; trazer fatos locais já verificados e incertezas explícitas;
ter os cinco axiomas abaixo e os quatro checkboxes de revisão separados.

- **Success criteria:** outra pessoa/agente consegue assumir ownership, localizar
  o contrato afetado, avaliar impacto e executar o procedimento pertinente sem relato do autor.
- **Completeness criteria:** população de targets/módulos/contratos/relações/procedimentos
  conciliada; skill mais três documentos únicos; todas as categorias examinadas; ausência
  justificada.
- **Quality standards:** tetos, navegação, evidências, exemplos seguros, não duplicação OKF
  e protocolo de cold review cumpridos.
- **Definition of done:** quatro entregas revisadas nos hashes finais, gates pertinentes
  verdes, PR integrado pelas regras existentes e índice/backlog reconciliados.
- **Invariants:** sem mudanças funcionais disfarçadas; sem autoconcessão de autoridade;
  sem falsa evidência de produção; sem omissão para caber; sem autoaprovação.

Não usar a mesma descrição genérica com só o nome trocado como pacote de execução.
Incluir o levantamento específico: aliases, targets, deps diretas/inversas, reexports,
fontes iniciais OKF, pontos de risco e comandos já identificados. A issue pode atribuir
a descoberta semântica ao autor, mas seu estado de preparação deve dizer o que falta.

A preparação comum, a responsabilidade de integração e a sequência finita estão em
[plans/PREPARATION.md](plans/PREPARATION.md). Os autores não editam índices globais
concorrentemente. O registro por package alimenta a agregação determinística.

Antes de emitir: pesquisar issues abertas E fechadas por chave/nome/aliases e ler o
backlog canônico completo. Um match relevante precisa de decisão: reaproveitar, ampliar
ou registrar diferença. Busca vazia isolada não prova unicidade. A emissão é serial,
registra IDs retornados, relê o resultado e evita duplicar depois de timeout.

Usar marcador estável: `<!-- corelink-ownership:v1:manifest=<path> -->`.
Renomear package/manifesto exige conciliar a chave antiga, não abrir outra issue cegamente.
Labels/assignees/milestones só são usados se existem e foram definidos, não inventados.

<a id="co12"></a>
## 12. Gates de adoção e sequência de trabalho

1. **G0 — Contrato candidato:** revisar os templates contra uma leaf (`corelink-hash`),
   um híbrido (`corelink-billing`), o composition root (`corelink-server`), um adapter
   wasm (`corelink-cf-bindings`) e um harness de teste. Os quatro documentos existem
   para os cinco pilotos e passaram o checker estrutural; os vereditos frios ainda não
   são uniformes e o piloto não é aprovação automática do contrato.
   A calibração de limites só é demonstrada quando cada piloto registra perfil,
   linhas/palavras/bytes medidos, população material (relações/módulos/procedimentos),
   overflow observado e decisão de capacidade; um checker verde isolado não basta.
   A última medição dos bytes atuais está em
   [PILOT-CALIBRATION-READBACK-20260923-CURRENT.md](evidence/revision-1.4/PILOT-CALIBRATION-READBACK-20260923-CURRENT.md);
   leituras anteriores permanecem históricas.
2. **G1 — Cold review do padrão:** revisor independente avalia completude, navegação,
   overflow, fonte única, negação de autoridade e aprovação vinculada a bytes.
   Corrigir e versionar o contrato antes de tratá-lo como aprovado.
3. **G2 — População e emissão:** obter censo Cargo, conciliar fuzz/vendors/independentes,
   hidratar pacote específico de cada issue, conferir duplicatas/backlog, fixar link
   para o contrato publicado e só então emitir uma issue por package elegível.
4. **G3 — Execução por package:** estudar → produzir os quatro artefatos → validar autoria
   → revisão fria por artefato → corrigir → re-review → integração com gates existentes.

O preflight de emissão verifica que o link congelado aponta para
`docs/ownership/STANDARD.md` em um commit existente e que os bytes desse commit
são os bytes do padrão candidato. O gerador do registry confere o pin observado
contra `origin/main` buscado localmente e a referência remota atual antes de
gravar as saídas; um pin ou fetch obsoleto bloqueia a geração.

Pins de fonte anteriores ao `main` atual permanecem evidência histórica até cada
superfície afetada receber novo SOURCE readback. O freeze não promove artefato
ancorado em SHA antigo quando há drift material sem reconciliação explícita.
O último readback remoto desta campanha está em
[RELEASE-STANDARD-REPAIR-20260923.md](evidence/revision-1.4/RELEASE-STANDARD-REPAIR-20260923.md),
com `origin/main` observado em `7f966dda8234f54b58766f33e5fca2ad02cc898b`.
Desde o readback `a18d1146`, main mudou workflows, documentos de campanha,
scripts e testes; o diff não contém Cargo manifests nem fonte Rust. As reconciliações de SOURCE, manifest,
workspace e lock já pendentes seguem bloqueando aprovação current-main dos
packages afetados; o novo pin do índice não as resolve.

**Estado histórico da revisão 1.1 (preservado para proveniência):** correções locais,
137 testes documentais, probe adversarial 13/13 e 420/420 checks estruturais foram
registrados naquele ponto. A matriz [VALIDATION-MATRIX.md](VALIDATION-MATRIX.md)
separa checks automáticos e requisitos manuais. O status corrente deve ser lido nos
registros de evidência versionados: o registry atual reconcilia 105 identidades e o
censo de deduplicação registra 105 decisões, mas cold review do padrão, congelamento
do contrato, reconciliação dos gates por package, integração em `main` e publicação
continuam separados e não são promovidos por sucesso em fixtures. Nenhum gate de
adoção é promovido automaticamente.
