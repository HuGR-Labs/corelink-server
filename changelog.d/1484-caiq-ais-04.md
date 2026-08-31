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

- **O mesmo CAIQ vendia BYOK como entregue em 13 linhas, e o SIG-LITE irmão em 2 (B-087).**
  `marketing/sales/legal-questionnaires/CAIQ-V4-pre-filled.md` atestava `CEK-02.1`
  ("optional BYOK envelope"), `CEK-10.1` ("Y — drilled weekly"), `CEK-11.1` ("Y —
  HSM-backed"), `DSP-09.1` (BYOK como medida suplementar de SCC), `BCR-04.1`, `CEK-18.1`
  (citando `apps/docs/docs/security/byok`, caminho inexistente) e mais sete linhas `CEK-*`
  descrevendo um ciclo de vida de CMK do cliente. Medido: `POST /v1/admin/byok/activate`
  devolve `501 byok_not_available`
  (`crates/corelink-container/src/routes/byok_admin.rs:245-257`) e o único provider
  compilado é `InMemoryFake` — e o próprio documento já respondia **P** em `CEK-09.1`
  (*"No BYOK at GA today … gated-inert"*). Registrada a ressalva que a correção exige nos
  dois sentidos: o workflow `byok_kill_switch_drill_weekly.yml` **existe, roda semanalmente
  e está verde** — mas o que ele executa não drila nada (ver a entrada seguinte).
  `marketing/sales/legal-questionnaires/SIG-LITE-2026-pre-filled.md` (`N.4` e `N.6`)
  recebeu a mesma correção.

- **A própria ressalva "honesta" era uma promessa: o drill não drila nem o fake (B-087).**
  O texto anterior dizia que o job *"exercises the `InMemoryFake` provider"* e que o
  `byok-kill-switch-rtt ≤ 5 min` era *"a measurement of the fake"*. Medido em
  `scripts/byok_kill_switch_drill.sh` (181 linhas): o script **não invoca binário algum do
  CoreLink**, então não toca nem o fake, e todo valor de PASS é literal de shell —
  `TENANT_STATUS="active"` (`:51`), `DETECTED=true` (`:82`), `DEK_CACHE_EMPTY=true`
  (`:100`), `TENANT_STATUS_POST="degraded_read_only"` (`:101`), `RECOVERY_STATUS="active"`
  (`:129`); as chamadas de KMS existem só como comentário (`:64-67`, `:125`). O "≤ 5 min" é
  `date +%s` antes e depois de um `sleep 2` (`:62`, `:81`, `:112`) e **não mede coisa
  alguma**; nenhuma asserção pode ser falsa, logo o verde é estruturalmente inevitável. E
  nenhuma execução deixou evidência: o step "Commit drill report" faz `git commit` **sem
  `git push`** (workflow `:56-68`), e `ls specs/_audits/ | grep -c byok-kill-switch-drill`
  = **0**. População do verde: as 8 execuções mais recentes do workflow, todas `schedule`,
  2026-07-05 → 2026-08-30, todas `success`. CAIQ `CEK-10.1` e SIG-LITE `N.6` reescritos
  para dizer isso.

- **No SIG-LITE, duas linhas ainda vendiam o BYOK que o mesmo PR retirava (B-087).**
  `K.10` ainda oferecia *"supplementary measures (BYOK envelope encryption, EU-region
  pin)"* — exatamente a afirmação pela qual o `DSP-09.1` do CAIQ foi corrigido nesta mesma
  passada — e `N.2` ainda listava AWS / GCP / Azure / HashiCorp Vault como *"customer-side
  BYOK KMS providers"* em serviço. `K.10` passa a declarar **uma** medida suplementar (o
  pin de região, guarda fail-closed servido) e registra que a TIA Schrems II precisa ser
  refeita sem as chaves do cliente; `N.2` passa a dizer que, com BYOK inerte, nenhum tenant
  alcança esses provedores e nenhum deles guarda chave por conta do CoreLink.

- **`LOG-03.1` atestava imutabilidade da trilha de auditoria; o R2 não tem Object Lock (B-087).**
  Passou a "P" e descreve *tamper-evident, não imutável*: a cadeia BLAKE3 append-only com
  cabeça assinada **detecta** adulteração na verificação, não a impede. O DPA executado
  (`legal/dpa/v1.0.0.en-US.md:110-111`) herda a mesma afirmação e **não foi emendado** —
  ato jurídico é do owner. Passou a ser justamente a condição que o `verify` de B-087 mede.

- **`SEF-03.1` atestava detecção 24×7 via página sintética semanal que não dispara em produção (B-087).**
  O cron de segunda 14:00 UTC vive no `[triggers]` default e no `[env.staging.triggers]`;
  o `wrangler.toml:262-264` diz no próprio comentário que *"production env intentionally
  omits `[triggers]`"*, e a imagem do pager sintético segue com `<PIN_AT_RELEASE>`. Passou
  a "P". A rotação PagerDuty 24/7 não é mensurável do repositório — a linha não a afirma
  nem a nega.

### Changed

- **O `verify` de B-087 era incapaz de ficar vermelho (B-087).** Com `ys=0` permanente ele
  saía sempre `exit 0` "ok-parcial", e a razão declarada de o item seguir aberto não era
  medida por nada — tendo ainda escapado do relógio de 14 dias, que `backlog_verify.py:196`
  só aplica a `verify: manual`. Agora gateia a condição **remanescente**: enquanto o DPA
  declarar *"immutable R2 with Object Lock"* o item fica `open`; quando parar, o comando
  fica **vermelho** mandando fechá-lo ou renomear a razão. Três controles do instrumento
  impedem que sumiço e conserto compartilhem saída (contagem das 9 linhas do CAIQ, a
  existência de `N.6` no SIG-LITE, e uma string de controle mais fraca no DPA).
