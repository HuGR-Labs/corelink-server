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

1. **Asistente de registro** — emite un PAT inicial con los scopes `cas:read cas:write ac:read ac:write`.
2. **Panel de administración** o `POST /v1/pats` — emite un PAT con cualquier subconjunto de scopes.

En el momento de la creación, el token en texto plano se muestra **exactamente una vez**. CoreLink almacena solo un hash SHA-256 con sal del valor del token. No existe un endpoint de recuperación.

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

Los PAT no tienen rotación automática. Debe rotarlos manualmente. Cadencia de rotación recomendada:

| Tipo de PAT | Cadencia |
|---|---|
| Inicial (personal) | 90 días o ante un cambio de personal |
| CI/CD | 90 días o ante un cambio de equipo |
| Integración (compartido) | 30 días |

Para rotar un PAT:

1. Cree el nuevo PAT (`POST /v1/pats`).
2. Actualice el secreto en su gestor de secretos.
3. Despliegue/reinicie todo lo que consume el PAT antiguo.
4. Revoque el PAT antiguo (`DELETE /v1/pats/:pat_id`).

No elimine el PAT antiguo antes de que el nuevo esté en uso — no hay período de gracia.

### Revocación

```bash
# List PATs to find the ID
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats

# Revoke
curl -s -X DELETE \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/pats/<pat_id>"
```

La revocación es inmediata. Las solicitudes en curso que usan el PAT revocado fallarán con `401` en milisegundos (limitado por la ventana de propagación del borde de Cloudflare, típicamente < 100 ms).

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
| `pat_prefix` | `clk_live` |
| `ip_address` | `1.2.3.4` (hashed in GDPR-constrained regions) |
| `timestamp` | `2026-05-28T08:42:00.123Z` |
| `bytes` | `4096` |

El log de auditoría es de solo anexado. Las entradas individuales no se pueden eliminar. Puede exportar el log de auditoría de su tenant como JSON o CSV desde el panel de administración.

## Cifrado en reposo

Todos los blobs almacenados en R2 se cifran en reposo usando AES-256 (claves gestionadas por Cloudflare de forma predeterminada). Los tenants del plan Enterprise pueden suministrar su propia clave AES-256 (BYOK). Contacte a ventas para habilitar BYOK.

## Divulgación responsable

Si encuentra una vulnerabilidad de seguridad en CoreLink, envíe un correo electrónico a [security@humangr.com](mailto:security@humangr.com). Acusamos recibo de los informes dentro de las 24 horas y apuntamos a parches dentro de las 72 horas para problemas críticos.

No abra issues públicas en GitHub por vulnerabilidades de seguridad.
