# Heldendatenbank – Entwurf

Stand: 2026-11-27

## Warum

ADBHelper soll die Daten aller Helden lokal speichern. Später soll darauf eine Kampfsimulation aufbauen. Deshalb müssen auch Helden drin sein, die man noch nicht besitzt.

## Ziele

- Eine lokale Datei mit **allen** Helden: Name, Fraktion, Klasse, Seltenheit, Schadensart, Reichweite.
- Pro Fertigkeit: Name, Art (Ult, Skill I, II, …), Abklingzeit, Reichweite, Energie, Freischaltstufe und die Zahlen je Stufe (Prozentwerte, Dauern).
- Für eigene Helden zusätzlich: Stufe, Aufstieg, EX-Waffe, und die **aktuellen Werte** aus „Hero Stats“ (HP, ATK, DEF, Krit, Haste, Proficiency usw.) mit Datum.
- Der Hero-Scan füllt und aktualisiert die Datei. Der bisherige AFKJ-Tracker-Export bleibt unverändert.

## Nicht-Ziele

- Die Kampfsimulation selbst.
- Eine Anzeige der Datenbank in der App (kommt später, wenn gewünscht).
- Texte aus fremden Quellen ohne Lizenz.

## Quellen und Lizenz

| Daten | Quelle | Bemerkung |
|---|---|---|
| Heldenliste (alle Helden), Fraktion, Klasse, Seltenheit | AFKJ Tracker `heroes-template.json`, zur Laufzeit geladen | nutzt der Scanner schon |
| Schadensart, Reichweite, Abklingzeiten, Freischaltstufen | github.com/arnecls/afjk-characters | keine Lizenz: nur reine Fakten, keine Texte; zur Laufzeit geladen, nicht mitgeliefert |
| Aktuelle Werte, Fertigkeitszahlen je Stufe | eigener Scan im Spiel | Hauptquelle für eigene Helden |
| Grundwerte je Aufstieg | gacha.gg | **nur wenn gacha.gg zustimmt** (Anfrage vom 2026-11-27) |

Spielfakten werden als Zahlen gespeichert, nicht als kopierte Beschreibungstexte. Die Spieltexte selbst (aus dem eigenen Spiel gelesen) werden nur lokal gespeichert.

## Ansätze

1. **JSON-Datei in `%APPDATA%\ADBHelper\heroes.json`** *(gewählt)*: wie `settings.json` und die Scan-Dateien, ohne neue Abhängigkeit, leicht lesbar und für die Simulation direkt ladbar. Bei rund 130 Helden ist die Größe kein Problem.
2. SQLite (rusqlite): Abfragen und Verlauf leichter, aber neue Abhängigkeit mit Lizenzprüfung und mehr Aufwand. Lohnt erst, wenn ein Werteverlauf über viele Scans gebraucht wird.
3. Mehrere Dateien je Held: unübersichtlich und ohne Vorteil.

## Aufbau (Vorschlag)

```json
{
  "version": 1,
  "updated": "2026-11-27T15:00:00Z",
  "heroes": {
    "frieren": {
      "name": "Frieren", "faction": "Celestial", "class": "Mage",
      "rarity": "Cele/Hypo", "damage_type": "Magic", "range": "Ranged",
      "owned": true,
      "progress": { "level": 900, "ascension": "Supreme+", "ex_weapon": 25, "scanned": "2026-11-27" },
      "stats": { "hp": 0, "atk": 2152000000, "phys_def": 283000000, "magic_def": 283000000,
                 "atk_spd": 8.0, "crit": 530.0, "haste": 44.1, "proficiency": 1040, "...": 0,
                 "scanned": "2026-11-27" },
      "skills": [
        { "slot": "ult", "name": "Zoltraak", "level": 5, "cooldown_s": 0, "range": "Global",
          "energy": 350, "unlock": null,
          "levels": [ { "level": 1, "values": [220, 30, 1400, 180] },
                      { "level": 2, "values": [240, 30, 1500, 180] } ],
          "text": "…", "source": "game" }
      ],
      "sources": { "list": "afkj-tracker", "facts": "afjk-characters", "stats": "game" }
    }
  }
}
```

- `values` sind die Zahlen in der Reihenfolge, in der sie im Text stehen; ATK- und SP-Anteile sind im Spiel farbig markiert und werden getrennt gespeichert (`atk`, `sp`), sobald das zuverlässig lesbar ist.
- Felder ohne Daten fehlen; ältere Dateien laden weiter (`serde(default)`).
- Nicht besessene Helden haben nur Liste und Fakten, bis eine Quelle für Werte erlaubt ist.

## Ablauf beim Scan

1. Heldenliste laden (Tracker) und Fakten (GitHub) laden; fehlt eine Quelle, bleibt der letzte Stand in der Datei.
2. Wie bisher die Heldenkarten in der Resonating Hall lesen.
3. Neu: je Held die Heldenseite öffnen, „Hero Stats“ (i neben der Kraft) lesen und durchscrollen, dann den Fertigkeiten-Reiter auf „Full“ schalten und jede Fertigkeit lesen.
4. Alles in `heroes.json` zusammenführen und atomar schreiben.

Schritt 3 kostet Zeit (geschätzt 20–40 s je Held, bei ~130 Helden über eine Stunde). Deshalb ist er ein **eigener Scan „Heldendatenbank“**, gestartet über einen Knopf auf der Seite **Einstellungen** (Entscheidung Dennis, 2026-11-27), nicht über die Aktionsliste im Dashboard. Daneben die Option „nur Helden ohne Daten oder älter als N Tage“. Der normale Hero-Scan (AFKJ-Tracker-Export) bleibt wie er ist und schreibt nur Liste, Fakten und Fortschritt in `heroes.json` mit.

Freigegeben von Dennis am 2026-11-27.

## Offen / dem Bau überlassen

- Genaue Vorlagen und Klickpunkte für „Hero Stats“ und den Fertigkeiten-Reiter.
- Wie die Zahlen je Fertigkeitsstufe aus dem Text getrennt werden (OCR mit Farbe).
- Ob das Spiel bei nicht besessenen Helden (Collections) Werte zeigt – nicht prüfbar mit dem Testkonto.
- Einbindung von gacha.gg, sobald eine Antwort da ist.
