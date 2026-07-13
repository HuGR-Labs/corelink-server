---
id: tenancy
title: Tenant-Modell und PAT-Geltungsbereich
sidebar_position: 2
description: "Wie CoreLink Tenants isoliert, wie PATs im Geltungsbereich eingeschränkt werden und wie tenant-übergreifender Zugriff aussieht (Antwort: unmöglich)."
---

<!-- i18n:MT (de) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Diese Seite wird übersetzt. Die englische Fassung gilt als verbindlich, bis ein Muttersprachler die Übersetzung freigibt (D+14).
>
> Canonical EN source: `docs/concepts/tenancy.md`

# Tenant-Modell und PAT-Geltungsbereich

## Was ist ein Tenant?

Ein **Tenant** ist die oberste Isolationsgrenze in CoreLink. Jedes Datenelement —
CAS-Blobs, AC-Einträge, Audit-Ereignisse, Abrechnungsdatensätze — gehört zu genau
einem Tenant. Keine Daten sind jemals tenant-übergreifend les- oder schreibbar.

Sie erhalten während der Registrierung eine Tenant-ID. Sie sieht aus wie eine
kurze, URL-sichere Kennung:

```text
acme-prod
```

Die Tenant-ID erscheint in jedem API-Pfad:

```
/v1/cas/acme-prod/<sha256>
/v1/ac/acme-prod/<action_digest>
```

## Personal Access Tokens (PATs)

PATs sind der einzige Anmeldeinformationstyp, den CoreLink für API-Aufrufe
akzeptiert. Es gibt keine API-Schlüssel, OAuth-Tokens oder Dienstkonten — ein PAT
*ist* das Dienstkonto.

### PAT-Eigenschaften

| Eigenschaft | Details |
|---|---|
| Präfix | `clk_live_` (Produktion) oder `clk_test_` (Testumgebung) |
| Geltungsbereich | Genau ein Tenant zum Zeitpunkt der Ausstellung |
| Einmal angezeigt | Wird nur bei der Erstellung im Klartext angezeigt; der Hash wird serverseitig gespeichert |
| Widerrufbar | Jederzeit über das Admin-Dashboard oder `DELETE /v1/pats/:pat_id` |
| Ablauf | Optional; bei der Erstellung festgelegt; standardmäßig ohne Ablauf |

### PAT-Geltungsbereiche

Beim Erstellen eines PAT können Sie ihn auf eine Teilmenge von Operationen
beschränken:

| Geltungsbereich | Gewährt |
|---|---|
| `cas:read` | `GET /v1/cas/*` |
| `cas:write` | `PUT /v1/cas/*` |
| `ac:read` | Action-Cache-Einträge lesen |
| `ac:write` | Action-Cache-Einträge schreiben |
| `admin` | Benutzerverwaltung, PAT-Verwaltung, Export des Audit-Logs |

Das Weglassen eines Geltungsbereichs bedeutet, dass der PAT diese Operation nicht
ausführen kann. Der bei der Registrierung ausgestellte Start-PAT hat
`cas:read cas:write ac:read ac:write` — genug für alle Build-Tool-Integrationen.

### CI/CD-Best-Practice

Verwenden Sie Ihren persönlichen Start-PAT nicht in der CI. Erstellen Sie einen
dedizierten CI-PAT mit den minimal erforderlichen Geltungsbereichen:

```bash
# Create a CI PAT with CAS + AC access only (no admin)
curl -s -X POST \
  -H "Authorization: Bearer $CORELINK_ADMIN_PAT" \
  -H "Content-Type: application/json" \
  -d '{"label": "ci-bazel", "scopes": ["cas:read", "cas:write", "ac:read", "ac:write"]}' \
  https://corelink-api.humangr.com/v1/pats
```

Speichern Sie den zurückgegebenen Token-Wert in den GitHub-Actions-Secrets, in
Vault oder im Secrets-Manager Ihrer Wahl.

## Tenant-übergreifende Isolation

CoreLink erzwingt die Tenant-Isolation auf jeder Ebene:

1. **API-Routing**: Jede CAS- und AC-Anfrage trägt die Tenant-ID im URL-Pfad. Der
   Worker validiert, dass der Tenant des PAT mit dem Tenant im Pfad übereinstimmt,
   bevor auf den Speicher zugegriffen wird.
2. **Speicherschicht**: R2-Objektschlüssel sind mit der Tenant-ID präfixiert. Ein
   Bug, der die Präfixprüfung auslässt, kann keine Schlüsselkollision erzeugen, die
   Daten eines anderen Tenants preisgibt, weil das Präfix verpflichtend und nicht
   optional ist.
3. **Audit-Log**: Jedes Ereignis trägt die Tenant-ID und wird in einem
   tenant-spezifischen KV-Namespace gespeichert. Abfragen auf Admin-Ebene sind auf
   den Namespace des aufrufenden Tenants beschränkt.

Tenant-übergreifende Lesevorgänge sind **nicht möglich** — weder als
Konfigurationsoption noch auf Anfrage noch über die Admin-API. Wenn Sie Artefakte
zwischen zwei Tenants teilen müssen (z. B. eine gemeinsam genutzte Bibliothek, die
von zwei Produktteams verwendet wird), laden Sie den Blob unter beiden Tenants hoch
oder verwenden Sie einen einzigen gemeinsamen Tenant mit mehreren nach Team
eingeschränkten PATs.

Diese Isolationsgarantie ist als Invariante **INV-TENANT-ISOLATION** im
Sicherheitsmodell dokumentiert. Die vollständige Liste der Invarianten finden Sie
unter [Sicherheit](../security.md).

## Organisation vs. Tenant

Heute hat CoreLink eine 1:1-Zuordnung zwischen Organisation
(Registrierungseinheit) und Tenant. Multi-Tenant-Organisationen (bei denen eine
Abrechnungsentität Sub-Tenants für verschiedene Teams oder Umgebungen verwaltet)
stehen auf der Roadmap, sind aber noch nicht verfügbar.

Ein gängiges Muster in der Zwischenzeit: Erstellen Sie separate Konten für
`acme-prod` und `acme-staging`, jeweils mit ihren eigenen PATs. Build-Pipelines
werden pro Umgebung konfiguriert.

## PATs auflisten und widerrufen

```bash
# List all PATs for your tenant
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats

# Revoke a PAT by ID
curl -s -X DELETE \
  -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/pats/<pat_id>
```

Nach dem Widerruf erhalten alle laufenden Anfragen, die diesen PAT verwenden,
`401 Unauthorized`.
