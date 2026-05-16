---
id: "CUSTOMER-COMM-BREACH-AUDIT-CHAIN-INTEGRITY-ES"
type: "customer_communication_template"
template_class: "breach_notification"
incident_class: "audit_chain_integrity"
locale: "es"
jurisdiction_anchor: "MX"
legal_basis: "LFPDPPP Art. 20-21 (Mexico — comunicación de vulneraciones de seguridad); cláusula primaria Art. 21 (análisis de causas + medidas correctivas tras vulneración); concordancia: GDPR Art. 34 / LGPD Art. 48"
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
legal_review_status: "PENDING_MX_ATTORNEY"
native_speaker_reviewed: true
legal_review_caveat: "LFPDPPP Art. 21 anchor confirmed by user task spec wave-18; distinct from LFPDPPP Art. 10 VI emergency-exception (deferred legal review per wave-17-ter audit D-8). Outside MX attorney still to sign off pre-GA per WI-S11-004 §6.1.4."
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

# Comunicado al Cliente — Incidente de Integridad de la Cadena de Auditoría

> **Esta es una comunicación obligatoria de vulneración de seguridad conforme al Art. 20-21 de la Ley Federal de Protección de Datos Personales en Posesión de los Particulares (LFPDPPP).** El template debe ser completado por el Privacy Officer y revisado por el equipo legal antes del envío al cliente. Eliminar este aviso antes de enviar.

---

**Asunto:** `[{{breach_severity}}] CoreLink — Incidente de integridad de la cadena de auditoría que afecta su tenant ({{breach_id}})`

**De:** privacy@hugr.dev
**Para:** {{customer_name}}
**Despachado:** *(completar con fecha/hora UTC del envío)*

---

Estimado(a) {{customer_name}}:

En cumplimiento del deber de comunicación previsto en el **Artículo 21 de la Ley Federal de Protección de Datos Personales en Posesión de los Particulares (LFPDPPP)** —que obliga al responsable a informar de forma inmediata al titular cuando una vulneración de seguridad afecte de forma significativa sus derechos patrimoniales o morales (Art. 20 LFPDPPP), y a analizar las causas e implementar acciones correctivas, preventivas y de mejora (Art. 21 LFPDPPP)— le comunicamos formalmente un incidente de seguridad detectado en su tenant CoreLink (`{{tenant_id_hex8}}`).

## 1. Resumen del Incidente

| Campo | Valor |
|---|---|
| **Identificador del incidente** | `{{breach_id}}` |
| **Severidad** | `{{breach_severity}}` |
| **Detectado el** | `{{breach_detected_at}}` (UTC) |
| **Naturaleza** | Falla de verificación criptográfica en una exportación de log de auditoría (rotura de cadena o manipulación) |
| **Clasificación técnica** | `{{classification}}` |
| **Ventana afectada** | `{{affected_window_from}}` → `{{affected_window_to}}` (UTC) |
| **Secuencia divergente** | `at_sequence = {{at_sequence}}` |
| **Tenant impactado** | `{{tenant_id_hex8}}` (su tenant — comunicación directa) |

Nuestro verificador del lado servidor detectó una discrepancia en el árbol de Merkle de un lote de eventos de auditoría entregado a su endpoint `GET /v1/audit/export`, frente al checkpoint canónico del daily-verifier. **Los bytes fueron entregados a su cliente; no podemos garantizar de forma independiente la integridad de esos bytes al momento de la entrega.**

## 2. Qué ocurrió

Se trata de una falla de integridad de la cadena de auditoría — **en este momento no existe indicio alguno de exfiltración de datos personales**. El riesgo cubierto por esta comunicación es el del **valor probatorio regulatorio** de los registros (usted pudo haber requerido esos logs para auditorías SOC 2, requerimientos de autoridad reguladora o procedimientos contenciosos). Se analizan tres ramas de causa-raíz en paralelo:

- **Rotura real de cadena en R2** (almacenamiento canónico): poco probable por el Object Lock; bajo investigación.
- **Manipulación de un registro archivado** (probabilidad cercana a cero por el Object Lock).
- **Bug del pipeline de exportación** (probabilidad media; replay criptográfico en curso).

## 3. Acción solicitada al cliente

`{{customer_action_required}}`

