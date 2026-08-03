---
id: bazel
title: Bazel-Integration
sidebar_position: 1
description: Konfigurieren Sie Bazel so, dass CoreLink über .bazelrc als Remote-Cache verwendet wird.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/integrations/bazel.md`

# Bazel-Integration

CoreLink implementiert den Cache der **Bazel Remote Execution API v2 (REAPI v2)** als
ByteStream-REST-Schema:

```text
https://corelink-api.humangr.com/bazel/v2/<your-tenant-id>/blobs/<hash>/<size>
```

Das Pfadsegment `<instance>` ist die UUID Ihres Tenants.

:::tip Zwei Möglichkeiten, Bazel auf CoreLink zu verweisen — beide aktiv
- **Standard-Plain-HTTP-Remote-Cache (am einfachsten):** Verweisen Sie `--remote_cache` auf den
  **stock-HTTP-Alias** `https://corelink-api.humangr.com/bazel/cache` — er bedient die
  Pfade `/cas/<sha256>` und `/ac/<sha256>`, die Standard-Bazel ausgibt (`PUT`→`204`, `GET`→`200`).
  Es wird kein REAPI-Client benötigt.
- **REAPI v2 / ByteStream:** Verweisen Sie `--remote_cache` auf `/bazel/v2` (die Konfiguration dieses Dokuments)
  für einen ByteStream-kompatiblen Client.

Beide werden per Bearer-PAT authentifiziert. (Bazel adressiert Inhalte per SHA-256, was die
`/bazel/*`-Routen akzeptieren; der *native* REST-CAS unter `/v1/cas/...` ist BLAKE3-basiert — siehe
[Rohes HTTP (curl)](./raw-curl).)
:::

## Voraussetzungen

- Standard-Bazel (für den Alias `/bazel/cache`) oder ein REAPI/ByteStream-
  kompatibler Client (für `/bazel/v2`). Beides funktioniert.
- Ein CoreLink-PAT (`corelink_pat_...`) mit Lese- und Schreibberechtigung für den Cache. Siehe [PAT-Erstellung](../concepts/tenancy.md).

## `.bazelrc` konfigurieren

Das Repository liefert eine eingecheckte Referenzkonfiguration unter
[`apps/examples/bazel/.bazelrc`](https://github.com/HumanGuardrail/corelink-server/blob/main/apps/examples/bazel/.bazelrc).
Sie verweist die REAPI-Instanz von Bazel auf Ihren Tenant:

```ini
# Point at the CoreLink REAPI v2 endpoint (the /bazel/v2 prefix is required).
build --remote_cache=https://corelink-api.humangr.com/bazel/v2

# Your tenant UUID becomes the REAPI :instance path segment.
build --remote_instance_name=${CORELINK_TENANT}

# Authenticate with your PAT.
build --remote_header=Authorization=Bearer ${CORELINK_PAT}

build --remote_upload_local_results=true
build --remote_timeout=60
```

Exportieren Sie beide Werte vor dem Kompilieren; in der CI übergeben Sie den PAT aus einem Secret, damit er
nie im Klartext erscheint:

```bash
export CORELINK_PAT="corelink_pat_XXXXXXXXXXXX"   # ${{ secrets.CORELINK_PAT }} in CI
export CORELINK_TENANT="acme-prod"
```

## Überprüfen, ob es funktioniert hat

Führen Sie einen Build aus und überprüfen Sie anschließend, ob PAT und Tenant erkannt werden:

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

Führen Sie für eine Cache-Treffer-Prüfung denselben Build zweimal aus. Prüfen Sie das
Ausführungsprotokoll von Bazel (`--execution_log_json_file`) auf Einträge `remoteCacheHit: true` beim zweiten
Durchlauf.

## Behebung Bazel-spezifischer Probleme

| Symptom | Wahrscheinliche Ursache | Lösung |
|---|---|---|
| Jede Anfrage gibt 404 zurück | `--remote_cache` verweist auf das falsche Präfix | Verwenden Sie `/bazel/cache` (Standard-Plain-HTTP) oder `/bazel/v2` (REAPI/ByteStream) — beide aktiv; ein reiner Host oder `/v1/cas` gibt für die Bazel-Pfade 404 zurück |
| `UNAUTHENTICATED` | Fehlender oder falscher `Authorization`-Header | Prüfen Sie, ob `CORELINK_PAT` in Ihrer Shell / CI-Umgebung exportiert ist |
| `PERMISSION_DENIED` / 403 | Der Instanzname ist nicht Ihr Tenant | Setzen Sie `--remote_instance_name` auf die UUID Ihres Tenants |
| Cache-Miss bei jedem Build | `--remote_upload_local_results=false` | Setzen Sie ihn in mindestens einem CI-Job auf `true` |

Vollständige Fehlerreferenz: [Fehlerbehebung](../troubleshooting.md).
