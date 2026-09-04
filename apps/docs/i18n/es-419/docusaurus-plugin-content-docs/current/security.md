---
id: security
title: Modelo de seguridad
sidebar_position: 11
description: Ciclo de vida del PAT, scopes, política de rotación, log de auditoría y la garantía INV-TENANT-ISOLATION.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/security.md`

# Modelo de seguridad

## Ciclo de vida del PAT

Los Personal Access Tokens (PAT) son el único tipo de credencial que CoreLink acepta. Entender su ciclo de vida es fundamental para operar de forma segura.

### Creación

Los PAT se crean en dos lugares:

1. **Asistente de registro** — emite automáticamente un PAT inicial con los scopes `cas:read cas:write ac:read ac:write`. Esta es la única vía de creación de PAT que está activa hoy.
2. **Emisión self-service de PAT** (`POST /v1/pats`) está activa para una sesión Clerk validada o un PAT canónico; el tenant se deriva en el servidor y el token plaintext se devuelve una sola vez.

En el momento de la creación, el token en texto plano se muestra **exactamente una vez**. CoreLink nunca almacena el texto plano. No existe un endpoint de recuperación.

**Guarde su PAT en un gestor de secretos antes de cerrar el cuadro de diálogo de creación.** Opciones adecuadas:

- 1Password / Bitwarden (personal)
- AWS Secrets Manager / HashiCorp Vault (equipo/CI)
- GitHub Actions secrets (workflows de CI)
- Doppler / Infisical (ingeniería de plataforma)

No guarde los PAT en:

- Repositorios Git (incluso los privados)
- Archivos `.env` versionados en el control de código fuente
- Logs de jobs de CI (`echo $CORELINK_PAT` está bien para depuración local; no en CI)
- Mensajes de Slack / Discord

### Solo HTTPS

CoreLink no sirve HTTP. Todo el tráfico de la API usa TLS 1.2 o TLS 1.3. HTTPS se aplica en el borde de Cloudflare; no hay forma de desactivarlo. Las conexiones en el puerto 80 se redirigen al puerto 443.

### Rotación

Los PAT no tienen rotación automática. Cadencia de rotación recomendada para la emisión self-service:

| Tipo de PAT | Cadencia |
|---|---|
| Inicial (personal) | 90 días o ante un cambio de personal |
| CI/CD | 90 días o ante un cambio de equipo |
| Integración (compartido) | 30 días |

**La emisión self-service (`POST /v1/pats`) y el alias del panel (`POST /v1/customer/keys`) están activos. Ambos comparten la política `pat-issue` por tenant: burst 10 y refill de 10/hora (un token cada 360 segundos), aplicada antes del mint y la auditoría. Las respuestas `429` incluyen `Retry-After`.

### Revocación

Hoy la revocación se gestiona a través de soporte — escriba a [support@humangr.com](mailto:support@humangr.com) con la etiqueta del PAT o el ID del tenant. Una vez revocado, las solicitudes en curso con ese PAT fallarán con `401` dentro de la ventana de propagación del borde de Cloudflare (típicamente < 100 ms).

## Scopes

Los PAT se limitan por scope en el momento de la creación. Los scopes disponibles son:

| Scope | Otorga |
|---|---|
| `cas:read` | Leer (descargar) blobs del CAS |
| `cas:write` | Escribir (subir) blobs al CAS |
| `ac:read` | Leer entradas del action cache |
| `ac:write` | Escribir entradas del action cache |
| `admin` | Gestión de PAT, gestión de usuarios, exportación del log de auditoría |

Principio de mínimo privilegio: otorgue a cada PAT el conjunto mínimo de scopes requerido. Un job de CI que solo puebla el caché necesita `cas:write ac:write`; un proxy de caché de solo lectura necesita `cas:read ac:read`.

## Aislamiento de tenant (INV-TENANT-ISOLATION)

La invariante de seguridad central de CoreLink:

> **INV-TENANT-ISOLATION**: Ninguna solicitud puede leer ni escribir datos pertenecientes a un tenant distinto del codificado en el PAT, sin importar la ruta de la URL, los encabezados o el cuerpo de la solicitud.

Esto se aplica en dos capas:

1. **Validación del PAT**: El worker resuelve el PAT a un `tenant_id`. Si el tenant de la ruta de la URL no coincide, la solicitud se rechaza con `403` antes de cualquier operación de almacenamiento.
2. **Namespace de clave de almacenamiento**: Las claves de objeto de R2 llevan el prefijo `<tenant_id>/cas/<hash>`. Un bug de almacenamiento que omita accidentalmente la verificación de tenant no puede producir una colisión porque la clave sigue conteniendo el prefijo del tenant.

CoreLink no ofrece compartición entre tenants. Si dos equipos necesitan compartir artefactos, deben usar un tenant compartido o subir el artefacto a ambos tenants de forma independiente.

## Qué auditamos

Cada lectura de CAS, escritura de CAS y operación de AC exitosa se anexa a un log de auditoría inmutable y limitado por tenant. Cada entrada registra:

| Campo | Ejemplo |
|---|---|
| `event_type` | `cas.write`, `cas.read`, `ac.write`, `ac.read` |
| `tenant_id` | `acme-prod` |
| `content_hash` | `sha256:e3b0c4...` |
| `pat_prefix` | `aZ3xQ1` |
| `ip_address` | `1.2.3.4` (hashed in GDPR-constrained regions) |
| `timestamp` | `2026-05-28T08:42:00.123Z` |
| `bytes` | `4096` |

El log de auditoría es de solo anexado. Las entradas individuales no se pueden eliminar. Puede exportar el log de auditoría de su tenant como JSON o CSV desde el panel de administración.

## Cifrado en reposo

Todos los blobs almacenados en R2 se cifran en reposo usando AES-256 (claves gestionadas por Cloudflare de forma predeterminada). Los tenants del plan Enterprise pueden suministrar su propia clave AES-256 (BYOK). Contacte a ventas para habilitar BYOK.

## Divulgación responsable

Si encuentra una vulnerabilidad de seguridad en CoreLink, envíe un correo electrónico a [security@humangr.com](mailto:security@humangr.com). Acusamos recibo de los informes dentro de las 24 horas y apuntamos a parches dentro de las 72 horas para problemas críticos.

No abra issues públicas en GitHub por vulnerabilidades de seguridad.
