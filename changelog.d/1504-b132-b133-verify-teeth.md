### Fixed

- **O `verify` do B-133 casava com o próprio comentário do workflow.**
  `grep -q "ci-use-host-toolchain.sh"` em `.github/workflows/dependabot-policy.yml`
  casava a linha 144 — prosa (*"Full reasoning: scripts/ci-use-host-toolchain.sh
  header."*) — e não o `run:` da linha 147. Dois mutantes sobreviviam, inclusive **o
  conserto que o próprio item recomenda** (rodar o script a partir do checkout do ref
  BASE, `bash _base/scripts/…`): um item de segurança cujo portão não reconhece o próprio
  reparo fica aberto para sempre, ou é fechado à mão sem prova. O predicado passa a
  ancorar o `run:` (`^[[:space:]]+run: bash scripts/ci-use-host-toolchain\.sh[[:space:]]*$`,
  1 casamento hoje); `run: echo skipped` e o caminho `_base/scripts/…` agora ficam ambos
  DRIFTED.
- **O B-132 prometia evidência SOC 2 e o portão media roteamento.** O item se chamava "a
  evidência SOC 2 está com lacuna", mas o `verify` só confirma que o braço `schedule` do
  `secrets-drift.yml` roteia para `ubuntu-latest`. Efeito: mover o braço para a frota
  **fechava** o item sem ninguém provar que a evidência voltou a ser produzida, e
  consertar o faturamento — o defeito real — **não** fechava. O título foi reescrito para
  o que o portão de fato mede, e o `verify-means` passa a declarar o que fechar não prova
  e que o defeito subjacente (todo job `ubuntu-*` não inicia por bloqueio de faturamento)
  segue sem item próprio. Nenhum portão novo sonda SOC 2.
