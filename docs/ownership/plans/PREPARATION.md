# Preparação finita antes da emissão do lote

**Responsável pela coordenação:** integração técnica CO-COMMON desta frente.
**Cold reviewer:** independente, ainda não designado. **Estado:** preparação;
nenhum item abaixo é uma issue já publicada.

O baseline do estudo continua `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`.
O checkout localizado no Mac está em `codex/b232-restack-20260908`, e não foi
resetado nem alterado. Seu ref `origin/main` coincide com o baseline, mas isso
NÃO prova que o conteúdo de trabalho coincide. Usar checkout isolado e verificado.

| WP | Success criteria | Completeness criteria | Quality standards | Definition of done | Invariants |
|---|---|---|---|---|---|
| CO-COMMON | Um único contrato e oráculo compartilhado | Schemas, parsers, relações, procedimentos, publicação, integração | Regressões negativas + controles; matriz manual explícita | Código/local docs revisados; integração no repo e cold review aprovadas | Sem autoaprovação, crons novos ou mudança do OKF por implicação |
| CO-CENSUS | Toda unidade tem identidade e seed inequívocos | Workspace + independentes/fuzz; vendors/fixtures classificados; targets/aliases/deps/consumers; seeds locais | Cargo locked/offline; baseline limpa; zero nome inferido de pasta | Censo real + preparação sem placeholders e fonte por seed | Não confundir declarações com runtime; nenhum reset de trabalho alheio |
| CO-PILOT | Limites utilizáveis com cobertura integral | Leaf hash, híbrido billing, root server, adapter CF e harness e2e-billing-flow; quatro arquivos por piloto | Medir bytes/linhas/palavras/fichas, população e navegação | Cinco conjuntos completos e relatório de capacidade; ajuste numérico justificado | Sem omissão, documentos extras, teto ampliado sem dados ou piloto sintético tratado como real |
| CO-FREEZE | Critérios estáveis antes de 95 execuções | Padrão, schemas, templates, matriz e piloto revisados em conjunto | Revisor novo/contexto novo; findings fechados nos hashes finais | Versão imutável publicada com proveniência da revisão | Autorrevisão não congela contrato |
| CO-EMIT | Uma issue certa por unidade elegível | Issues abertas/fechadas, aliases, manifestos anteriores, backlog completo; ledger/readback | Serial; retomada sem duplicação; body limitado; nenhuma label/assignee inventada | IDs/URLs conferidos por leitura; cada issue vinculada a backlog e contrato | DRAFT/BLOCKED não são READY; timeout não autoriza retry cego |
| CO-INTEGRATE | Entregas paralelas preservadas | Registros por crate; peers consistentes; índice/backlog e gates existentes | Ordem de integração independente; fontes/reviews fresh | Quatro artefatos revisados e merged por crate; lote conciliado | Nenhuma edição concorrente de índices globais nem merge com gate pendente |

## Dependências e paralelização

CO-COMMON + CO-CENSUS alimentam CO-PILOT; CO-FREEZE exige o piloto e a revisão comum;
CO-EMIT exige freeze, censo/seed e dedup/backlog. O estudo de crates é paralelizável.
A autoria das entregas por crate NÃO exige aguardar autoria de todas as outras;
se um peer não estiver pronto, registrar a dependência com condição de fechamento.

CO-INTEGRATE é um ponto de integração, não uma ordem para serializar 95 autores.
Nenhuma issue de crate deve criar uma variante própria do schema ou gerador.

## Execução já obtida nesta revisão

Correções locais de parser, slug, schemas/gates, consistência de relações, estados
por procedimento, render/preflight e regressões estão no pacote. Seus testes estão
em `evidence/revision-1.1/`. Não há execução Cargo real, cinco pilotos completos,
coleta integral do backlog, cold review independente ou publicação certificada.
Essas dependências permanecem abertas; nenhuma foi convertida em exceção silenciosa.
