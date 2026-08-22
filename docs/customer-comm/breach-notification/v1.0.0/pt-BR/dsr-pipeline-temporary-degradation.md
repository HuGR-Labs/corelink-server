---
id: "CUSTOMER-COMM-BREACH-DSR-PIPELINE-DEGRADATION-PT-BR"
type: "customer_communication_template"
template_class: "breach_notification"
incident_class: "dsr_pipeline_temporary_degradation"
locale: "pt-BR"
jurisdiction_anchor: "BR"
legal_basis: "LGPD Art. 48 (incidente que possa acarretar risco/dano relevante) + Art. 18 §5 (atendimento aos direitos do titular em tempo razoável); ANPD Res. CD/ANPD nº 15/2024"
version: "1.0.0"
created: "2026-05-15"
owner: "Privacy Officer (Gustavo Schneiter interim)"
parent_runbook: "RB-BREACH-NOTIFICATION"
parent_wi: "WI-S11-006"
wave: "wave-18"
audit_event_triggers:
  - "corelink.privacy.statuspage_publish_failed.v1"
  - "corelink.privacy.dsr_sla_breach.v1"
delivery_channel: "Cloudflare Email transactional + status page banner"
legal_review_status: "PENDING"
native_speaker_reviewed: true
template_variables:
  - "{{breach_id}}"
  - "{{breach_detected_at}}"
  - "{{customer_name}}"
  - "{{tenant_id_hex8}}"
  - "{{outage_window_from}}"
  - "{{outage_window_to}}"
  - "{{outage_duration_hours}}"
  - "{{affected_dsr_kinds}}"
  - "{{affected_request_count}}"
  - "{{containment_actions}}"
  - "{{estimated_resolution_eta}}"
  - "{{incident_status_url}}"
  - "{{customer_action_required}}"
  - "{{contact_email}}"
  - "{{breach_severity}}"
---

# Comunicado ao Cliente — Degradação Temporária do Pipeline de Direitos do Titular (DSR)

> **Esta é uma comunicação obrigatória de incidente nos termos do Art. 48 da LGPD c/c Art. 18 §5 (atendimento ao direito do titular em prazo razoável), na hipótese de perda de disponibilidade do pipeline de DSR superior a 24 horas — conforme GDPR Art. 33(2)(c) inclui perda de disponibilidade.** Preencher pelo Privacy Officer + revisão Legal antes do despacho. Remover esta seção antes do envio.

---

**Assunto:** `[{{breach_severity}}] CoreLink — Degradação temporária do pipeline de direitos do titular afetando o seu ambiente ({{breach_id}})`

**De:** privacy@hugr.dev
**Para:** {{customer_name}}
**Data do despacho:** *(preencher com data/hora UTC de envio)*

---

Prezado(a) {{customer_name}},

Em cumprimento aos deveres de comunicação previstos no **Art. 48 da Lei Geral de Proteção de Dados (LGPD)** — combinado com o **Art. 18 §5** (atendimento ao direito do titular em prazo razoável) e com a **Resolução CD/ANPD nº 15/2024** — comunicamos formalmente uma indisponibilidade prolongada do pipeline de **Direitos do Titular (DSR — Data Subject Requests)** detectada em sua tenant CoreLink (`{{tenant_id_hex8}}`).

> **Importante:** o GDPR Art. 33(2)(c) e a interpretação canônica da ANPD (Res. 15/2024) reconhecem perda de disponibilidade dos dados pessoais e da capacidade de exercício de direitos como hipótese de incidente comunicável quando configurar risco ou dano relevante. Indisponibilidade do canal DSR superior a 24h, por afetar diretamente o exercício de direitos previstos no Art. 18 da LGPD, está nesse escopo.

## 1. Resumo do Incidente

| Campo | Valor |
|---|---|
| **Identificador do incidente** | `{{breach_id}}` |
| **Gravidade** | `{{breach_severity}}` |
| **Detectado em** | `{{breach_detected_at}}` (UTC) |
| **Natureza** | Indisponibilidade prolongada do pipeline DSR (perda de disponibilidade — LGPD Art. 18 §5; GDPR Art. 33(2)(c)) |
| **Janela de indisponibilidade** | `{{outage_window_from}}` → `{{outage_window_to}}` (UTC) |
| **Duração** | `{{outage_duration_hours}}` horas (acima do limiar de 24h) |
| **Tipos de DSR afetados** | `{{affected_dsr_kinds}}` (subconjunto dos 7 direitos do Art. 18) |
| **Pedidos pendentes** | `{{affected_request_count}}` |
| **Tenant impactado** | `{{tenant_id_hex8}}` (sua tenant — comunicação direta) |

## 2. O que aconteceu

