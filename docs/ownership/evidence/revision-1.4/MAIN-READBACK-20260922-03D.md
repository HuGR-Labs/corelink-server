# Readback de `main` — 2026-09-22 (`03d30712`)

`git ls-remote origin refs/heads/main` retornou:

`03d30712478bc0c191a8c2bb36378ddaba4f5f67`

Desde `791f474d`, o delta reproduzível é de 121 arquivos, 4.554 linhas
adicionadas e 1.273 removidas. Além de workflows, backlog e contratos, ele
altera superfícies relevantes para os documentos: `corelink-container`,
`corelink-billing-stripe-materializer`, testes de quota/billing, Stripe real,
auditoria e migrações D1. Os cinco pilotos deixam de estar integralmente
source-stable; billing/server/materializer exigem novo readback antes de
qualquer aprovação. Nenhuma execução ou alcance é inferido deste diff.
