---
id: "CUSTOMER-COMM-BREACH-AUDIT-CHAIN-INTEGRITY-PT-BR"
type: "customer_communication_template"
template_class: "breach_notification"
incident_class: "audit_chain_integrity"
locale: "pt-BR"
jurisdiction_anchor: "BR"
legal_basis: "LGPD Art. 48 (comunicação ao titular) + Art. 46 (medidas de segurança); ANPD Res. CD/ANPD nº 15/2024"
version: "1.0.0"
created: "2026-05-15"
owner: "Privacy Officer (Gustavo Schneiter interim)"
parent_runbook: "RB-BREACH-NOTIFICATION"
parent_wi: "WI-S11-006"
wave: "wave-18"
audit_event_triggers:
  - "corelink.audit.export_verify_failed.v1"
  - "corelink.audit.export_integrity_failed.v1"
delivery_channel: "Cloudflare Email transactional + status page banner (SEV-1)"
legal_review_status: "PENDING"
native_speaker_reviewed: true
template_variables:
  - "{{breach_id}}"
  - "{{breach_detected_at}}"
  - "{{customer_name}}"
  - "{{tenant_id_hex8}}"
  - "{{affected_window_from}}"
  - "{{affected_window_to}}"
  - "{{at_sequence}}"
  - "{{classification}}"
  - "{{containment_actions}}"
  - "{{estimated_resolution_eta}}"
  - "{{incident_status_url}}"
  - "{{customer_action_required}}"
  - "{{contact_email}}"
  - "{{breach_severity}}"
---

# Comunicado ao Cliente — Incidente de Integridade da Cadeia de Auditoria

> **Esta é uma comunicação obrigatória de incidente de segurança nos termos da Lei nº 13.709/2018 (LGPD) Art. 48 e da Resolução CD/ANPD nº 15/2024.** O template deve ser preenchido pelo Privacy Officer + revisão Legal antes do despacho ao cliente. Remover esta seção antes do envio.

---

**Assunto:** `[{{breach_severity}}] CoreLink — Incidente de integridade da cadeia de auditoria afetando o seu ambiente ({{breach_id}})`

**De:** privacy@hugr.dev
**Para:** {{customer_name}}
**Data do despacho:** *(preencher com data/hora UTC de envio)*

---

Prezado(a) {{customer_name}},

Em cumprimento aos deveres de comunicação previstos no **Art. 48 da Lei Geral de Proteção de Dados (LGPD)** e à **Resolução CD/ANPD nº 15/2024**, comunicamos formalmente um incidente de segurança detectado em sua tenant CoreLink (`{{tenant_id_hex8}}`).

## 1. Resumo do Incidente

| Campo | Valor |
|---|---|
| **Identificador do incidente** | `{{breach_id}}` |
| **Gravidade** | `{{breach_severity}}` |
| **Detectado em** | `{{breach_detected_at}}` (UTC) |
| **Natureza** | Falha de verificação criptográfica em exportação de log de auditoria (chain-break ou tampering) |
| **Classificação técnica** | `{{classification}}` |
| **Janela afetada** | `{{affected_window_from}}` → `{{affected_window_to}}` (UTC) |
| **Sequência divergente** | `at_sequence = {{at_sequence}}` |
| **Tenant impactado** | `{{tenant_id_hex8}}` (sua tenant — comunicação direta) |

O nosso verificador server-side detectou que a árvore de Merkle de um lote de eventos de auditoria entregue ao seu endpoint `GET /v1/audit/export` apresentou divergência em relação ao checkpoint diário canônico. **Os bytes foram entregues ao seu cliente; não conseguimos atestar a integridade independente desses bytes no momento da entrega.**

## 2. O que aconteceu

Trata-se de uma falha de integridade da cadeia de auditoria — não há indicação, neste momento, de exfiltração de dados pessoais. O risco protegido por esta comunicação é o de **registro probatório regulatório** (você pode ter precisado desses logs para auditoria SOC 2, fiscalização ANPD ou contencioso). A causa-raiz está sob apuração ativa nos seguintes ramos:

- **Chain break real em R2** (armazenamento canônico): pouco provável em razão do Object Lock; investigação em curso.
- **Tampering de linha arquivada** (probabilidade quase nula pelo Object Lock).
- **Bug do pipeline de exportação** (probabilidade média; replay já em execução).

## 3. Ação que pedimos ao(à) cliente

`{{customer_action_required}}`

Cenários típicos:

- **Nenhuma ação imediata é necessária** se você não compartilhou o export afetado com auditor externo nem o utilizou como evidência.
- **Re-exportar o log de auditoria** após a resolução (veremos um aviso explícito quando o endpoint for reaberto — atualmente devolvendo `503` para sua tenant por força do nosso runbook RB-AUDIT-EXPORT-VERIFY-FAILED §3).
- **Notificar auditor downstream** se você já compartilhou o export afetado, esclarecendo que o lote está sob hold de integridade.
- **Confirmar o recebimento desta comunicação em até 24 horas** pelo e-mail `privacy@hugr.dev`.

## 4. O que já fizemos

1. **Endpoint de exportação de auditoria suspenso** para a sua tenant (HTTP `503`) até a investigação concluir.
2. **Pausa das gravações da cadeia de auditoria** para a sua tenant — eventos novos seguem sendo coletados em buffer de retenção e serão reidratados após o rebuild.
3. **Bundle forense capturado** dos chunks R2 afetados (BLAKE3-addressed, write-once).
4. **Replay criptográfico em execução** contra o checkpoint canônico do daily-verifier.
5. **Comunicação à ANPD** sendo preparada em paralelo (LGPD Art. 48 §1º — análise de risco em andamento; comunicação efetiva conforme a classificação SEV consolidada na triagem).

## 5. Status da investigação e ETA

- **Estado atual:** investigação ativa; classificação técnica `{{classification}}`.
- **ETA de resolução:** `{{estimated_resolution_eta}}` (UTC). MTTR contratual ≤ 4h end-to-end; pode haver extensão se o rebuild precisar varrer janela ampla do R2.
- **Página de status do incidente:** `{{incident_status_url}}`.
- **Post-mortem público:** dentro de 5 dias úteis após o encerramento, publicado em `https://corelink-docs.humangr.com/trust/incident-history` (referência `{{breach_id}}`).

## 6. Direitos do(a) titular e canais regulatórios

Esta comunicação é parte do nosso dever previsto no **Art. 48 da LGPD** (comunicação ao titular e à ANPD na ocorrência de incidente de segurança que possa acarretar risco ou dano relevante). O conteúdo segue o roteiro do Art. 48 §1º incisos I–VI (natureza dos dados, titulares envolvidos, medidas técnicas, riscos, motivos do atraso quando aplicável, medidas de mitigação).

Caso entenda que seus direitos foram violados, você pode:

- Apresentar petição diretamente à HuGR pelo canal `privacy@hugr.dev`.
- Apresentar reclamação à **Autoridade Nacional de Proteção de Dados (ANPD)** pelo portal oficial `https://www.gov.br/anpd/pt-br/canais_atendimento/cidadao`.
- Acessar seus direitos previstos no Art. 18 da LGPD (acesso, correção, eliminação, portabilidade, revogação de consentimento) pelo nosso endpoint self-service `POST /v1/privacy/dsr/{kind}` (autenticação obrigatória).

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
- Runbook técnico RB-AUDIT-EXPORT-VERIFY-FAILED (`specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md`)
- LGPD Art. 48 + Art. 46 + ANPD Res. CD/ANPD nº 15/2024
- INV-AUDIT-APPEND-ONLY (§3.6 L116; R2 Object Lock 7 anos)
