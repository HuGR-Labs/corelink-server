### Fixed

- **O gate `backlog-verify` usava grupo de concorrência CONSTANTE com
  `cancel-in-progress`, então todo PR do repositório dividia um único slot e o
  próximo PR a tocar o `BACKLOG.md` cancelava o gate do anterior.** Não é portão
  lento — é portão que **silenciosamente para de ter opinião**, e um check
  cancelado parece "sem falha" para quem pergunta *"tem falha?"* em vez de contar
  o conjunto. Explica a leva de checks `CANCELLED` re-rodados à mão em 2026-08-31,
  que estavam sendo tratados como colateral de carga da máquina. O grupo passa a
  variar por `github.ref`. Dos 14 workflows com grupo constante, este é o **único**
  com gatilho `pull_request`; nos outros 13 (cron/dispatch) a serialização é
  desejada e nada foi tocado.
