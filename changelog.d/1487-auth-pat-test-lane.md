### Added

- **Lane de CI que executa os testes de `corelink-auth` e `corelink-pat`.** As duas
  primitivas de autenticação do produto — WebAuthn e OTP de recuperação num lado,
  Argon2id e verificação de credencial no outro — carregam ~438 funções de teste e
  não tinham **nenhuma** execução de teste em CI. Apareciam num único workflow
  (`mutation-nightly.yml`), e lá só como matriz de mutação; essa lane é hosted, sem
  `schedule:`, e soma 0 sucessos em 35 execuções. A lane nova roda `clippy -D
  warnings` e `cargo test --all-targets` nos dois crates, em `runs-on: corelink`
  (frota Linux efêmera do produto, faturamento zero), disparada por `pull_request`
  com `paths:` — sem `push: main` e sem cron.

### Fixed

- **`welcome-first-pr.yml` não promete mais um portão que não existe.** O texto de
  boas-vindas dizia a todo primeiro contribuidor que a CI roda `cargo build
  --workspace`, `cargo clippy --workspace` e `cargo test --workspace` — *"CI runs
  all three"*. A terceira é falsa: os portões de PR são crate-scoped. O texto agora
  pede que ele rode os três localmente e diz que o teste de workspace não faz parte
  do portão hoje.
