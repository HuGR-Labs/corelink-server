### Fixed

- **B-080 — metade do catálogo canônico de escopos de PAT era decorativa; removida em vez
  de fingida.** `crates/corelink-pat/src/scopes.rs` publicava doze escopos canônicos, dos
  quais seis (`admin:tenant-read`, `admin:tenant-write`, `admin:tokens`, `admin:billing`,
  `admin:audit`, `admin:users`) não eram consultados por **nenhum** ponto de aplicação: um
  token com `admin:audit` recebia exatamente o privilégio de um sem ele, e qualquer plano
  de menor-privilégio escrito sobre esses nomes era decorativo. Os seis foram **removidos**
  do catálogo e colapsados no único nome que é de facto cunhável, persistível e aplicado —
  `admin`. A evidência que escolheu remover em vez de aplicar: (i) **não eram cunháveis** —
  o classificador self-serve (`scope::classify_requested_scopes`) devolve `Err` para
  qualquer token `admin:*`, e o mint interno (`routes::internal_pat::scope_label_to_bits`)
  só aceita os rótulos `admin`/`cas:rw`/`read-write`/`read-only`, ligando os seis bits
  apenas **em bloco** via `SCOPE_ADMIN_ALL`; (ii) **não eram persistíveis** — `pat.scope` é
  `TEXT CHECK (scope IN ('read-write','read-only','admin'))` (`migrations/d1/0037`) e não
  existe coluna de bitset, de modo que o `PatScopes` u64 nunca sai da memória do mint;
  (iii) **o consumidor natural já é aplicado sob outro vocabulário** —
  `scope::requires_billing_admin` (`billing`/`admin`/`owner`) gateia
  `/v1/customer/billing*` e `/v1/customer/audit`, e o dashboard admin autentica por sessão
  Clerk, não por PAT. Implementar o enforcement dos seis teria ensinado o ponto de
  aplicação a reconhecer strings que nenhum cunhador emite e que o D1 não sabe guardar —
  além de abrir um caminho **novo** para billing (`requires_billing_admin("admin:billing")`
  passaria a ser `true`), ou seja, aumento de superfície disfarçado de reparo.

### Added

- **Guarda de classe para nomes de escopo decorativos.**
  `crates/corelink-container/tests/scope_catalog_closure.rs` reprova qualquer nome
  publicado no catálogo canônico de `corelink-pat` que nenhum predicado de
  `corelink_server::scope` consulte — fechando a **classe**, não só a instância. Traz
  calibração explícita do reconhecedor (controles positivo e negativo, sem os quais um
  reconhecedor que devolvesse sempre `true` deixaria o teste vacuamente verde),
  anti-vacuidade (catálogo vazio reprova), a **prova de negação** que o item exigiu (cada
  nome removido é recusado em billing, cache read/write, find-missing e no cunhador, com
  controles provando que os predicados não recusam tudo) e uma ledger de exceções
  declaradas que não pode envelhecer em silêncio. A ledger registra `cache:delete`,
  `execute:action` e `report:result` — descobertos no mesmo levantamento e igualmente sem
  consumidor algum, o que faz **9 de 12** nomes decorativos, e não 6 — como capacidades não
  construídas, tornando a dívida visível em vez de invisível.

### Changed

- **`auth_model.md` §3.1 passa a distinguir escopo APLICADO de escopo apenas planejado**
  (v0.2.0 → v0.3.0). Nova §3.1.1 registra por que os seis `admin-*` granulares nunca
  saíram do papel e por que a granularidade que eles prometiam já existe sob outros nomes.
  Como a mudança **remove** alegação não implementada e não expande superfície de
  segurança, §3.4/FF-HR-005 (ADR + security review para **adicionar** escopo) não dispara;
  reintroduzir qualquer um dos seis continua sujeito a §3.4 na íntegra.
- **`marketing/corelink-feature-catalog.html`** deixa de vender treze escopos como
  enforcement de menor-privilégio entregue: o card agora nomeia o que é aplicado hoje
  (cache read/write/find-missing e admin) e diz explicitamente que os três reservados não
  concedem nada.
