### Fixed

- **O critério de completude do WP-6 era falso-vermelho por construção.** `gh pr view --json files` devolve 100 de 173 arquivos sem aviso de truncagem, e na forma REST o campo é `filename` — pedir `.path` devolve linhas vazias sem o `jq` reclamar. Era a regra "nunca conclua ausência a partir de saída truncada" falhando dentro do comando escrito para aplicá-la. Também corrige três números que não reproduziam.
