### Fixed

- **O portão do backlog não conseguia responder às próprias perguntas.** Quatro `verify` consultam histórico do git (`merge-base`, `rev-list`, `git log`) e o `actions/checkout` clonava raso, sem histórico nenhum — então esses checks reportavam verde sem verificar. Agora clona com `fetch-depth: 0`, e o guard de clone raso **reprova** em vez de pular, porque um check que não consegue responder é pior que um vermelho: ele mente.
