---
id: "CUSTOMER-COMM-BREACH-DSR-PIPELINE-DEGRADATION-ES"
type: "customer_communication_template"
template_class: "breach_notification"
incident_class: "dsr_pipeline_temporary_degradation"
locale: "es"
jurisdiction_anchor: "MX"
legal_basis: "LFPDPPP Art. 20-21 (Mexico — comunicación de vulneraciones de seguridad incluyendo pérdida o limitación temporal del acceso a derechos ARCO); cláusula primaria Art. 21 (análisis de causas + medidas correctivas); concordancia: GDPR Art. 34 / LGPD Art. 48"
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
legal_review_status: "PENDING_MX_ATTORNEY"
native_speaker_reviewed: true
legal_review_caveat: "LFPDPPP Art. 21 anchor confirmed by user task spec wave-18; distinct from LFPDPPP Art. 10 VI emergency-exception (deferred legal review per wave-17-ter audit D-8). Outside MX attorney still to sign off pre-GA per WI-S11-004 §6.1.4."
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

# Comunicado al Cliente — Degradación Temporal del Pipeline de Derechos del Titular (DSR)

> **Esta es una comunicación obligatoria de vulneración de seguridad conforme al Art. 20-21 LFPDPPP, en la hipótesis de pérdida temporal de disponibilidad del canal de ejercicio de los derechos ARCO superior a 24 horas — figura reconocida también por el GDPR Art. 33(2)(c) (loss of availability).** Completar por el Privacy Officer + revisión legal antes del envío. Eliminar este aviso antes de enviar.

---

**Asunto:** `[{{breach_severity}}] CoreLink — Degradación temporal del pipeline de derechos ARCO que afecta su tenant ({{breach_id}})`

**De:** privacy@hugr.dev
**Para:** {{customer_name}}
**Despachado:** *(completar con fecha/hora UTC del envío)*

---

Estimado(a) {{customer_name}}:

En cumplimiento del deber de comunicación previsto en el **Art. 20 de la LFPDPPP** (informar al titular cuando una vulneración de seguridad afecte de forma significativa sus derechos) combinado con el **Art. 21** (analizar causas e implementar acciones correctivas, preventivas y de mejora), le comunicamos formalmente una indisponibilidad prolongada de nuestro pipeline de **Derechos del Titular (DSR — Data Subject Requests)**, detectada en su tenant CoreLink (`{{tenant_id_hex8}}`).

> **Importante:** la indisponibilidad prolongada del canal que permite ejercer los derechos **ARCO** (Acceso, Rectificación, Cancelación, Oposición — Arts. 22-26 LFPDPPP) constituye, en concordancia internacional con el GDPR Art. 33(2)(c) y la LGPD Art. 18 §5, una vulneración comunicable cuando la duración supera los umbrales operativos razonables. Nuestra política interna establece el umbral en 24 horas.

## 1. Resumen del Incidente

| Campo | Valor |
|---|---|
| **Identificador del incidente** | `{{breach_id}}` |
| **Severidad** | `{{breach_severity}}` |
| **Detectado el** | `{{breach_detected_at}}` (UTC) |
| **Naturaleza** | Indisponibilidad prolongada del pipeline DSR (pérdida de disponibilidad — LFPDPPP Art. 22-26; GDPR Art. 33(2)(c)) |
| **Ventana de indisponibilidad** | `{{outage_window_from}}` → `{{outage_window_to}}` (UTC) |
| **Duración** | `{{outage_duration_hours}}` horas (por encima del umbral de 24h) |
| **Tipos de DSR afectados** | `{{affected_dsr_kinds}}` (subconjunto de los 7 derechos canónicos) |
| **Solicitudes pendientes** | `{{affected_request_count}}` |
| **Tenant impactado** | `{{tenant_id_hex8}}` (su tenant — comunicación directa) |

## 2. Qué ocurrió

