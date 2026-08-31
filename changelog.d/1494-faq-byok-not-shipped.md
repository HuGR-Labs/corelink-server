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
