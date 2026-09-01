### Added

- **Lane de CI que executa os testes de `corelink-auth` e `corelink-pat`.** As duas
  primitivas de autenticação do produto — WebAuthn e OTP de recuperação num lado,
  Argon2id e verificação de credencial no outro — passam a ter um portão de PR
  dedicado: `clippy -D warnings`, testes debug `--all-targets`, e o alvo
  `constant_time` de `corelink-pat` em release. A lane roda em `runs-on: corelink`
  (frota Linux efêmera do produto) e sua população de gatilhos inclui as migrações de
  schema incorporadas pela cadeia compilada, manifests/lockfile, toolchain/config e
  a cadeia local de crates compilada
  pelos testes. O harness `emit_e2e_seed` continua deliberadamente fora deste
  portão: requer chave de assinatura e emite uma credencial/SQL; sua classificação e
  execução segura pertencem ao B-068.

### Fixed

- **`welcome-first-pr.yml` não promete mais um portão que não existe.** O texto de
  boas-vindas dizia a todo primeiro contribuidor que a CI roda `cargo build
  --workspace`, `cargo clippy --workspace` e `cargo test --workspace` — *"CI runs
  all three"*. A terceira é falsa: os portões de PR selecionam lanes por superfície
  de dependência, não uma única execução de `cargo test --workspace`. O texto agora
  pede que ele rode os três localmente e diz que o teste de workspace não faz parte
  do portão hoje.
