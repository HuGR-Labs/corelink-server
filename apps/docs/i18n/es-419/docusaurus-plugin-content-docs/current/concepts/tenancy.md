---
id: tenancy
title: Modelo de tenant y alcance de PAT
sidebar_position: 2
description: "Cómo CoreLink aísla los tenants, cómo se define el alcance de los PAT y cómo se ve el acceso entre tenants (respuesta: imposible)."
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/concepts/tenancy.md`

# Modelo de tenant y alcance de PAT

## ¿Qué es un tenant?

Un **tenant** es el límite de aislamiento de nivel superior en CoreLink. Cada pieza
de datos — blobs del CAS, entradas del AC, eventos de auditoría, registros de
facturación — pertenece a exactamente un tenant. Ningún dato es jamás legible ni
escribible entre tenants.

Usted recibe un tenant ID durante el registro. Se parece a un identificador corto y
seguro para URL:

```text
acme-prod
```

El tenant ID aparece en cada ruta de API:

```
/v1/cas/acme-prod/<sha256>
/v1/ac/acme-prod/<action_digest>
```

## Personal Access Tokens (PATs)

Los PAT son el único tipo de credencial que CoreLink acepta para las llamadas a la
API. No hay claves de API, tokens OAuth ni cuentas de servicio — un PAT *es* la
cuenta de servicio.

### Propiedades del PAT

| Propiedad | Detalles |
|---|---|
| Formato | `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` — `<env>` es `pat` (PAT de usuario), `ci` (token de runner de CI) o `ro` (token de solo lectura) |
| Alcance | Exactamente un tenant en el momento de la emisión |
| Se muestra una vez | Se muestra en texto plano solo en la creación; nunca se almacena en texto plano del lado del servidor |
| Revocable | Solo el PAT inicial emitido en el registro existe self-service hoy; revocar o emitir PATs adicionales (`DELETE /v1/pats/:pat_id`, `POST /v1/pats`) aún no está conectado a ninguna ruta — mientras tanto, escriba a soporte |
| Vencimiento | Opcional; se establece en el momento de la creación; por defecto no vence |

### Alcances de PAT

Al crear un PAT, puede restringirlo a un subconjunto de operaciones:

| Alcance | Otorga |
|---|---|
| `cas:read` | `GET /v1/cas/*` |
| `cas:write` | `PUT /v1/cas/*` |
| `ac:read` | Leer entradas del action cache |
| `ac:write` | Escribir entradas del action cache |
| `admin` | Gestión de usuarios, gestión de PAT, exportación del log de auditoría |

Omitir un alcance significa que el PAT no puede realizar esa operación. El PAT
inicial emitido durante el registro tiene `cas:read cas:write ac:read ac:write` —
suficiente para todas las integraciones de herramientas de build.

### Buenas prácticas de CI/CD

No use su PAT inicial personal en CI. La creación self-service de un PAT de CI
dedicado (`POST /v1/pats`) está planificada pero aún no conectada a una ruta.
Mientras tanto, escriba a [support@humangr.com](mailto:support@humangr.com) para
solicitar un PAT de CI dedicado con los alcances mínimos requeridos
(típicamente `cas:read cas:write ac:read ac:write`, sin `admin`).

Almacene el valor del token devuelto en los secrets de GitHub Actions, en Vault o
en el gestor de secretos de su elección.

## Aislamiento entre tenants

CoreLink impone el aislamiento de tenants en cada capa:

1. **Enrutamiento de API**: cada solicitud de CAS y AC lleva el tenant ID en la
   ruta de la URL. El worker valida que el tenant del PAT coincida con el tenant de
   la ruta antes de tocar el almacenamiento.
2. **Capa de almacenamiento**: las claves de objeto de R2 llevan como prefijo el
   tenant ID. Un bug que omita la verificación del prefijo no puede producir una
   colisión de claves que filtre datos de otro tenant, porque el prefijo es
   obligatorio, no opcional.
3. **Log de auditoría**: cada evento lleva el tenant ID y se almacena en un
   namespace KV específico del tenant. Las consultas de nivel de administrador
   tienen alcance limitado al namespace del tenant que realiza la llamada.

Las lecturas entre tenants **no son posibles** — ni como opción de configuración,
ni por solicitud, ni mediante la API de administración. Si necesita compartir
artefactos entre dos tenants (por ejemplo, una biblioteca compartida usada por dos
equipos de producto), suba el blob bajo ambos tenants o use un único tenant
compartido con múltiples PAT con alcance por equipo.

Esta garantía de aislamiento está documentada como la invariante
**INV-TENANT-ISOLATION** en el modelo de seguridad. Consulte
[Seguridad](../security.md) para ver la lista completa de invariantes.

## Organización vs. tenant

Hoy, CoreLink tiene un mapeo 1:1 entre organización (unidad de registro) y tenant.
Las organizaciones multi-tenant (donde una entidad de facturación gestiona
sub-tenants para diferentes equipos o entornos) están en el roadmap, pero aún no
están disponibles.

Un patrón común mientras tanto: cree cuentas separadas para `acme-prod` y
`acme-staging`, cada una con sus propios PAT. Los pipelines de build se configuran
por entorno.

## Listar y revocar PATs

Hoy no existe una ruta self-service para listar o revocar PATs (`GET`/`POST
/v1/pats`, `DELETE /v1/pats/:pat_id` están planificadas pero no conectadas).
Existe una superficie de solo lectura admin-only para que soporte inspeccione
los PAT de un tenant (`GET /v1/admin/tenants/{tenant_id}/pats`), pero no es
llamable con un PAT normal. Para revocar un PAT, escriba a
[support@humangr.com](mailto:support@humangr.com).

Una vez que la revocación esté disponible (self-service o vía soporte),
cualquier solicitud en curso que use ese PAT recibirá `401 Unauthorized`.
