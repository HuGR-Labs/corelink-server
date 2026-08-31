### Fixed

- **Catorze workflows usavam grupo de concorrência CONSTANTE com cancelamento em
  voo, o que torna o grupo repo-global.** No `backlog-verify` — o único dos 14 com
  gatilho `pull_request`/`push` — isso fazia **cada PR cancelar o gate dos irmãos**;
  um `push` na `main` chegou a cancelar a execução de um PR. Não é portão lento: é
  portão que **para de ter opinião**, e cancelado parece "sem falha" para quem
  pergunta *"tem falha?"* em vez de contar o conjunto. Explica a leva de checks
  cancelados re-rodados à mão em 2026-08-31, tratados como colateral de carga.
- **Nos outros 13 o efeito é menor mas real**, e a leitura inicial de que "são cron,
  a serialização é desejada" estava **errada**: dois `workflow_dispatch` do mesmo
  workflow em **branches diferentes** se cancelam. Foi o que matou três dispatches
  no mesmo dia. Todos passam a escopar por
  `${{ github.event.pull_request.number || github.ref }}`.
- **`release-slsa3` contradizia o próprio commit de origem.** O commit `50741f7c`
  declara que seis workflows de prod-build/signing/notarize **omitem** o
  cancelamento porque *"cancelar deploy/sign em-flight é pior que enfileirar o
  próximo"*. Cinco honram; este não — uma build de proveniência SLSA L2 em voo
  podia ser cancelada por um segundo release. O flag foi **removido**, restaurando a
  intenção declarada em vez de escopar o grupo.
- **Os 26 grupos constantes com cancelamento desligado NÃO foram tocados**:
  constante + sem cancelamento é o idioma **correto** para singleton diário —
  serializa em vez de matar. Escopá-los por ref permitiria duas execuções da mesma
  noturna se sobreporem.
