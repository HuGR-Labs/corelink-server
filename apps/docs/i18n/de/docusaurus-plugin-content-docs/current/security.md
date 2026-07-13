---
id: security
title: Sicherheitsmodell
sidebar_position: 11
description: PAT-Lebenszyklus, Scopes, Rotationsrichtlinie, Audit-Log und die INV-TENANT-ISOLATION-Garantie.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/security.md`

# Sicherheitsmodell

## PAT-Lebenszyklus

Personal Access Tokens (PATs) sind der einzige Anmeldeinformationstyp, den CoreLink akzeptiert. Das Verständnis ihres Lebenszyklus ist für einen sicheren Betrieb entscheidend.

### Erstellung

PATs werden an zwei Stellen erstellt:

1. **Registrierungsassistent** — stellt ein Starter-PAT mit den Scopes `cas:read cas:write ac:read ac:write` aus.
2. **Admin-Dashboard** oder `POST /v1/pats` — stellt ein PAT mit einer beliebigen Teilmenge von Scopes aus.

Bei der Erstellung wird der Klartext-Token **genau einmal** angezeigt. CoreLink speichert nur einen gesalzenen SHA-256-Hash des Token-Werts. Es gibt keinen Endpunkt zum Abrufen.

**Speichern Sie Ihr PAT in einem Secret-Manager, bevor Sie den Erstellungsdialog schließen.** Geeignete Optionen:

- 1Password / Bitwarden (persönlich)
- AWS Secrets Manager / HashiCorp Vault (Team/CI)
- GitHub Actions secrets (CI-Workflows)
- Doppler / Infisical (Plattform-Engineering)

Speichern Sie PATs nicht in:

- Git-Repositorys (auch nicht in privaten)
- `.env`-Dateien, die in der Versionsverwaltung eingecheckt sind
- CI-Job-Logs (`echo $CORELINK_PAT` ist zum lokalen Debuggen in Ordnung; nicht in CI)
- Slack-/Discord-Nachrichten

### Nur HTTPS

CoreLink stellt kein HTTP bereit. Der gesamte API-Verkehr verwendet TLS 1.2 oder TLS 1.3. HTTPS wird am Cloudflare-Edge erzwungen; es gibt keine Möglichkeit zur Deaktivierung. Verbindungen auf Port 80 werden auf Port 443 umgeleitet.

### Rotation

PATs haben keine automatische Rotation. Sie müssen manuell rotieren. Empfohlene Rotationskadenz:

| PAT-Typ | Kadenz |
|---|---|
| Starter (persönlich) | 90 Tage oder bei Personalwechsel |
| CI/CD | 90 Tage oder bei Teamwechsel |
| Integration (gemeinsam genutzt) | 30 Tage |

So rotieren Sie ein PAT:

1. Erstellen Sie das neue PAT (`POST /v1/pats`).
2. Aktualisieren Sie das Secret in Ihrem Secret-Manager.
3. Deployen/starten Sie alles neu, das das alte PAT nutzt.
4. Widerrufen Sie das alte PAT (`DELETE /v1/pats/:pat_id`).

Löschen Sie das alte PAT nicht, bevor das neue in Gebrauch ist — es gibt keine Übergangsfrist.

### Widerruf

```bash
# List PATs to find the ID
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats

# Revoke
curl -s -X DELETE \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/pats/<pat_id>"
```

Der Widerruf erfolgt sofort. Laufende Anfragen, die das widerrufene PAT verwenden, schlagen innerhalb von Millisekunden mit `401` fehl (begrenzt durch das Propagationsfenster des Cloudflare-Edge, typischerweise < 100 ms).

## Scopes

PATs werden zum Zeitpunkt der Erstellung mit Scopes versehen. Die verfügbaren Scopes sind:

| Scope | Gewährt |
|---|---|
| `cas:read` | Blobs aus dem CAS lesen (herunterladen) |
| `cas:write` | Blobs in den CAS schreiben (hochladen) |
| `ac:read` | Action-Cache-Einträge lesen |
| `ac:write` | Action-Cache-Einträge schreiben |
| `admin` | PAT-Verwaltung, Benutzerverwaltung, Audit-Log-Export |

Prinzip der geringsten Rechte: Geben Sie jedem PAT den minimal erforderlichen Satz an Scopes. Ein CI-Job, der nur den Cache befüllt, benötigt `cas:write ac:write`; ein schreibgeschützter Cache-Proxy benötigt `cas:read ac:read`.

## Tenant-Isolation (INV-TENANT-ISOLATION)

Die zentrale Sicherheitsinvariante von CoreLink:

> **INV-TENANT-ISOLATION**: Keine Anfrage kann Daten lesen oder schreiben, die zu einem anderen Tenant als dem im PAT kodierten gehören, unabhängig von URL-Pfad, Headern oder Anfragekörper.

Dies wird auf zwei Ebenen erzwungen:

1. **PAT-Validierung**: Der Worker löst das PAT zu einer `tenant_id` auf. Stimmt der Tenant im URL-Pfad nicht überein, wird die Anfrage mit `403` abgelehnt, bevor irgendein Speichervorgang stattfindet.
2. **Speicherschlüssel-Namespace**: R2-Objektschlüssel erhalten das Präfix `<tenant_id>/cas/<hash>`. Ein Speicher-Bug, der die Tenant-Prüfung versehentlich auslässt, kann keine Kollision erzeugen, weil der Schlüssel weiterhin das Tenant-Präfix enthält.

CoreLink bietet keine tenant-übergreifende Freigabe. Wenn zwei Teams Artefakte teilen müssen, müssen sie einen gemeinsamen Tenant verwenden oder das Artefakt unabhängig in beide Tenants pushen.

## Was wir auditieren

Jeder erfolgreiche CAS-Lesevorgang, CAS-Schreibvorgang und AC-Vorgang wird an ein unveränderliches, tenant-bezogenes Audit-Log angehängt. Jeder Eintrag erfasst:

| Feld | Beispiel |
|---|---|
| `event_type` | `cas.write`, `cas.read`, `ac.write`, `ac.read` |
| `tenant_id` | `acme-prod` |
| `content_hash` | `sha256:e3b0c4...` |
| `pat_prefix` | `clk_live` |
| `ip_address` | `1.2.3.4` (hashed in GDPR-constrained regions) |
| `timestamp` | `2026-05-28T08:42:00.123Z` |
| `bytes` | `4096` |

Das Audit-Log ist reines Anhängen (append-only). Einzelne Einträge können nicht gelöscht werden. Sie können das Audit-Log Ihres Tenants als JSON oder CSV aus dem Admin-Dashboard exportieren.

## Verschlüsselung im Ruhezustand

Alle in R2 gespeicherten Blobs werden im Ruhezustand mit AES-256 verschlüsselt (standardmäßig von Cloudflare verwaltete Schlüssel). Tenants des Enterprise-Tarifs können ihren eigenen AES-256-Schlüssel bereitstellen (BYOK). Kontaktieren Sie den Vertrieb, um BYOK zu aktivieren.

## Verantwortungsvolle Offenlegung

Wenn Sie eine Sicherheitslücke in CoreLink finden, senden Sie bitte eine E-Mail an [security@humangr.com](mailto:security@humangr.com). Wir bestätigen Meldungen innerhalb von 24 Stunden und streben bei kritischen Problemen Patches innerhalb von 72 Stunden an.

Öffnen Sie keine öffentlichen GitHub-Issues für Sicherheitslücken.
