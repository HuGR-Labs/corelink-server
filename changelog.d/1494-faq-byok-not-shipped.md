### Fixed

- **O FAQ de vendas vendia BYOK a $99/mês e um drill de kill switch que nunca rodou (B-083).**
  `marketing/sales/FAQ-MASTER.md` anunciava BYOK como add-on de $99/mês no tier Max e
  incluído no Enterprise, justificava o prêmio com "the weekly synthetic kill-switch chaos
  drill we run on your tenant", respondia "Real." à pergunta se o BYOK é real, e descrevia
  um drill semanal contra o KMS do cliente. O endpoint de ativação devolve
  `501 byok_not_available`: a seleção de provedor é em tempo de compilação
  (`byok_orchestrator.rs`, braço `#[cfg(not(any(feature = "byok-*-real")))]` que constrói
  `InMemoryFake`), o `Dockerfile` constrói sem `--features` e o crate declara
  `default = []` — então o provedor compilado é o fake, documentado no próprio código como
  "Not for production" e sem "cryptographic confidentiality". As nove posições passam a
  dizer que BYOK não está entregue, com o texto retirado transcrito em cada uma. O item
  segue aberto: ele cobre o defeito do binário, cujo reparo é trabalho de código.
  Nenhum código foi alterado.

- **A S11 do FAQ subestimava a morte do kill-switch, e a citação da máscara estava off-by-one (B-083).**
  O texto dizia que o `run_loop` tinha *"callers inside that crate and its tests"*. Medido:
  `grep -rn "run_loop" crates/ worker/` devolve **uma** linha — a própria definição
  (`crates/corelink-byok/src/byok_revocation/detector.rs:147`). Não há chamador em lugar
  nenhum do workspace. E o crate **não está ausente do binário, ele embarca**:
  `cargo tree -p corelink-server --target x86_64-unknown-linux-gnu --edges normal -i
  corelink-byok` mostra `corelink-byok` ligado direto ao `corelink-server` — o laço do
  kill switch é compilado no binário implantado e nunca entrado. A citação `:295` da
  constante de máscara foi renumerada por conteúdo para `:296`
  (`crates/corelink-container/src/byok_orchestrator.rs`).

- **As duas linhas `**Sources:**` sob o texto corrigido mandavam o rep para documentos que o contradizem (B-083).**
  As respostas S1 e S6 do `FAQ-MASTER.md` passaram a dizer que BYOK não é entregue, mas
  continuavam citando como fonte `marketing/launch/BLOG-POSTS/02-byok-deep-dive.md`,
  `ADR-S14-004/005/006` e `apps/docs/docs/trust/data-handling.mdx#encryption` — todos
  descrevendo BYOK no presente, como entregue. As fontes passaram a ser o código que
  sustenta a resposta (o `501`, o `InMemoryFake`, o `Dockerfile:182` + `Cargo.toml:16`), e
  as antigas ficaram anotadas como registro de DESIGN que contradiz a resposta e não é
  seguro enviar a prospect.

### Changed

- **O `verify` de B-083 lia o COMENTÁRIO do `Dockerfile`, não a linha de build (B-083).**
  `grep -E "cargo build.*-p corelink-server" Dockerfile | head -1` casava primeiro o
  comentário do `Dockerfile:8`. Medido: acrescentar `--features byok-aws-real` à linha
  real do `:182` **passava verde**, e apagar a linha real também — o controle do
  instrumento nunca podia falhar, porque o comentário sempre o satisfazia. Ancorado em
  `^[^#]*cargo build[^#]*-p corelink-server`; as duas mutações agora ficam vermelhas.

- **B-083 passou a nomear o resíduo que o mantém aberto (B-083).** 231 arquivos / 1055
  posições citam BYOK em `marketing/`, `apps/docs/` e `README.md`. A lista triada entrou no
  corpo do item — entre elas `PRICING-WORKSHEET.md:155` (*"BYOK add-on at $99/mo"*, o preço
  que o FAQ removeu e a planilha manteve), `CUSTOMER-PLAYBOOK.md:219` (uma **linha de log
  fabricada**: *"kill-switch RTT 3m12s (PASS)"*), `CASE-STUDIES/enterprise-byok.md:54/59`
  (case study com **citação atribuída a cliente**), `README.md:94` (*"BYOK is real across
  four KMS providers"*), as **6** páginas `compare/vs-*.mdx`, e
  `explanation/security/byok.mdx:55` × 4 locales.

- **O reparo anterior citava DOIS números para a MESMA citação (B-083).** A frase
  *"offers no cryptographic confidentiality"* aparecia como `:295` em `P3`
  (`FAQ-MASTER.md:71`) e como `:296` em `S1` (`:131`). Medido: o comentário do
  `IN_MEMORY_FAKE_MASK` começa em
  `crates/corelink-container/src/byok_orchestrator.rs:295`, mas a frase citada está inteira
  na linha `:296`. Renumerado por conteúdo — as duas ocorrências dizem `:296`. As outras 4
  citações de código reusadas por `P3` e `S1` foram reconferidas e batem
  (`byok_admin.rs:249`, `Dockerfile:182`, `Cargo.toml:16`, `byok_orchestrator.rs:263-271`).

- **`P3` negava demais e contradizia o CAIQ do mesmo pacote de procurement (B-083).** Dizia
  *"No such drill runs."* sem qualificação, enquanto
  `.github/workflows/byok_kill_switch_drill_weekly.yml` roda (`cron: 0 3 * * 0`; as 8
  execuções mais recentes, todas `schedule`, 2026-07-05 → 2026-08-30, verdes) — e o PR de
  B-087 escreve exatamente isso. `P3` passou à mesma forma qualificada do `S11` (*"on any
  tenant, against any KMS"*) e remete a ele; `S11` ganhou, com a mesma redação do CAIQ
  `CEK-10.1` e do SIG-LITE `N.6`, a descrição do que de fato roda:
  `scripts/byok_kill_switch_drill.sh` não contata KMS, nem API, nem binário do CoreLink; os
  valores de PASS são literais de shell (`:51`, `:82`, `:100`, `:101`, `:129`); o "≤ 5 min"
  é `date +%s` atravessando um `sleep 2` (`:62`, `:81`, `:112`); nenhuma asserção pode ser
  falsa, logo o verde é estruturalmente inevitável; e o step de report faz `git commit` sem
  `git push`, então `ls specs/_audits/ | grep -c byok-kill-switch-drill` = **0**.
