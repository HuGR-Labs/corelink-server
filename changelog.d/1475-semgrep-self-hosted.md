### Fixed

- **`semgrep.yml` roda de novo — a linha `runs-on:` ficou para trás numa
  reescrita e custou 388 execuções sem um único sucesso.** O corpo do job foi
  portado para a frota macOS self-hosted em 2026-06-02 (sem docker, sem
  `container:`, `python3`/`pip3` do host em vez de `actions/setup-python`, cujo
  `RUNNER_TOOL_CACHE` é ingravável na frota), e o comentário logo acima do
  `runs-on:` descreve exatamente essa portabilidade — mas a linha em si continuou
  `ubuntu-latest`. Com os minutos GitHub-hosted bloqueados por faturamento desde
  2026-08-24, todo dispatch passou a pedir um box que a org não recebe. Repontada
  para `[self-hosted, mac, corelink-builder]`; nada no corpo do job mudou.

  O `BACKLOG.md` B-110 foi reescrito no mesmo commit: ele cobria cinco lanes e seu
  `verify` exigia que **todas** seguissem hosted, então ficaria vermelho no merge
  seguinte. Agora cobre as quatro restantes (`cas_foundation`, `coverage`,
  `ffi-matrix-ci`, `mutation-nightly`) e carrega uma catraca que exige que a
  `semgrep` permaneça self-hosted. Corrige também um fato errado do item: ele
  afirmava que a lane declarava `runs-on: [self-hosted, Linux, X64]` — esse texto
  vivia num **comentário** descrevendo o valor anterior, não no código.
