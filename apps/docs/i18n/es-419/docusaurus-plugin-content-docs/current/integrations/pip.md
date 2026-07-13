---
id: pip
title: Espejo de pip (PyPI)
sidebar_position: 7
description: Apunte pip, uv, poetry o pdm a CoreLink como un espejo de caché delante de PyPI.
---

<!-- i18n:MT (es-419) — TMX-seeded machine-translation stub; replace with native-speaker translation before GA -->

> MT: Esta página está en traducción. La versión canónica en inglés es la fuente de verdad hasta la revisión por hablante nativo (D+10).
>
> Canonical EN source: `docs/integrations/pip.md`

# Espejo de pip (PyPI)

CoreLink es un **espejo de caché** delante de `pypi.org`. Implementa la
Simple Index API (PEP 691 JSON, con fallback a HTML PEP 503), de modo que `pip`, `uv`,
`poetry` y `pdm` puedan usarlo como su índice de paquetes. Los wheels y las distribuciones de
código fuente se almacenan en caché en el CAS de su tenant y se les verifica la integridad contra el
fragmento `#sha256=` del upstream antes de almacenarlos en caché.

Este es un **espejo de solo lectura** para instalar paquetes públicos. No hospeda
índices privados y no acepta `twine upload`.

## Requisitos previos

- `pip` (o un cliente compatible) instalado.
- Un PAT de CoreLink (`corelink_pat_...`).
- El UUID de su tenant.

## Configurar

pip se autentica con HTTP basic auth, donde el nombre de usuario es `hugr` y la
contraseña es su PAT. Coloque la URL del índice Simple de su tenant — con las credenciales
incrustadas — en `pip.conf` (`~/.config/pip/pip.conf` en Linux,
`~/Library/Application Support/pip/pip.conf` en macOS):

```ini
[global]
index-url = https://hugr:corelink_pat_XXXXXXXXXXXX@corelink-api.humangr.com/pip/<your-tenant-id>/simple/
```

Luego instale de forma normal:

```bash
pip install requests
```

O pásela inline para un uso puntual (y para CI, donde el PAT proviene de un secret):

```bash
pip install --index-url "https://hugr:${CORELINK_PAT}@corelink-api.humangr.com/pip/<your-tenant-id>/simple/" requests
```

`uv` respeta la misma `index-url`:

```bash
uv pip install --index-url "https://hugr:${CORELINK_PAT}@corelink-api.humangr.com/pip/<your-tenant-id>/simple/" requests
```

## Verificar que funcionó

```bash
pip install --force-reinstall --no-cache-dir -v requests 2>&1 \
  | grep corelink-api.humangr.com | head
```

Ver `corelink-api.humangr.com/pip/.../simple/` en el registro detallado confirma que
pip está resolviendo a través del espejo.

## Resolución de problemas

| Síntoma | Causa probable | Solución |
|---|---|---|
| `401 Unauthorized` | El nombre de usuario no es `hugr`, o la contraseña no es un PAT `corelink_pat_...` | Use `hugr` como nombre de usuario y su PAT como contraseña |
| `Could not find a version` | Proyecto aún no almacenado en caché y upstream inaccesible | Reintente; CoreLink obtiene de PyPI en la primera solicitud |
| Discrepancia de hash en un wheel | El artefacto del upstream cambió | CoreLink verifica el fragmento `#sha256=` y falla de forma cerrada ante una discrepancia |

Referencia completa de errores: [Resolución de problemas](../troubleshooting.md).
