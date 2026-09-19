# Evidências, cobertura e limitações

## Proveniência

Código: `main` em `cca798ff5bc2df660ecf2570ed243eb9775ff3d0`. Identidade GitHub: `HuGR-Labs/corelink-server`, repository ID `1232040291`. O checkout de outra frente estava sujo; a entrega foi criada em branch/worktree isolado, sem stash, reset ou limpeza do trabalho alheio.

O [snapshot](evidence/github-snapshot.json) contém timestamps por endpoint e terminou em `2026-09-19T21:02:31.763285+00:00`. Ele foi coletado por GET, com paginação. A configuração live pode mudar a qualquer instante; a execução futura exige nova coleta. Este pacote não certifica o estado atual de fornecedores externos.

## População medida

| Medida | Resultado e interpretação |
| --- | --- |
| Entradas Git na árvore-base | 7.641; não inclui arquivos não rastreados ou alterações de outros branches. |
| Bytes de blobs lidos | 81.188.772. |
| Blobs textuais UTF-8 examinados | 7.628. |
| Symlinks | Um blob de symlink lido; alvo não seguido. |
| Arquivos binários/não UTF-8 | 13, com path/object/hash no inventário; conteúdo interno não analisado pelo scanner. |
| Linhas candidatas | 3.063 em 819 arquivos; NÃO significa 3.063 substituições necessárias. |
| Classificação heurística | 1.998 docs; 515 código/config; 235 testes/exemplos; 169 workflows; 146 histórico. Cada classe exige revisão semântica. |
| Refs remotas observadas | 132 heads, 52 tags; preservar object SHA/tipo no freeze futuro, não usar essas contagens como constante. |
| PRs/releases observados | Sete PRs abertos (drafts) e uma release no server. Releases da CLI pertencem ao outro repo. |
| Workflows fonte/API | 138 arquivos; 147 registros active. Três paths dinâmicos da plataforma e seis paths de arquivo ausentes na árvore explicam o delta. |
| Repo settings | 18 secrets por nome, sete variáveis sem valores; duas environments; cinco runners Mac; duas Apps organizacionais. |

O [source-inventory.json](evidence/source-inventory.json) contém hashes e localizações, sem reproduzir linhas potencialmente sensíveis. Guarda hash SHA-256 do blob e de cada linha candidata, além do object ID Git. O resultado é determinístico para commit/manifesto/versão do scanner fixados.

Os 13 arquivos não textuais incluem três distribuições publicadas (`.zip`, `.tgz`, `.whl`), imagens e seis rulesets comprimidos. Não foram descompactados pelo scanner. Tratar as distribuições históricas como bytes imutáveis; mudanças de metadata futura exigem outra versão, não edição retroativa desses arquivos. A ausência de LFS pointers/gitlinks/oversize no scan da árvore-base não prova ausência desses elementos em todo o histórico ou em outros branches.

## Inventários complementares

[workflow-surface.json](evidence/workflow-surface.json) foi extraído com PyYAML BaseLoader, percorrendo valores YAML em vez de comentários. Inclui arquivos, seletores de jobs, seis workflows com `id-token: write` e mapeamento de 82 nomes de secrets/30 variáveis para consumidores. É análise sintática de expressões literais; referências dinâmicas e condições precisam de interpretação. Não contém valores e não diagnostica secrets faltantes por subtração de contagens.

[workflow-api-delta.json](evidence/workflow-api-delta.json) separa registros da plataforma de paths ausentes no commit. Não transforma registro active em prova de execução e não instrui desativação automática. [dependency-identities.json](evidence/dependency-identities.json) documenta leituras mínimas dos três repos externos, sem auditar ou alterar seu conteúdo.

## Resultados não verificáveis e lacunas de escopo

| Superfície | Resultado | Tratamento |
| --- | --- | --- |
| Rulesets | HTTP 403 | Não declarar lista vazia ou enforcement. Revalidar capacidade/política do destino. |
| Protection main | HTTP 403 | Mesmo tratamento; o branch metadata não substitui prova de regras aplicáveis. |
| Codespaces secrets | HTTP 404 | Pode ser acesso/capacidade/ausência; permanece UNVERIFIED. |
| Organização destino | Não fornecida | G01/G02 bloqueados. |
| Policies/trust/integrações em providers | Não auditados live | Inventário por fornecedor em G05/G06; fonte não prova estado implantado. |
| GitHub Packages efetivos | Não inventariados live | Não inferir uso de GHCR por exemplos; verificar por pacote quando aplicável. |
| Inbound references de outros repos/usuários | Não auditadas exaustivamente | Handoff dos responsáveis; scope de leitura deste trabalho é o server e metadata dos irmãos. |
| Runs no novo owner e observação | Não executados | G09/G10 pertencem à execução futura. |
| Compatibilidade real do matcher Rust | Código inspecionado, sem reprodução em produção | Tratar como hotspot de migração e exigir teste do caminho consumidor em WP-02. |

Uma referência em fonte é uma evidência concreta de acoplamento, não prova automática de que aquele trecho foi executado em produção. Um GET bem-sucedido lista a população daquele endpoint e instante, não todos os sistemas vinculados.

## Validação da entrega

Os primeiros 27 testes focais passaram, cobrindo plano sem processos, scope/IDs/owners, árvore suja, symlinks, binary/LFS/limite, determinismo, delta, paginação, redação, erros HTTP, timeout e alias. A revisão de robustez adicionou controles para tipos/formas inválidas de manifesto. O resultado final, incluindo integridade documental, é registrado em [validation.json](evidence/validation.json).

A revisão efetuada nesta entrega é **self-review**, não aprovação independente. Testes locais do kit não comprovam toda a aplicação, não são ensaio de transferência e não substituem CI remoto ou revisão de supply chain dos patches de WP-02. Consultar o PR para o head e estado de checks; não deduzir verde remoto de um log local.

## Reprodução e conservação

Executar os comandos do README/RUNBOOK em diretório temporário novo. Comparar o JSON do scan com o inventário deste commit; uma coleta nova de GitHub deve permanecer separada do snapshot histórico. Não atualizar timestamps para parecer recente sem coletar novamente. Regenerar a análise YAML quando workflows mudarem e registrar parser/método/SHA.

A entrega não modificou runtime, credentials, políticas GitHub, Apps, repositórios irmãos ou ownership. O objetivo entregue é documentação e ferramental de preparação. **Não houve transferência; não há declaração de prontidão para transferir.**