El pipeline DSR — que ejecuta de forma determinística los 7 tipos canónicos de solicitudes del titular (acceso, rectificación, cancelación, oposición, portabilidad, información sobre tratamiento, revocación de consentimiento) — quedó indisponible durante **{{outage_duration_hours}} horas** consecutivas para su tenant. **No existe indicio alguno de exfiltración ni de comprometimiento de la confidencialidad de los datos personales almacenados.** Lo que falló fue la capacidad de **atender solicitudes activas**, lo que afecta el ejercicio de los derechos ARCO del titular.

El análisis de causa-raíz está en curso sobre las siguientes ramas:

- **Falla del publicador de Statuspage** que orquesta el estado de los jobs DSR en el plano de control.
- **Saturación de la cola del worker de cancelación** (cross-backend; 12 backends canónicos).
- **Degradación de dependencia externa** (Cloudflare D1 / R2 en la región afectada).

## 3. Acción solicitada al cliente

`{{customer_action_required}}`

Escenarios típicos:

- **Reenviar solicitudes DSR pendientes** una vez recibida nuestra comunicación de "servicio normalizado" — no se requiere acción inmediata.
- **Notificar a los titulares de datos** cuya solicitud DSR le haya sido remitida durante la ventana afectada.
- **Confirmar la recepción de este aviso en un plazo de 24 horas** respondiendo a `privacy@hugr.dev`.

> **Garantía:** toda solicitud DSR enviada durante la ventana queda **preservada en colas durables** (Durable Object + KV) — cero pérdida de solicitudes. La indisponibilidad afectó al tiempo de atención (latencia), no a la recepción.

## 4. Lo que ya hemos hecho

1. **Statuspage actualizado** con banner SEV-1 (`{{incident_status_url}}`).
2. **Cola de DSRs preservada** en almacenamiento durable — cero pérdida de solicitudes.
3. **Workers DSR escalados** horizontalmente conforme al plan de capacidad.
4. **Cumplimiento del Art. 21 LFPDPPP** — análisis de causas y diseño de acciones correctivas y preventivas en curso; informe consolidado en el post-mortem (§5).
5. **Job de drenaje retroactivo** preparado: al restaurar el servicio, todas las solicitudes en cola se procesarán en orden cronológico con renotificación automática al titular.

## 5. Estado de la investigación y ETA de resolución

- **Estado actual:** mitigación parcial en producción; el pipeline vuelve progresivamente a la operación nominal.
- **ETA de resolución plena:** `{{estimated_resolution_eta}}` (UTC).
- **Página de estado del incidente:** `{{incident_status_url}}`.
- **Post-mortem público:** dentro de 5 días hábiles posteriores al cierre, en `https://corelink-docs.humangr.com/trust/incident-history` (referencia `{{breach_id}}`).

## 6. Derechos del titular y canales regulatorios

Reafirmamos que los derechos **ARCO** previstos en los **Arts. 22-26 de la LFPDPPP** **permanecen plenamente exigibles**, no obstante la degradación transitoria del canal técnico. El plazo legal de respuesta de 20 días previsto en el Art. 32 LFPDPPP sigue computándose desde la recepción efectiva de la solicitud — la indisponibilidad del pipeline NO suspende el plazo legal.

Si usted considera que sus derechos de protección de datos han sido vulnerados, puede:

- Presentar una solicitud directamente a HuGR a través de `privacy@hugr.dev`.
- Presentar una denuncia ante el **Instituto Nacional de Transparencia, Acceso a la Información y Protección de Datos Personales (INAI)**: `https://home.inai.org.mx/?page_id=1782`.
- Iniciar el procedimiento de protección de derechos (PPD) ante el INAI conforme al Capítulo VII de la LFPDPPP.

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
- LFPDPPP Art. 20-21 + Arts. 22-26 (ARCO) + Art. 32 (México) · GDPR Art. 34 · LGPD Art. 48
- WI-S11-001 (DSR API 7 endpoints) + WI-S11-002 (Erasure worker)