O pipeline de DSR — que executa, em fluxo determinístico, os 7 tipos de solicitações de direitos do titular (acesso, correção, eliminação, portabilidade, oposição, informação sobre compartilhamento, revogação de consentimento) — ficou indisponível por **{{outage_duration_hours}} horas** consecutivas para a sua tenant. **Não há indicação de exfiltração nem comprometimento da confidencialidade dos dados pessoais armazenados.** O que falhou foi a capacidade de **atender solicitações ativas**, o que afeta o direito do(a) titular ao exercício dos direitos do Art. 18 da LGPD.

A causa-raiz está em apuração ativa nos seguintes ramos:

- **Falha do publicador de Statuspage** que orquestra o estado dos jobs DSR no plano de controle.
- **Pressão de fila no worker de eliminação** (cross-backend; 12 backends canônicos).
- **Degradação de dependência externa** (Cloudflare D1 / R2 em região afetada).

## 3. Ação que pedimos ao(à) cliente

`{{customer_action_required}}`

Cenários típicos:

- **Re-submeter solicitações DSR pendentes** após a comunicação de "tudo normalizado" — não há ação imediata.
- **Notificar titulares de dados** que tenham pedido DSR diretamente a você caso o pedido tenha sido encaminhado durante a janela afetada.
- **Confirmar o recebimento desta comunicação em até 24 horas** pelo e-mail `privacy@hugr.dev`.

> **Garantia:** todo pedido DSR submetido durante a janela é **preservado em fila durável** (Durable Object + KV) — nenhum pedido foi perdido. A indisponibilidade afetou o atendimento (latência) e não a recepção.

## 4. O que já fizemos

1. **Statuspage atualizado** com banner SEV-1 (`{{incident_status_url}}`).
2. **Fila de DSRs preservada** em armazenamento durável — zero perda de pedidos.
3. **Workers DSR escalados** horizontalmente conforme o plano de capacidade.
4. **Comunicação à ANPD** sendo avaliada conforme a triagem SEV consolidada (perda de disponibilidade pode acionar Art. 48 §1º — análise de risco em curso).
5. **Job de drenagem retroativa** preparado: ao retomar o serviço, todos os pedidos enfileirados serão atendidos em ordem cronológica com renotificação automática ao(à) titular.

## 5. Status da investigação e ETA

- **Estado atual:** mitigação parcial em produção; pipeline volta progressivamente.
- **ETA de resolução plena:** `{{estimated_resolution_eta}}` (UTC).
- **Página de status do incidente:** `{{incident_status_url}}`.
- **Post-mortem público:** dentro de 5 dias úteis após o encerramento, em `https://corelink-docs.humangr.com/trust/incident-history` (referência `{{breach_id}}`).

## 6. Direitos do(a) titular e canais regulatórios

Reforçamos que os direitos previstos no **Art. 18 da LGPD** (confirmação, acesso, correção, anonimização/bloqueio/eliminação, portabilidade, eliminação de dados de consentimento, informação sobre compartilhamento, oposição, revogação) **permanecem plenamente exigíveis**, ainda que o canal técnico tenha apresentado degradação transitória. O prazo legal de 15 dias úteis para resposta segue contando a partir do recebimento efetivo do pedido — a indisponibilidade do pipeline NÃO suspende o prazo legal.

Caso entenda que seus direitos foram violados:

- Apresente petição diretamente à HuGR pelo canal `privacy@hugr.dev`.
- Apresente reclamação à **Autoridade Nacional de Proteção de Dados (ANPD)** em `https://www.gov.br/anpd/pt-br/canais_atendimento/cidadao`.

## 7. Contato

- **DPO (Encarregado interino):** Gustavo Schneiter
- **E-mail DPO:** `gustavo@humangr.com`
- **E-mail Privacidade (canal canônico):** `{{contact_email}}` *(padrão: `privacy@hugr.dev`)*
- **Canal dedicado deste incidente:** `{{incident_status_url}}`

Permaneceremos disponíveis 24/7 para qualquer dúvida durante a janela do incidente.

Atenciosamente,
**Gustavo Schneiter**
DPO interino — HuGR Labs Ltda.
`gustavo@humangr.com`

---

**Referências cruzadas:**
- Aviso de Privacidade HuGR CoreLink v1.0.0 (`legal/privacy-notice/v1.0.0/pt-BR.md`)
- Runbook canônico RB-BREACH-NOTIFICATION (`specs/_runbooks/RB-BREACH-NOTIFICATION.md`)
- LGPD Art. 48 + Art. 18 §5 + Art. 46; ANPD Res. CD/ANPD nº 15/2024
- WI-S11-001 (DSR API 7 endpoints) + WI-S11-002 (Erasure worker)
