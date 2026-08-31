### Fixed

- **`release-cli.yml` publica num repositório que existe.** As cinco ocorrências
  de `HumanGuardrail/corelink-cli` no arquivo — incluindo os dois `--repo` do
  `gh release create` e do `gh release upload` — nomeavam um repositório que
  devolve **404**. O controle `HuGR-Labs/corelink-cli` resolve. O job `release`
  não poderia ter sucesso nem com a matriz de build inteiramente verde, e o
  comentário logo acima já dizia, desde 2026-08-02, que a casa canônica era
  `HuGR-Labs/corelink-cli` — só o código nunca foi atualizado.

  As referências a `HumanGuardrail/corelink-server` no mesmo arquivo **não** foram
  tocadas: elas resolvem por redirecionamento permanente do GitHub. `corelink-cli`
  não tem esse redirecionamento porque veio de uma conta pessoal, não daquela org
  — que é precisamente por que um 404 e o outro não.
