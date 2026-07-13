---
id: oci-registry
title: OCI-Registry-Integration (Docker / Podman)
sidebar_position: 5
description: Übertragen und laden Sie Container-Images und OCI-Artefakte in CoreLink, eine vollständige Registry gemäß OCI Distribution Spec v1.1.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/integrations/oci-registry.md`

# OCI-Registry-Integration (Docker / Podman)

CoreLink ist eine vollständige Registry gemäß **OCI Distribution Spec v1.1**. Jeder standardmäßige
OCI-Client — `docker`, `podman`, `buildah`, `crane`, `helm` (OCI-Charts), BuildKit-
Cache-Exporter — kann per Push und Pull darauf zugreifen. Manifeste werden im
Key-Value-Speicher pro Tenant abgelegt; Blobs (Layer und Configs) liegen im CAS des Tenants.

Der Registry-Host ist `corelink-api.humangr.com`. Anders als die übrigen Cache-
Oberflächen hat der OCI-Pfad **kein Tenant-Segment** in der URL — Ihr Tenant wird
aus dem Token abgeleitet, mit dem Sie sich authentifizieren, über den standardmäßigen
zweistufigen Bearer-Token-Ablauf der Registry (`GET /token`, dann `Authorization: Bearer`).

## Voraussetzungen

- `docker` (oder `podman`) installiert.
- Ein CoreLink-PAT (`corelink_pat_...`) mit Lese- und Schreibberechtigung für den Cache.

## Anmelden

Melden Sie sich mit Ihrem PAT als Passwort an. Der Benutzername wird nicht geprüft — jeder Wert
(zum Beispiel `corelink`) funktioniert:

```bash
echo "$CORELINK_PAT" | docker login corelink-api.humangr.com \
  --username corelink --password-stdin
```

Docker führt den Token-Austausch beim nächsten Push oder Pull automatisch durch.

## Ein Image übertragen

Taggen Sie das Image mit dem CoreLink-Host und pushen Sie es. Das erste Pfadsegment nach dem
Host ist der **Repository-Name** (kein Tenant):

```bash
docker tag my-app:latest corelink-api.humangr.com/my-app:latest
docker push corelink-api.humangr.com/my-app:latest
```

## Ein Image laden

```bash
docker pull corelink-api.humangr.com/my-app:latest
```

Podman verwendet dieselbe Referenz:

```bash
podman pull corelink-api.humangr.com/my-app:latest
```

:::note Die Isolation erfolgt pro Tenant, mit dem PAT als Schlüssel
Zwei Tenants können beide `my-app:latest` ohne Kollision pushen — jedes Repository ist
auf den Tenant beschränkt, der aus dem authentifizierenden PAT aufgelöst wird. Der `_catalog`-
Endpunkt ist standardmäßig deaktiviert.
:::

## Fehlerbehebung

| Symptom | Wahrscheinliche Ursache | Lösung |
|---|---|---|
| `401 Unauthorized` beim Push | Nicht angemeldet oder PAT abgelaufen | Führen Sie `docker login` erneut mit einem aktuellen PAT aus |
| `denied: requested access to the resource is denied` | Dem PAT fehlt die Schreibberechtigung | Verwenden Sie einen PAT mit Schreibberechtigung für den Cache |
| `manifest unknown` beim Pull | Das Image wurde nie in diesen Tenant übertragen | Pushen Sie es zuerst oder prüfen Sie Host/Name der Referenz |

Vollständige Fehlerreferenz: [Fehlerbehebung](../troubleshooting.md).
