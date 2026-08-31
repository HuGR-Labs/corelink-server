### Fixed

- **O CAIQ v4 de procurement atestava "Y" para testes de segurança que não rodam (B-087).**
  `marketing/sales/legal-questionnaires/CAIQ-V4-pre-filled.md`, linha `AIS-04.1`,
  respondia "Y" justificando "CodeQL + Semgrep (custom rules) on every PR; cargo-fuzz
  daily". Nenhum dos três gatilhos existe: `codeql.yml` não tem lane `pull_request` (é
  cron noturno + dispatch), o cron do `semgrep.yml` está comentado desde 2026-08-10 após
  0 de 8 execuções bem-sucedidas, e o cron do `fuzz-nightly.yml` está comentado com o
  dispatch declarado no próprio arquivo como o único gatilho. A linha passa a "P" e nomeia
  o que de fato gateia um PR que toca Rust — clippy `-D warnings`, `cargo test`,
  `cargo-audit`, `cargo-deny` — com a ressalva de que o filtro de `paths` desses lanes não
  alcança um PR só de documentação. As outras duas linhas do item (`STA-08.1`,
  `STA-11.1`) já haviam sido corrigidas em purga anterior. Nenhum código foi alterado.