Escenarios típicos:

- **No se requiere acción inmediata** si no compartió el export afectado con un auditor externo ni lo utilizó como evidencia.
- **Re-exportar el log de auditoría** una vez liberado el endpoint (le notificaremos explícitamente cuando la suspensión se levante — actualmente devolvemos HTTP `503` para su tenant conforme al runbook RB-AUDIT-EXPORT-VERIFY-FAILED §3).
- **Notificar a su auditor externo** si ya compartió el export afectado, indicándole que el lote está bajo retención de integridad.
- **Confirmar la recepción de este aviso en un plazo máximo de 24 horas** respondiendo a `privacy@hugr.dev`.

## 4. Lo que ya hemos hecho

1. **Endpoint de exportación de auditoría deshabilitado** para su tenant (HTTP `503`) hasta concluir la investigación.
2. **Pausa de las escrituras de la cadena de auditoría** para su tenant — los eventos nuevos quedan encolados en un buffer de retención y serán drenados de vuelta a la cadena tras el rebuild.
3. **Bundle forense capturado** de los chunks R2 afectados (direccionable por BLAKE3, write-once).
4. **Replay criptográfico en ejecución** contra el checkpoint canónico del daily-verifier.
5. **Cumplimiento del Art. 21 LFPDPPP** — análisis de causas y diseño de acciones correctivas y preventivas en curso; informe consolidado en el post-mortem (§5).

## 5. Estado de la investigación y ETA de resolución

- **Estado actual:** investigación activa; clasificación técnica `{{classification}}`.
- **ETA de resolución:** `{{estimated_resolution_eta}}` (UTC). MTTR contractual ≤ 4h de extremo a extremo; podría extenderse si el rebuild abarca una ventana amplia de R2.
- **Página de estado del incidente:** `{{incident_status_url}}`.
- **Post-mortem público:** dentro de 5 días hábiles posteriores al cierre, publicado en `https://corelink.humangr.com/incidents/{{breach_id}}`.

## 6. Derechos del titular y canales regulatorios

Esta comunicación forma parte de nuestro deber bajo el **Art. 20-21 LFPDPPP** y, en concordancia internacional, del **GDPR Art. 34** y de la **LGPD Art. 48**. El contenido sigue la estructura mínima del Art. 21 LFPDPPP (naturaleza del incidente, datos comprometidos, recomendaciones al titular, acciones correctivas adoptadas).

Si usted considera que sus derechos de protección de datos han sido vulnerados, puede:

- Presentar una solicitud directamente a HuGR a través de `privacy@hugr.dev`.
- Presentar una denuncia ante el **Instituto Nacional de Transparencia, Acceso a la Información y Protección de Datos Personales (INAI)**: `https://home.inai.org.mx/?page_id=1782`.
- Ejercer sus derechos **ARCO** (Acceso, Rectificación, Cancelación, Oposición — Arts. 22-26 LFPDPPP) mediante nuestro endpoint self-service `POST /v1/privacy/dsr/{kind}` (autenticación requerida).

## 7. Contacto

- **DPO (Encargado interino de Protección de Datos):** Gustavo Schneiter
- **Correo del DPO:** `gustavo@humangr.com`
- **Buzón de privacidad (canal canónico):** `{{contact_email}}` *(por defecto: `privacy@hugr.dev`)*
- **Canal dedicado de este incidente:** `{{incident_status_url}}`

Estaremos disponibles 24/7 para cualquier consulta durante la ventana del incidente.

Atentamente,
**Gustavo Schneiter**
DPO interino — HuGR Labs Ltda.
`gustavo@humangr.com`

---

**Referencias cruzadas:**
- Aviso de Privacidad HuGR CoreLink v1.0.0 (`legal/privacy-notice/v1.0.0/es-MX.md`)
- Runbook canónico RB-BREACH-NOTIFICATION (`specs/_runbooks/RB-BREACH-NOTIFICATION.md`)
- Runbook técnico RB-AUDIT-EXPORT-VERIFY-FAILED (`specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md`)
- LFPDPPP Art. 20-21 (México) · GDPR Art. 34 · LGPD Art. 48 · EDPB Guidelines 9/2022
- INV-AUDIT-APPEND-ONLY (§3.6 L116; R2 Object Lock 7 años)
