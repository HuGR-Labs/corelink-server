---
id: pip
title: pip-Spiegel (PyPI)
sidebar_position: 7
description: Verweisen Sie pip, uv, poetry oder pdm als Caching-Spiegel vor PyPI auf CoreLink.
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/integrations/pip.md`

# pip-Spiegel (PyPI)

CoreLink ist ein **Caching-Spiegel** vor `pypi.org`. Er implementiert die
Simple Index API (PEP 691 JSON, mit PEP-503-HTML-Fallback), sodass `pip`, `uv`,
`poetry` und `pdm` ihn als Paketindex verwenden können. Wheels und Quelldistributionen
werden im CAS Ihres Tenants zwischengespeichert und vor dem Zwischenspeichern per Integritätsprüfung
gegen das `#sha256=`-Fragment des Upstreams geprüft.

Dies ist ein **schreibgeschützter Spiegel** zur Installation öffentlicher Pakete. Er hostet keine
privaten Indizes und akzeptiert kein `twine upload`.

## Voraussetzungen

- `pip` (oder ein kompatibler Client) installiert.
- Ein CoreLink-PAT (`corelink_pat_...`).
- Die UUID Ihres Tenants.

## Konfigurieren

pip authentifiziert sich per HTTP-Basic-Auth, wobei der Benutzername `hugr` und das
Passwort Ihr PAT ist. Tragen Sie die Simple-Index-URL Ihres Tenants — mit eingebetteten
Anmeldedaten — in `pip.conf` ein (`~/.config/pip/pip.conf` unter Linux,
`~/Library/Application Support/pip/pip.conf` unter macOS):

```ini
[global]
index-url = https://hugr:corelink_pat_XXXXXXXXXXXX@corelink-api.humangr.com/pip/<your-tenant-id>/simple/
```

Installieren Sie anschließend wie gewohnt:

```bash
pip install requests
```

Oder übergeben Sie sie inline für eine einmalige Nutzung (und für die CI, wo der PAT aus einem Secret stammt):

```bash
pip install --index-url "https://hugr:${CORELINK_PAT}@corelink-api.humangr.com/pip/<your-tenant-id>/simple/" requests
```

`uv` berücksichtigt dieselbe `index-url`:

```bash
uv pip install --index-url "https://hugr:${CORELINK_PAT}@corelink-api.humangr.com/pip/<your-tenant-id>/simple/" requests
```

## Überprüfen, ob es funktioniert hat

```bash
pip install --force-reinstall --no-cache-dir -v requests 2>&1 \
  | grep corelink-api.humangr.com | head
```

`corelink-api.humangr.com/pip/.../simple/` im ausführlichen Protokoll bestätigt, dass
pip über den Spiegel auflöst.

## Fehlerbehebung

| Symptom | Wahrscheinliche Ursache | Lösung |
|---|---|---|
| `401 Unauthorized` | Der Benutzername ist nicht `hugr`, oder das Passwort ist kein `corelink_pat_...`-PAT | Verwenden Sie `hugr` als Benutzernamen und Ihren PAT als Passwort |
| `Could not find a version` | Projekt noch nicht zwischengespeichert und Upstream nicht erreichbar | Wiederholen Sie es; CoreLink holt es bei der ersten Anfrage von PyPI |
| Hash-Abweichung bei einem Wheel | Das Upstream-Artefakt hat sich geändert | CoreLink prüft das `#sha256=`-Fragment und schlägt bei einer Abweichung fehlsicher fehl |

Vollständige Fehlerreferenz: [Fehlerbehebung](../troubleshooting.md).
