# CO-COMMON — contrato compartilhado de execução e integração

**Versão:** 1.1-candidate. **Estado:** implementado/testado localmente onde indicado;
integração no repositório e cold review não realizadas.

[Identidades](#identidades) · [Registro](#registro) · [Relações](#relacoes) ·
[Procedimentos](#procedimentos) · [Integração](#integracao) · [Emissão](#emissao).

<a id="identidades"></a>
## Identidades e paths

`repository_id + manifest_path` identifica uma unidade; `package` é o nome Cargo,
não o basename da pasta. O slug vem EXCLUSIVAMENTE de `prepare_census.skill_slug`.
Frontmatter, pasta, índice, rascunho e registro usam esse mesmo resultado.
Underscores/maiúsculas são normalizados; nomes longos recebem sufixo determinístico.
Colisão aborta o censo antes de gerar ou publicar; nunca renomeia um package para
acomodar a skill. Uma eventual mudança de convenção exige revisão da política.

Uma renomeação de manifesto não dispara criação: registrar `former_manifests`,
conciliar os marcadores antigos e preservar a issue existente. A migração dos
registros e endpoints de relações é uma mudança coordenada, revisada, não inferida
por similaridade de nomes. Troca de organização mantém o repository_id; atualizar
URLs sem usar owner/name como nova identidade da unidade.

## Layout de integração

```text
docs/ownership/
  STANDARD.md, COMMON.md, VALIDATION-MATRIX.md
  templates/, schemas/, tools/, tests/
  records/<package>.json
  evidence/<package>/...
  crates/<package>/REFERENCE.md
  crates/<package>/BLAST_RADIUS.md
  crates/<package>/MAINTENANCE.md
  registry.json, index.md
.claude/skills/<skill_slug>/SKILL.md
```

Os quatro documentos são conteúdo de uso. O registro é evidência estruturada; não
armazena explicações omitidas do blast/manual. Histórico de revisões fica no Git.
Arquivos temporários e fixtures de teste não são registrados como reviews reais.

<a id="registro"></a>
## Um registro por package, com schema fechado

A forma executável está em `schemas/package-record.schema.json`; campos desconhecidos,
tipos errados e listas obrigatórias vazias são recusados. O modelo em
`templates/review-record.json` deliberadamente NÃO passa enquanto não for preenchido.

| Grupo | Conteúdo obrigatório |
|---|---|
| Identidade | Repository ID, manifesto, package, slug, baseline, caminho/ID do registro |
| Padrão | Versão, caminho e SHA-256 do contrato consumido |
| Fontes | Caminho, SHA-256 e papel; manifesto da crate obrigatório |
| Evidência | ID, caminho, SHA-256, classe de prova; nenhum comando é executado a partir dela |
| Descoberta | Regras de busca literal e fingerprint dos matches observados |
| Seleções | Package, target, features e evidência da seleção |
| Capacidade | Contagens de relações/módulos/procedimentos e evidência da medição |
| Revisão | Autor/revisor/sessões, atestado de contexto novo, data e evidência |
| Artefatos | Exatamente quatro tipos, paths/hashes finais, vereditos, checks e findings |
| Relações | Endpoints, contrato compartilhado, direções, ativação e efeito local |
| Procedimentos | Escopo de revisão e execução por PROC, separados |

`ownership_gate.py` exige correspondência entre documento e registro, checks mínimos
por artefato, evidências existentes e não alteradas e zero finding obrigatório aberto.
O CLI também verifica ancestralidade da baseline e conteúdo das fontes em Git.
Os testes unitários isolam essa camada de consistência com fixtures sintéticas;
não fingem executar o CLI contra o repositório real.

Seu resultado positivo é `EVIDENCE_CONSISTENT`, NÃO uma nova aprovação independente.
Strings de identidade, hashes e um arquivo dizendo PASS não demonstram que a pessoa
realmente executou uma cold review. A integração deve verificar a proveniência do
review no canal/sessão autorizado e a [matriz manual](VALIDATION-MATRIX.md).

## Anti-drift implementado e residual declarado

As regras usam glob de `fnmatch` sobre caminhos relativos e buscas LITERAIS. Enumeram
arquivos rastreados e novos não ignorados do checkout; capturam cada arquivo que
contém um termo, com termos encontrados e hash completo. Novo consumidor, remoção
ou alteração de match muda o fingerprint. Não usar regex/shell arbitrário vindo
no registro. Fontes não UTF-8 ou não legíveis bloqueiam, sem exclusão silenciosa.

Não incluir no escopo de busca os próprios documentos/registros/evidências: isso
criaria autorreferência. Usar nomes Cargo, aliases Rust, símbolos e identificadores
de recursos concretos. O revisor confere que os termos e diretórios cobrem o domínio.
Termos demasiado amplos ou estreitos são defeitos de preparação, não resolvidos por hash.

Uma busca literal não descobre todo vínculo dinâmico, código gerado ou contrato
externo. Mudanças no conjunto de buscas também mudam seu fingerprint. A completude
semântica e a adequação das buscas permanecem requisitos de cold review. Alteração
em arquivo não relacionado que não produz match não invalida os demais packages.

<a id="relacoes"></a>
## Relações: uma identidade, duas visões locais

A chave é estável e qualificada, por exemplo
`repo:1232040291:boundary:billing-aggregator-export-001`. Não reutilizar uma chave
aposentada para outro contrato. `REL-001` é apenas a âncora local no documento.

**Separar três direções:** dependência sempre consumidor→provedor; fluxo de dados
indica consumer-to-provider/provider-to-consumer/bidirectional/not-applicable;
impacto indica em que sentido a alteração/falha se propaga. Um reexport não prova
agendamento nem fluxo runtime. Uma relação de dados não vira dependência Cargo.

O registro carrega os dois endpoints, owner do contrato, ID/fingerprint de contrato,
superfície e condição de ativação. As duas visões devem concordar nesses campos.
`local_effect` e `local_anchor` diferem: explicam a consequência naquela crate e
apontam para sua ficha, sem copiar a política inteira. O hash de contrato deve
estar ancorado no conjunto de fontes; não é uma string decorativa.

A integração recusa dupla definição divergente ou falta de um dos lados quando
ambos os packages pertencem ao lote. Um peer ainda não documentado exige fronteira
explícita e responsabilidade no plano; não pode desaparecer nem ser declarado fresco.
O fechamento do lote reconcilia todos os peers internos. Arestas externas registram
limite, versão conhecida e coordenação; não são certificadas por uma lista Cargo.

<a id="procedimentos"></a>
## Aprovação do manual versus execução de um procedimento

Cada PROC registra `review_status`, `execution_status`, modo, ambiente, resultado,
evidências de revisão/execução, limitação e `required_for_acceptance`.

| Estado de execução | Significado | Regra de fechamento |
|---|---|---|
| EXECUTED_LOCAL | Passos executados no ambiente local declarado | Evidência EXECUTED_LOCAL e resultado PASS |
| REVIEWED_NOT_EXECUTED | Conteúdo conferido; passos não executados | Limite visível, sem reivindicar certificação operacional |
| BLOCKED_FOR_OPERATION | Precisa de ambiente/autorização indisponível | Limite visível e nenhuma operação implícita |

Os diagnósticos e validações locais aplicáveis necessários à aceitação usam
`required_for_acceptance=true`: não podem fechar sem execução. Para operações de
produção não executadas, o campo pode ser false APENAS com justificativa do revisor
sobre o escopo documental aprovado; isso não torna a operação certificada.
`AUTHORIZED_OPERATION` nunca recebe certificação produtiva de um teste local.

O conjunto de IDs de procedimentos no registro deve coincidir exatamente com o
manual; um PROC extra/omitido bloqueia. A aprovação do documento é limitada por
esses estados. Não esconder um procedimento crítico como opcional para passar.

<a id="integracao"></a>
## Responsabilidade e autoria paralela

**CO-COMMON:** a integração técnica desta frente, delegada ao assistente pelo
proprietário, responde pelo padrão, schemas, ferramentas, rollout e conciliação.
O revisor independente continua sendo outro ator/contexto, ainda a designar;
esta atribuição operacional não preenche CODEOWNERS nem inventa assignee GitHub.

Autores de crates alteram somente seus quatro artefatos e evidência/registro próprios.
Não editam à mão registry, índice nem o mesmo bloco do backlog em paralelo.
A integração agrega registros deterministicamente por package_uid, verifica
relações dos peers e regenera índices. O mesmo conjunto de contribuições, em
qualquer ordem, deve produzir bytes equivalentes e preservar ambos os reviews.

Registros individuais inválidos permanecem BLOCKED, e falhas cruzadas bloqueiam a
integração. Uma avaliação estrutural não ignora gates existentes de PR, cold review,
backlog ou a autorização de merge. Autoria é paralela; integração final é serial.

<a id="emissao"></a>
## Emissão serial e retomada

`render_issue.py` consome nome/targets reais do censo e seed específico. Não publica,
não adivinha basename e não produz READY. Limita cada issue a 240 linhas, 3.000
palavras e 24.000 bytes UTF-8; overflow é erro, sem truncamento de contexto.

`publication_gate.py` decide elegibilidade usando snapshot completo de issues
abertas/fechadas, marcador estável, antigos manifestos, contrato congelado imutável
e seis pré-condições com evidência: censo, contexto, capacidade, contrato comum,
deduplicação e backlog. O snapshot é obtido pela conexão GitHub autorizada; esse
auxiliar não acessa a rede nem certifica sozinho a completude da coleta.

Revisão v1.1 preserva `<!-- corelink-ownership:v1:manifest=... -->`: mudar a versão
do padrão não cria uma identidade nova. Match fechado exige decisão explícita,
não reabertura automática. Múltiplos matches bloqueiam.

Registrar intenção e hash do body antes da chamada. Timeout põe a linha em UNCERTAIN;
consultar novamente e conciliar antes de repetir. Uma ausência ainda ambígua não
libera retry cego. Confirmar número/URL/repository_id/marcador E body por leitura
posterior; somente então registrar CONFIRMED. Nenhum desses helpers cria issues.
