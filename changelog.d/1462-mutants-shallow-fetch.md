### Fixed

- **A lane `cargo-mutants (changed lines only)` desfazia o próprio `fetch-depth: 0`
  e ficava vermelha sem relação com o código do PR.** O checkout pede história
  completa **de propósito** — o comentário no arquivo diz "needed for the base
  diff" — e o passo seguinte fazia `git fetch --depth=1`, que **converte o
  repositório de volta para raso** e joga fora exatamente a história recém-buscada.
  O sintoma é `fatal: origin/main...HEAD: no merge base` e exit 128, e ele aparece
  quando o branch foi rebaseado ou a base andou o bastante — ou seja, nos dias
  movimentados, e nunca num dia parado. Removido o `--depth=1`.
- **A mesma lane passaria vacuamente se o diff saísse vazio por outro motivo.**
  Com `pr.diff` vazio, o `grep` de fontes Rust não casa e o passo sai com
  `exit 0` imprimindo "0 mutants, passing" — verde tendo testado nada. Acrescentado
  um `git merge-base` explícito que **falha alto** antes de calcular o diff.
