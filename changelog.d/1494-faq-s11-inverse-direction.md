### Fixed

- **S11 do FAQ negava a direção inversa pela quarta vez: o laço nunca rodou, mas o CORPO
  dele é testado (B-083).** O texto dizia *"the kill-switch loop has never run … Not the
  released binary, not another crate, **not a test**"*. Verdadeiro para `run_loop` —
  `grep -rn --include='*.rs' "run_loop" .` sobre **todo `.rs` do repositório** devolve
  exatamente **1** linha, a própria definição. **Falso** para o laço de revogação:
  `RevocationDetector::run_one_cycle`, que `run_loop` chama a cada iteração via
  `run_cycle_inner`, tem **5** call sites de teste na mesma população —
  `crates/corelink-byok/tests/byok_revocation_prop_revocation.rs:101` e `:288`,
  `crates/corelink-byok/tests/byok_revocation_adversarial.rs:120` e `:158`, e
  `tests/e2e-byok-revoke/tests/happy_revoke_flow.rs:139` — e o doc-comment em
  `crates/corelink-byok/src/byok_revocation/detector.rs:144` diz *"Use `run_one_cycle` in
  tests"*. Um auditor hostil derrubaria a frase e levaria junto a conclusão correta. A
  resposta agora separa as duas: a **lógica** por ciclo é coberta por testes de propriedade
  e adversariais; o que nunca rodou é o **laço que a acionaria em produção**, e nenhum
  drill jamais a exerceu contra KMS ou tenant reais.

- **S11 afirmava compartilhar a *redação* com o CAIQ e o SIG-LITE; compartilha os FATOS
  (B-083).** `N.6` omite dois dos literais de shell, o CAIQ acrescenta uma citação de
  código e um fecho, e só o FAQ diz textualmente que o drill não prova nada. Trocado
  `wording` por `facts`, com a divergência declarada — de outro modo a frase convida a
  procurar uma sentença compartilhada que não existe.

- **Citação da máscara desalinhada entre o FAQ e o `BACKLOG.md` (B-083).** O FAQ cita
  `crates/corelink-container/src/byok_orchestrator.rs:296` (correto — a frase *"offers no
  cryptographic confidentiality"* está inteira em `:296`) e a nota do B-083 citava `:295`,
  que é só a primeira linha do doc-comment. Alinhado **por conteúdo**.

### Changed

- **Resíduo de BYOK nomeado no Trust Center, agora rastreado (B-083).**
  `apps/docs/docs/trust/subprocessors.mdx:105-113` tabela AWS KMS / GCP Cloud KMS / Azure
  Key Vault / HashiCorp Vault como sub-processadores em *"Customer-controlled CMK (BYOK
  option)"*, precedida de *"CoreLink only holds wrapped DEKs"* — falso hoje. **Não
  reparada aqui** (é do PR que varrer o resíduo documental de 231 arquivos / 1055
  posições), mas **nomeada** na triagem do item, porque achado que fica só em prosa é
  achado que nunca vira item. População medida: 25 PRs abertos / 72 branches remotas; dois
  PRs tocam o arquivo (#1490 em `:133+`, Sigstore; #1397 no cabeçalho e nos canais de
  notificação, a partir de base defasada) e **nenhum toca a tabela**.

- **A mensagem de reprovação de B-083 mandava "feche o item" e nada mais (B-083).**
  Contradizia o próprio `verify-means`: compilar a feature `byok-*-real` é condição
  necessária, não suficiente — [B-084] cobra um drill REAL contra um KMS real, e o resíduo
  documental não é lido pelo comando. Agora manda **reavaliar** primeiro, nomeando o que
  falta; fechar vem por último. Q-7 remedido nos dois lados por mutação (estado real →
  `CONFIRMED`; feature na linha real de build, linha real apagada, e `default` com byok →
  `DRIFTED`), com a saída registrada no `verify-means`.
