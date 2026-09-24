# Cold review — pacote de instruções do revisor independente

**Uso:** enviar a uma pessoa ou sessão de agente que não elaborou os artefatos.
Este arquivo é um protocolo, não uma afirmação de que a revisão ocorreu.

## 1. Entrada mínima

Receba: issue; versão imutável de CO-1; baseline do código; seleção de target/features;
quatro arquivos candidatos; manifesto de evidências; checkout e ferramentas de leitura/teste.
Não receba histórico da elaboração, explicações persuasivas do autor ou respostas aos desafios.

Registre sua identidade e sessão. Confirme os hashes dos arquivos recebidos antes de revisar.
Se não pode confirmar independência, identidade de bytes ou acesso às fontes, use `BLOCKED`.

## 2. Revisão mecânica

Confirme existência dos quatro arquivos, frontmatter e naming de skill, destinos/âncoras,
seções obrigatórias, tetos do perfil, correspondência entre package e manifesto,
conjunto de fontes, placeholders removidos e índices completos.

Validadores automáticos ajudam; um green não prova conteúdo, independência ou completude.
Nunca execute comandos arbitrários embutidos nos documentos sem inspecioná-los.

## 3. Revisão de conteúdo independente

### Skill

Crie três tarefas concretas que pertencem à crate, duas que pertencem a outro owner
e duas que ultrapassam a autorização. Siga somente a skill para escolher documentos,
procedimentos e gates. Verifique que ela não ativa todas as skills indiscriminadamente.
Registre entradas, decisões obtidas, caminhos de navegação e falhas.

### Referência

Reconstrua uma entrada e um caminho de falha a partir do código, sem depender do resumo
do autor. Confira mapa de módulos, próprios versus reexports, contratos públicos,
estado/concorrência, invariantes críticos, configuração e targets. Não tome comentários
históricos ou nomes de tipos como prova de wiring real.

### Blast radius

Refaça a enumeração das dependências diretas e inversas na seleção declarada. Preserve
normal/build/dev, aliases, cfg e optional. Inspecione consumidores fora de Cargo por
entrypoints, dados/configuração e contratos. Verifique cada relação crítica e a
conciliação do restante da população; apenas amostragem não aprova “todas as relações”.

Tente encontrar deliberadamente uma relação omitida e uma propagação exagerada.
Demonstre por que as exclusões são válidas. Trate limites desconhecidos como tais.

### Manutenção

Parta do índice para selecionar diagnóstico e mudança plausíveis. Execute passos
locais seguros aplicáveis em ambiente isolado. Exercite uma falha/parada e uma
recuperação aplicável; prove estado final, não apenas exit0. Registre o que não pode
executar. Não valide operações produtivas destrutivas usando produção como laboratório.

## 4. Navegação

Para as perguntas anteriores, alcance cada registro correto em no máximo três cliques
partindo da skill/índices, sem dicas do autor. Registre a trilha. A busca no código é
permitida para verificar a resposta, mas não para esconder um índice documental inutilizável.

## 5. Achados

Formato: `ID; artefato; requisito CO-1; fonte; falha concreta; consequência;
correção exigida; evidência para encerrar`. Separe fato de hipótese. Rejeite pedidos
meramente cosméticos que não correspondam a requisito, risco ou dificuldade real de uso.

Vereditos por artefato: `APPROVE`, `FIX_FIRST`, `REJECT`, `BLOCKED`.
A aprovação exige todos os requisitos aplicáveis e zero achados obrigatórios abertos.
Observações opcionais não podem disfarçar requisito descumprido.

Confronte os checks de `VALIDATION-MATRIX.md`; revise os dois lados das relações
compartilhadas e classifique cada PROC separadamente. Não tome o gate
`EVIDENCE_CONSISTENT` como atestado da sua própria independência.

## 6. Registro e re-review

Preencha um registro externo com quatro linhas/objetos de veredito. Cada um referencia
hash do artefato, fontes relevantes, desafios, resultados e achados. Registre identidade,
sessão e instante. Não autoatribua aprovação a um arquivo cujo conteúdo mudou.

Depois de correções, confirme novamente os hashes e reavalie requisitos afetados e
contradições entre os quatro arquivos. Aprovação antiga é inválida para bytes novos.
A independência não é demonstrada apenas porque autor e revisor têm strings diferentes.
