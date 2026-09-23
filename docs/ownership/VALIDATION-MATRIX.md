# Matriz de aceitação CO-1 — v1.1 candidata

Uma linha automática não substitui a revisão manual adjacente. O dono de cada gate
mecânico é CO-COMMON; o executor da parte manual é o cold reviewer, independente do autor.

| Requisito | Automático entregue | Evidência manual para encerrar |
|---|---|---|
| Identidade package ≠ pasta | normalize_metadata + confronto do manifesto | Censo Cargo real, completo e na baseline correta |
| Fuzz/independentes/vendor | classify_manifest; decisão explícita; metadados separados | Escopo completo dos manifestos rastreados e motivos das exclusões |
| Slug/pasta/frontmatter | Um normalizador; colisão recusa; render usa slug | Renomeações e aliases antigos reconciliados |
| Quatro arquivos | Schema + tipos únicos + paths canônicos | Conteúdo específico, não quatro documentos genéricos |
| YAML e tipos | SafeLoader com chave duplicada recusada + JSON Schema | Consistência do vocabulário de domínio |
| Tipo/documento/perfil/baseline | check_docs + metadados contra registro | Seleção de target e features efetiva |
| Limites totais | Linhas, palavras e bytes simultâneos | Cinco pilotos completos; legibilidade e cobertura real |
| Limites das fichas | H3 renderizado; âncora interna não corta a contagem | Nenhuma omissão para caber; atomicidade da família |
| Overflow de uma única ficha | Mesmos tetos rígidos; erro de capacidade | Medição da ficha, remoção de redundância e revisão do teto compartilhado |
| Resumo e parágrafos | Soma do lead; limite de parágrafo renderizado | Clareza, objetividade, ausência de prosa vazia |
| Seções e âncoras | AST CommonMark; código/comentários não contam | Correspondência do título com seu conteúdo |
| Índices e retorno | Registro indexado antes da ficha e link de retorno | Percurso real entre arquivos em até três cliques |
| Links locais | Root seguro, arquivo/âncora explícita presentes | Âncoras automáticas antigas e links externos no renderer real |
| Frontmatter skill | Naming, description ≤600, metadata de strings; sem allowed-tools | 3 gatilhos positivos, 2 negativos, 2 recusas de autoridade |
| API/invariantes/fluxos | Estrutura e limites; IDs nas fichas | Símbolos e enforcer reais; predicados falsificáveis, estados finais |
| Relações diretas/inversas | Metadados declarados preservam aliases/kind/cfg | Recenso e trace semântico por build válido |
| Relações fora de Cargo | Busca literal registrada, sem execução de comandos | SQL/TS/config/wire/geração/dados compartilhados e consumers externos |
| Relação nas duas crates | Assinatura compartilhada, owner, direções e peers | Contrato correto e impacto local explicado nos dois documentos |
| Prova de fonte | Hash atual; CLI confere fonte na baseline ancestral Git | Linha/símbolo realmente sustenta a afirmação, não só wrapper |
| Evidências de revisão | Arquivos/hash/refs requeridos e quatro checks conjuntos | Origem real da revisão, sem respostas do autor antecipadas |
| Independência | Identidade/sessão diferentes, atestado e evidência | Confirmar contexto novo; strings não demonstram independência |
| Quatro APPROVEs | Exatamente quatro, findings obrigatórios resolvidos | Todos os requisitos materiais, não apenas o mínimo mecânico |
| Manual e PROCs | IDs exatos, estado de execução por PROC, evidência tipada | Procedimentos locais aplicáveis executados; falha/parada/recuperação |
| Produção não executada | Não aceita evidência local como operação produtiva | Alcance da aprovação documental limitado explicitamente |
| Anti-drift | Documento/fonte/manifesto/match novo/termos mudados invalidam | Reavaliar adequação das buscas e alterações semânticas |
| Mudança não relacionada | Fixture negativa de invalidação global passa | Revisão do alcance dos termos de busca |
| Integração paralela | Agregação ordenada; duplicatas/conflitos recusados | Backlog e gates de merge existentes conciliados |
| Preparação de issue | Renderer recusa seed vazio, slug errado e URL mutável | Fatos, riscos, comandos e conceitos realmente estudados |
| Deduplicação | Marcadores antigos/atuais e matches abertos/fechados | Coleta completa; aliases/texto/backlog e decisão registrada |
| Timeout/readback | UNCERTAIN bloqueia retry; body/hash/ID conferidos | Leitura real após write; resolução da ambiguidade pelo operador |
| Integração OKF | Nenhuma alegação de integração automática | Implementar vínculo no repo sem alterar contrato OKF congelado |
| CI/merge/runtime | Não certificados pelos helpers | Gates reais e evidência operacional pertinente |

As fixtures de gate usam atores, SHAs, documentos e resultados **sintéticos**. Elas
provam comportamento dos validadores, não completude de crates nem cold review real.
