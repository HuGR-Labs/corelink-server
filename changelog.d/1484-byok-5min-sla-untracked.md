### Changed

- **Um SLA assinado promete 5 minutos para uma feature que devolve `501`, e nenhum comando
  media isso (B-087).** O `SLA_SECONDS=360` (6 min) de `scripts/byok_kill_switch_drill.sh:34`
  é número interno. O **5** vem de instrumento assinado: `legal/sla/v1.0.0.md:46`, linha
  Enterprise, *"BYOK kill-switch p99 ≤ 5 min"* (com créditos de serviço e *enhanced
  remedies* atrás dela), e `legal/dpa-residency-amendment.md:230`, linha **Right to
  Erasure**, *"Customer revokes CMK (BYOK kill switch) → crypto-erase ≤ 5 min globally"* —
  isto é, o kill-switch é oferecido como **o mecanismo de Direito ao Esquecimento do
  GDPR** — repetido em `:351`. Contra isso, `POST /v1/admin/byok/activate` devolve `501
  byok_not_available`. O achado estava **nomeado em prosa** (o corpo de B-083 cita a linha
  do SLA e roteia para B-087) e **medido por nada**: o `verify` de B-087 lia só a string
  *"immutable R2 with Object Lock"* em `legal/dpa/v1.0.0.en-US.md`. O `verify` passa a
  contar **duas** razões independentes e fica `open` enquanto qualquer uma casar. Este PR
  **não emenda instrumento jurídico assinado** — só mede que ele afirma o que afirma;
  emendar é do owner.

- **A mensagem de reprovação de B-087 oferecia "feche o item" como primeira opção (B-087).**
  O ramo "vermelho por progresso" também dispara numa reescrita meramente cosmética dos
  instrumentos, e "feche" em primeiro lugar é caminho para dar o item por resolvido com a
  alegação viva, apenas reformulada. A ordem foi invertida: **reescreva o item nomeando a
  razão que sobrou** primeiro, fechar por último e só se nenhuma sobrar.

### Fixed

- **Citação de linha já defasada na `main` (B-087).** `CEK-10.1` do CAIQ citava o step
  "Commit drill report" como `(workflow :56-68)`. Um commit posterior acrescentou 3 linhas
  ao bloco `concurrency:` de `.github/workflows/byok_kill_switch_drill_weekly.yml`, e na
  `main` o step está em `:59-71` com o `git commit` em `:69` — o range citado passou a
  cobrir o step de **execução**. Passou a ser citado **pelo nome do step**, que não se
  desloca.

- **Precisão do que o drill de fato mede (B-087).** CAIQ `CEK-10.1` e SIG-LITE `N.6` agora
  registram que `DETECT_LATENCY_S` (`:75` → `:86`) e `TOTAL_S` (`:62` → `:112`) atravessam
  o **mesmo** e único `sleep 2` — não são duas medidas independentes — e que os dois
  `sleep 1` (`:126`, `:128`) rodam depois de `:112`, fora das duas.
