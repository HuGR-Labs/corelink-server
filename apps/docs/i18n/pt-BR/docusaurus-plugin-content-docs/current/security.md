---
id: security
title: Modelo de segurança
sidebar_position: 11
description: Ciclo de vida do PAT, escopos, política de rotação, log de auditoria e a garantia INV-TENANT-ISOLATION.
---

<!-- i18n:MT (pt-BR) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/security.md`

# Modelo de segurança

## Ciclo de vida do PAT

Personal Access Tokens (PATs) são o único tipo de credencial que o CoreLink aceita. Entender seu ciclo de vida é fundamental para operar com segurança.

### Criação

PATs são criados em dois lugares:

1. **Assistente de cadastro** — emite um PAT inicial com os escopos `cas:read cas:write ac:read ac:write`.
2. **Painel de administração** ou `POST /v1/pats` — emite um PAT com qualquer subconjunto de escopos.

No momento da criação, o token em texto simples é exibido **exatamente uma vez**. O CoreLink armazena apenas um hash SHA-256 com salt do valor do token. Não há endpoint de recuperação.

**Armazene seu PAT em um gerenciador de segredos antes de fechar a caixa de diálogo de criação.** Opções adequadas:

- 1Password / Bitwarden (pessoal)
- AWS Secrets Manager / HashiCorp Vault (equipe/CI)
- GitHub Actions secrets (workflows de CI)
- Doppler / Infisical (engenharia de plataforma)

Não armazene PATs em:

- Repositórios Git (mesmo os privados)
- Arquivos `.env` versionados no controle de código-fonte
- Logs de jobs de CI (`echo $CORELINK_PAT` é aceitável para depuração local; não em CI)
- Mensagens de Slack / Discord

### Somente HTTPS

O CoreLink não serve HTTP. Todo o tráfego da API usa TLS 1.2 ou TLS 1.3. O HTTPS é imposto na borda da Cloudflare; não há como desativar. Conexões na porta 80 são redirecionadas para a porta 443.

### Rotação

PATs não têm rotação automática. Você deve rotacioná-los manualmente. Cadência de rotação recomendada:

| Tipo de PAT | Cadência |
|---|---|
| Inicial (pessoal) | 90 dias ou em mudança de pessoal |
| CI/CD | 90 dias ou em mudança de equipe |
| Integração (compartilhado) | 30 dias |

Para rotacionar um PAT:

1. Crie o novo PAT (`POST /v1/pats`).
2. Atualize o segredo no seu gerenciador de segredos.
3. Implante/reinicie tudo o que consome o PAT antigo.
4. Revogue o PAT antigo (`DELETE /v1/pats/:pat_id`).

Não exclua o PAT antigo antes que o novo esteja em uso — não há período de tolerância.

### Revogação

```bash
# List PATs to find the ID
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats

# Revoke
curl -s -X DELETE \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/pats/<pat_id>"
```

A revogação é imediata. Requisições em andamento usando o PAT revogado falharão com `401` em milissegundos (limitado pela janela de propagação da borda da Cloudflare, tipicamente < 100 ms).

## Escopos

Os PATs são escopados no momento da criação. Os escopos disponíveis são:

| Escopo | Concede |
|---|---|
| `cas:read` | Ler (baixar) blobs do CAS |
| `cas:write` | Escrever (enviar) blobs no CAS |
| `ac:read` | Ler entradas do action cache |
| `ac:write` | Escrever entradas do action cache |
| `admin` | Gerenciamento de PAT, gerenciamento de usuários, exportação do log de auditoria |

Princípio do menor privilégio: dê a cada PAT o conjunto mínimo de escopos necessário. Um job de CI que apenas popula o cache precisa de `cas:write ac:write`; um proxy de cache somente leitura precisa de `cas:read ac:read`.

## Isolamento de tenant (INV-TENANT-ISOLATION)

A invariante de segurança central do CoreLink:

> **INV-TENANT-ISOLATION**: Nenhuma requisição pode ler ou escrever dados pertencentes a um tenant diferente daquele codificado no PAT, independentemente do caminho da URL, dos cabeçalhos ou do corpo da requisição.

Isso é imposto em duas camadas:

1. **Validação do PAT**: O worker resolve o PAT para um `tenant_id`. Se o tenant no caminho da URL não corresponder, a requisição é rejeitada com `403` antes de qualquer operação de armazenamento.
2. **Namespace de chave de armazenamento**: As chaves de objeto do R2 são prefixadas com `<tenant_id>/cas/<hash>`. Um bug de armazenamento que acidentalmente omita a verificação de tenant não pode produzir uma colisão porque a chave ainda contém o prefixo do tenant.

O CoreLink não oferece compartilhamento entre tenants. Se duas equipes precisarem compartilhar artefatos, elas devem usar um tenant compartilhado ou enviar o artefato para ambos os tenants de forma independente.

## O que auditamos

Toda leitura de CAS, escrita de CAS e operação de AC bem-sucedida é anexada a um log de auditoria imutável e escopado por tenant. Cada entrada registra:

| Campo | Exemplo |
|---|---|
| `event_type` | `cas.write`, `cas.read`, `ac.write`, `ac.read` |
| `tenant_id` | `acme-prod` |
| `content_hash` | `sha256:e3b0c4...` |
| `pat_prefix` | `clk_live` |
| `ip_address` | `1.2.3.4` (hashed in GDPR-constrained regions) |
| `timestamp` | `2026-05-28T08:42:00.123Z` |
| `bytes` | `4096` |

O log de auditoria é somente-anexação. Entradas individuais não podem ser excluídas. Você pode exportar o log de auditoria do seu tenant como JSON ou CSV pelo painel de administração.

## Criptografia em repouso

Todos os blobs armazenados no R2 são criptografados em repouso usando AES-256 (chaves gerenciadas pela Cloudflare por padrão). Tenants do plano Enterprise podem fornecer sua própria chave AES-256 (BYOK). Contate vendas para habilitar o BYOK.

## Divulgação responsável

Se você encontrar uma vulnerabilidade de segurança no CoreLink, envie um e-mail para [security@humangr.com](mailto:security@humangr.com). Confirmamos o recebimento dos relatos em até 24 horas e visamos correções em até 72 horas para problemas críticos.

Não abra issues públicas no GitHub para vulnerabilidades de segurança.
