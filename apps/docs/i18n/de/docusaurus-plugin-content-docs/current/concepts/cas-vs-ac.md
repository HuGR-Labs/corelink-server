---
id: cas-vs-ac
title: Inhaltsadressierbarer Speicher und Action Cache
sidebar_position: 1
description: Wie sich CAS und AC unterscheiden, wann jeweils welcher verwendet wird und wie Bazel beide zusammen nutzt.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/concepts/cas-vs-ac.md`

# Inhaltsadressierbarer Speicher und Action Cache

CoreLink stellt zwei unterschiedliche Caches bereit, die zusammenarbeiten. Die
meisten Nutzer denken nur an einen — den, der ihre Build-Ergebnisse speichert —
aber beide zu verstehen ist die 5-minütige Lektüre wert.

## Inhaltsadressierbarer Speicher (CAS)

CAS speichert beliebige Blobs, indiziert nach ihrem SHA-256-Digest. Der Schlüssel
*ist* der Digest: Es sind kein separater Dateiname, kein Versions-Tag und keine
Metadaten erforderlich.

```
key:   sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
value: <bytes>
```

Eigenschaften:

- **Unveränderlich**: Sobald ein Blob unter einem bestimmten Digest gespeichert
  ist, ändert sich der Inhalt unter diesem Digest nie.
- **Dedupliziert**: Wenn zwei Tenants (oder zwei CI-Jobs) dieselben Bytes
  hochladen, speichert die Speicherschicht eine Kopie. Beide Tenants zahlen für den
  Zugriff, nicht für doppelte Speicherung.
- **Überprüfbar**: Der Client, der `sha256(downloaded_bytes)` berechnet, stimmt
  immer mit dem zum Abrufen verwendeten Schlüssel überein.

### CAS im REAPI-Kontext

In der [Remote Execution API](https://github.com/bazelbuild/remote-apis) wird CAS
verwendet für:

- Inhalte von Quelldateien (Eingaben für Aktionen)
- Kompilierte Objektdateien und finale Artefakte (Ausgaben von Aktionen)
- `Directory`-Proto-Nachrichten, die den Eingabebaum beschreiben

Bazel lädt Eingaben in das CAS hoch, bevor eine Remote-Aktion abgeschickt wird. Der
Executor liest die Eingaben aus dem CAS, führt die Aktion aus und lädt die Ausgaben
zurück in das CAS.

### CAS-HTTP-Endpunkte

```
PUT /v1/cas/<tenant_id>/<sha256>   body: raw bytes  → 201 + {"hash": "sha256:<digest>"}
GET /v1/cas/<tenant_id>/<sha256>                    → 200 + raw bytes
```

Vollständige Details finden Sie in der [HTTP-API-Referenz](../api/http.md).

## Action Cache (AC)

Der Action Cache ordnet einen **action digest** einem **action result** zu. Ein
action digest ist der SHA-256 eines serialisierten `Action`-Protos — er kodiert den
Befehl, den Eingabebaum und die Plattformeigenschaften deterministisch. Das action
result erfasst die Ausgabe-Digests, den Exit-Code und die Zeitmessung.

```
key:   sha256(<Action proto>)
value: ActionResult { output_files: [...], exit_code: 0, ... }
```

Eigenschaften:

- **Überspringt redundante Arbeit**: Wenn `action_digest` im AC ist, ruft das
  Build-Tool die zwischengespeicherten Ausgaben aus dem CAS ab und überspringt die
  erneute Ausführung der Aktion.
- **Auf Tenant beschränkt**: AC-Einträge eines Tenants sind für einen anderen
  niemals sichtbar.
- **Durch jede Eingabeänderung invalidiert**: Da der Schlüssel der Digest der
  Eingaben + des Befehls ist, erzeugt jede Änderung an Quelldateien, Flags oder der
  Toolchain einen anderen Schlüssel — der Cache verfehlt sauber.

### Wann Bazel den AC verwendet

```
build tool
  1. Compute action_digest = sha256(Action{command, inputs, platform})
  2. GET /ac/<tenant>/action_digest  → HIT: fetch outputs from CAS, done
                                     → MISS: run action locally (or remotely)
  3. On success: PUT /ac/<tenant>/action_digest  → store result
                 PUT /v1/cas/<tenant>/<output_hash> → store each output
```

### Diagramm: CAS + AC zusammen

```
        ┌─────────────────────────────────────────────────┐
        │                   Bazel client                   │
        └───┬─────────────────────────────────┬───────────┘
            │ 1. check AC                      │ 3. PUT outputs to CAS
            ▼                                  ▼
     ┌─────────────┐                   ┌────────────────┐
     │ Action Cache│   2. AC miss →    │ Build executor │
     │  (AC)       │   run action      │ (local or RE)  │
     └─────────────┘                   └────────────────┘
            │ 4. write result back              │
            └──────────────────────────────────►│
                                                │ 5. read outputs from CAS
                                                ▼
                                       ┌────────────────┐
                                       │      CAS        │
                                       └────────────────┘
```

## Vergleich

| | CAS | Action Cache |
|---|---|---|
| Schlüssel | SHA-256 des Inhalts | SHA-256 des Action-Protos |
| Wert | Rohbytes | ActionResult (Ausgabe-Digests, Exit-Code) |
| Unveränderlich | Ja | Ja (Einträge werden nicht aktualisiert, nur einmal geschrieben) |
| Verwendet für | Blobs (Dateien, Protos) | Memoisierung von Build-Aktionen |
| Ohne REAPI verfügbar | Ja (REST-API) | Nur über REAPI-gRPC |
| Turborepo | Nicht direkt (Turbo verwendet ein eigenes Artefaktformat) | Turborepos `TURBO_API` bildet auf dieses Konzept ab |

## Was Turborepo "remote cache" nennt

Turborepo stellt CAS/AC nicht als eigenständige Konzepte dar. Seine
Remote-Cache-API ist ein vereinfachtes HTTP-Protokoll, bei dem Task-Ausgaben anhand
eines Hashes der Task-Eingaben gespeichert werden. CoreLink stellt einen
Turborepo-kompatiblen Endpunkt bereit — siehe
[Turborepo-Integration](../integrations/turborepo.md).
