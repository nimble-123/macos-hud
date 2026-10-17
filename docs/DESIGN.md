# Product Design — hud

**Version:** 0.1.0 · **Status:** zur Prüfung · **Stand:** 2026-10-17

Dieses Dokument legt die Produktidentität von `hud` fest. TUI-Theme, README, Diagramme und künftige Grafiken leiten sich davon ab.

## 1. Gegenstand

| | |
|---|---|
| **Produkt** | `hud`: ein Head-up-Display für die eigene Entwicklermaschine im Terminal |
| **Zielgruppe** | Entwickler:innen mit vielen lokalen Repositories, Paketmanagern und KI-Werkzeugen |
| **Hauptaufgabe** | Auf einen Blick zeigen, *was Aufmerksamkeit braucht*, und Routinearbeit (Fast-Forward, Updates) sicher erledigen |

## 2. Leitidee: Glass Cockpit und „Dark Cockpit“

Moderne Cockpits folgen dem **Dark-Cockpit-Prinzip**: Solange alles normal ist, leuchtet nichts. Farbe erscheint nur, wenn etwas Aufmerksamkeit verlangt. Die Farben folgen dem Crew-Alerting-Code (EICAS) und tragen feste Bedeutung.

`hud` übernimmt beides:

1. **Normalzustand ist ruhig.** Aktuelle, saubere Repos und aktuelle Pakete erscheinen in neutraler Schrift. Ein grünes Signal kommt nur kurz nach einer erledigten Aktion.
2. **Farbe = Bedeutung.** Jede Statusfarbe hat genau eine Bedeutung, überall gleich: TUI, Doku, Diagramme.
3. **Magenta = Fokus.** Wie die Kurslinie im Flight Management System markiert Magenta, *wo man gerade ist* (Auswahl, aktiver Tab, Fokus) und ist die Markenfarbe. Magenta kodiert nie einen Status.

## 3. Farbsystem

Alle Textfarben sind gegen ihren Hintergrund auf WCAG ≥ 4.5:1 geprüft (2026-10-17).

### 3.1 Basis

| Rolle | Hell | Dunkel | Verwendung |
|---|---|---|---|
| `panel` | `#F2F3F1` | `#161B22` | Hintergrund (Doku/Diagramme; die TUI nutzt den Terminal-Hintergrund) |
| `ink` | `#1C2128` | `#E6E8E3` | Primärtext |
| `muted` | `#47505C` | `#AEB5BD` | Sekundärtext, Rahmen, normale Zustände |
| `soft` | `#5A6470` | `#8F98A3` | Hinweise, Tastenkürzel, Einheiten |
| `focus` | `#B02A8A` | `#E870C6` | Marke, Auswahl, aktiver Tab, Fokusrahmen |

### 3.2 Status (EICAS)

| Status | Symbol | Hell | Dunkel | Bedeutung in hud |
|---|---|---|---|---|
| `ok` | ● | `#24774A` | `#5CC98A` | gerade erledigt (Fast-Forward durchgeführt, Update installiert) |
| `advisory` | ◆ | `#13738A` | `#4CC0D6` | Hinweis: Repo liegt zurück, Update verfügbar, Aktion geplant |
| `caution` | ▲ | `#8F6200` | `#E8B23A` | Achtung: übersprungen (dirty, ahead, opted out), manuelle Aktion nötig |
| `warning` | ✖ | `#B42F25` | `#F0675B` | Warnung: Fehler, divergiert, CLI nicht verfügbar |
| `neutral` | ○ | `muted` | `muted` | normal, nichts zu tun, unbekannt |

**Regeln**

- Status wird **immer** über Symbol + Text + Farbe vermittelt, nie über Farbe allein (Farbsehschwäche, `NO_COLOR`).
- Pro Zeile höchstens ein Statussymbol.
- `warning` hat Vorrang vor `caution` vor `advisory`. Ein Repo zeigt immer nur den höchsten Status.

### 3.3 Terminal-Abbildung

| Modus | Erkennung | Abbildung |
|---|---|---|
| Truecolor | `COLORTERM=truecolor\|24bit` | Hex-Werte aus 3.1/3.2, hell oder dunkel nach Theme |
| 256 Farben | `TERM` enthält `256color` | nächstliegende xterm-256-Farbe (beim Start berechnet) |
| 16 Farben | sonst | `focus`→Magenta, `advisory`→Cyan, `caution`→Yellow, `warning`→Red, `ok`→Green, `muted`→BrightBlack |
| Keine Farbe | `NO_COLOR` gesetzt | nur Symbole, Fett und Unterstreichung |

**Theme-Erkennung:** `theme = "auto" | "dark" | "light"` in der Config, Default `auto`. Bei `auto` fragt hud beim Start den Terminal-Hintergrund per OSC 11 ab (Timeout 100 ms, Fallback `dark`). hud malt **keinen** eigenen Hintergrund: Es nutzt den Hintergrund des Terminals und färbt nur Vordergrund und Rahmen.

### 3.4 Symbolsätze

hud kennt die Terminal-Schrift nicht und kann sie nicht zuverlässig erkennen. Der Symbolsatz ist deshalb einstellbar: `icons = "unicode" | "nerd" | "ascii"`, Default `unicode`.

| Bedeutung | `unicode` (Default) | `nerd` (Nerd Fonts, z. B. Hack Nerd Font) | `ascii` |
|---|---|---|---|
| ok | ● | `nf-fa-check_circle` | `+` |
| advisory | ◆ | `nf-fa-info_circle` | `i` |
| caution | ▲ | `nf-fa-exclamation_triangle` | `!` |
| warning | ✖ | `nf-fa-times_circle` | `x` |
| neutral | ○ | `nf-fa-circle_o` | `-` |
| Git-Repository | — | `nf-dev-git_branch` | — |
| Homebrew | — | `nf-fa-beer` | — |
| npm | — | `nf-dev-npm` | — |
| Skills | — | `nf-fa-puzzle_piece` | — |
| KI-Anbieter | — | `nf-md-robot` | — |
| CPU / Speicher | — | `nf-oct-cpu` / `nf-md-memory` | — |

- Quell-Icons (Git, Homebrew, npm, Skills, KI) erscheinen nur im Satz `nerd`; in `unicode` und `ascii` steht dort der Name der Quelle.
- Der Einrichtungsdialog zeigt eine Vorschau beider Sätze und fragt, ob die Nerd-Icons korrekt dargestellt werden; die Antwort setzt `icons`.
- Ligaturen rendert das Terminal; hud gibt dafür nichts vor. Texte vermeiden Zeichenfolgen, deren Ligatur die Bedeutung verfälschen könnte (z. B. `->` als Pfeil, stattdessen `→`).
- Die Glyphen-Namen folgen dem Nerd-Fonts-Cheat-Sheet; die Abbildung auf Codepoints liegt in `hud-tui`.

## 4. Typografie

| Kontext | Schrift | Rolle |
|---|---|---|
| TUI | Terminal-Schrift der Nutzer:in | hud gibt keine Schrift vor; empfohlen ist eine Nerd Font (z. B. Hack Nerd Font) für den Symbolsatz `nerd` |
| Doku-Titel, Wortmarke | **Chakra Petch** 600 | technisch-eckig, Anmutung von Cockpit-Beschriftung |
| Diagramm-Knoten | **Chakra Petch** 600 | Namen von Bausteinen |
| Technische Werte | **IBM Plex Mono** 400/500 | Pfade, Befehle, Typen, Ports |
| Anmerkungen | **IBM Plex Serif** *italic* | redaktionelle Randnotizen in Diagrammen |

Zahlen in der TUI sind rechtsbündig mit fester Breite formatiert (entspricht `tabular-nums`). Aktualisierte Werte verschieben dadurch das Layout nicht.

## 5. TUI-Layout

```
┌ hud ───────────────────────────────────────────────────────────────┐
│ Übersicht  Workspaces  Updates  KI  Protokoll  Einstellungen       │  Tabs, aktiver in focus
├────────────────────────────────────────────────────────────────────┤
│ CPU        23 %   ▁▂▂▃▅▃▂▁▁▂… (60 Werte = 2 min)                    │  Summen + Sparklines
│ Speicher   65 %   ▅▅▅▆▆▆▅▅▅▅…   10,5 GB / 16,0 GB                   │
│ Prozesse  612                                                      │
│                                                                    │
│ ◆ 3 Repos zurück   ▲ 2 übersprungen   ✖ 1 divergiert               │  nur Abweichungen
│ ◆ 11 Updates (Homebrew 1, npm (global) 10)                         │
│                                                                    │
│ KI heute   1,2 M Token   ≈ 4,10 $   API-Äquivalent                 │
│                                                                    │
├────────────────────────────────────────────────────────────────────┤
│ ⠋ npm install -g wrangler@4.151.1                    seit 00:12    │  laufende Aktion
│ ? Hilfe  Tab Wechseln  ↵ Aktion  r Aktualisieren  q Beenden        │  klickbare Fußzeile
└────────────────────────────────────────────────────────────────────┘
```

- **Linksbündig**, eine Spalte für Text, Zahlen rechtsbündig in eigenen Spalten.
- **Rahmen** in `muted`, nur der aktive Bereich in `focus`.
- **Fußzeile** zeigt kontextabhängige Aktionen; jedes Element ist per Maus klickbar und hat ein Tastenkürzel.
- **Übersicht zeigt nur Abweichungen** (Dark Cockpit). Ist alles normal, steht dort ein einziger neutraler Satz, z. B. „Alles aktuell. Letzte Prüfung vor 4 min.“ Repos zählen als „zurück“ (auch bei Opt-out), „übersprungen“ (Regel blockiert), „divergiert“ oder „Fehler“; nicht verfügbare Workspaces erscheinen eigens. Updates zählen ohne gepinnte Pakete.
- Die Sparklines sind rechtsbündig: der neueste Wert steht immer an derselben Stelle. CPU oder Speicher ab 90 % erscheinen in `caution`.

### 5.1 Tastenbelegung

| Taste | Aktion |
|---|---|
| `Tab` / `Shift-Tab` | nächster / vorheriger Tab |
| `↑` `↓` | Auswahl bewegen; am Listenende geht es oben weiter und umgekehrt |
| `Enter` | ausgewählte Aktion ausführen |
| `f` | Fast-Forward für das ausgewählte Repo (Workspaces) |
| `u` | ausgewähltes Paket aktualisieren |
| `←` `→`, `1`–`5` | im KI-Tab: Zeitraum wechseln |
| `t` | im KI-Tab: Telemetrie in Claude Code einschalten (mit Rückfrage) |
| `/` | im Protokoll suchen |
| `f` | im Protokoll: Filter wechseln (sonst Fast-Forward) |
| `a` | Workspace hinzufügen (Einstellungen) |
| `d` | ausgewählten Workspace entfernen (Einstellungen, mit Rückfrage) |
| `r` | jetzt aktualisieren; im Einstellungen-Tab: Config neu laden |
| `?` | Hilfe |
| `q`, `Ctrl-C` | beenden; läuft eine Installation, fragt ein Dialog: warten (`Enter`, Default), abbrechen (`a`) oder zurück (`Esc`) |

### 5.2 Updates-Tab

```
Homebrew   7.0.9                                                       Quelle und Werkzeugversion
▌ ◆ wget                  1.21.4 → 1.24.5                              Auswahl in focus
  ▲ firefox               118.0 → 119.0          Major-Update
  ○ node@20               20.11.0 → 20.18.1      gepinnt
  ⠋ wrangler              4.81.0 → 4.151.1       läuft                 Spinner statt Statussymbol
Skills   1.7.2
  72 Skills installiert, Updates unbekannt
  ◆ Alle Skills aktualisieren
```

- Je Quelle ein Kopf mit Werkzeugversion; fehlt das Werkzeug oder ist es defekt, steht das an dieser Stelle statt der Pakete.
- Je Paket Statussymbol, Name, `aktuell → neu` und eine Notiz: Zustand der Installation, „Major-Update“ oder „gepinnt“. Gepinnte Pakete sind nicht installierbar.
- Skills meldet keine einzelnen Updates; der Tab zeigt die Zahl installierter Skills und eine Zeile „Alle Skills aktualisieren“.
- `u` oder `Enter` öffnet einen zentrierten Bestätigungsdialog in `focus` mit dem genauen Befehl. Bei einem Major-Update warnt eine Zeile in `advisory`. `Enter` installiert, `Esc` oder `n` bricht ab; solange der Dialog offen ist, gehen alle Tasten an ihn.
- Laufende Installationen erscheinen auf allen Tabs in der Fortschrittszeile der Fußzeile, vor Scan und Sync.

### 5.3 Einstellungen-Tab

```
Darstellung
▌ Sprache                       Automatisch (Deutsch)      ermittelter Wert in soft
  Theme                         Automatisch (Dunkel)
  Symbole                       Unicode (empfohlen)
Workspaces                      Automatischer Fast-Forward
  /Users/nimble/workspace       ● an
  /Volumes/1TB EXT/workspace    ○ aus                      aus in muted
  + Workspace hinzufügen
Automatische Installation
  Homebrew-Formeln              ○ aus
  Homebrew-Casks                ○ aus
  npm global (nie über Major-Versionen)  ○ aus
```

- Drei Abschnitte, je ein fetter Kopf in `ink`; Werte in einer eigenen Spalte.
- `Enter`, Leertaste oder `→` ändern die Zeile: Auswahlen wechseln zum nächsten Wert, `←` zum vorherigen; Schalter kippen. Die Anzeige zeigt immer den gespeicherten Stand, nicht eine lokale Vorschau.
- `a` oder `Enter` auf „Workspace hinzufügen“ öffnet eine Eingabezeile in `focus` mit Cursor `█`; Pfade beginnen mit `/` oder `~/`. Solange die Eingabe offen ist, gehen alle Tasten an sie.
- `d` öffnet einen zentrierten Dialog „Workspace entfernen?“ mit dem Hinweis, dass nur die Config geändert wird.
- Sprache, Theme und Symbolsatz gelten sofort. Ein abgelehnter Schreibversuch erscheint unter den Einstellungen in `warning`, ein ungültiger Pfad in `caution`.

### 5.5 KI-Tab

```
Heute   7 Tage   30 Tage   Dieser Monat   Letzter Monat     gilt für den ganzen Tab

Claude Code   474 Logdateien in 239 ms   Preise Stand 2026-10-17
Telemetrie    ● empfängt auf 127.0.0.1:4318, Claude Code sendet

Verbrauch
  Token               192,9 M   Input 99,1 k   Output 230,7 k   Cache schr. 12,4 M   Cache lesen 180,2 M
  Kosten              ≈ 137,67 $      API-Äquivalent
  Cache-Hit           97 %

Modelle
  ████████████████████████████████████████████▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒░        Balken 60 Zellen
  ■ Opus 5.5 71 %      ■ Sonnet 5.5 28 %      ■ Haiku 4.5 <1 %
  Modell                     Antworten   Input   Output   Cache schr.   Cache lesen      Kosten

Aufwand
  Subagenten          31 % der neuen Token      Thinking 37 % vom Output

Aktivität
  ▁▁▂▂▃▃▅▅▂▂ …                                                       Stunden (heute) oder Tage
  0 Uhr                                      24 Uhr
  Gesamt              5 Sessions, 1.751 Antworten, 837 Tool-Calls

Kontext   aktuell
  ███████░░░░░░░░░░░░░░░░░░░░░░░░░░░░   12 %   121,0 k / 1,0 M
  opus-5-5, macos-hud, vor 6 min

Projekte
  macos-hud 34 %      takt 14 %      skills-hub 7 %

Telemetrie   nur solange hud läuft
  Aktive Zeit, Code, Kosten laut Claude
```

- Die Zeitraumauswahl steht ganz oben und gilt für alle Abschnitte; jede Kennzahl zeigt genau einen Wert. Nur „Kontext“ zeigt den aktuellen Stand und trägt den Hinweis „aktuell“.
- Abschnitte haben eine fette Überschrift und eine Leerzeile davor; der Tab nutzt die Höhe. Token und Kosten stehen nur unter „Verbrauch“ und in der Modelltabelle, nie doppelt.
- „Neue Token“ sind alle außer Cache-Lesen. Modell- und Projektanteile beziehen sich darauf, Thinking auf den Output, Cache-Hit auf alle Prompt-Token.
- Balken nutzen `█` und `░`; die drei größten Modelle bekommen `advisory`, `focus` und `caution`, der Rest `muted`. Ab 80 % Kontext wird der Kontextbalken `caution`.
- Die Aktivität zeigt bei „Heute“ die 24 Stunden, sonst die Tage des Zeitraums, mit Beschriftung von Anfang und Ende.
- Kennzeichnung nach Genauigkeit: `≈` geschätzt, `~` unvollständig, `—` nicht verfügbar. Kosten tragen „API-Äquivalent“ oder „Schätzung“ und das Stand-Datum der Preistabelle.
- Zahlen kompakt mit einer Nachkommastelle (`192,9 M`, englisch `192.9M`), Beträge auf Cent gerundet, Tausender gruppiert.
- Werte aus der Telemetrie erscheinen, sobald Claude Code sendet oder Daten vorliegen, und tragen immer `~`. `t` schaltet die Telemetrie in Claude Code ein (Dialog mit den genauen Einträgen für `settings.json`).

### 5.4 Protokoll-Tab

```
Filter: alle    Suche: wget█
  Zeit              Aktion        Auslöser  Ergebnis           Ziel
▌ 2026-10-17 12:41  Installation  manuell   ✖ fehlgeschlagen   Npm wrangler 4.81.0 → 4.151.1
  2026-10-17 12:15  Fast-Forward  auto      ● erledigt         /Users/nimble/workspace/projects/hud
────────────────────────────────────────────────────────────────
Ziel            Npm wrangler 4.81.0 → 4.151.1
Ergebnis        fehlgeschlagen   Dauer 14 s
Ausgabe-Ende    npm error code EACCES …
```

- Neueste Aktion oben, Zeiten in der lokalen Zeitzone. Die Ergebnisspalte zeigt Statussymbol und Ergebnis in der UI-Sprache.
- Der Detailbereich unten zeigt die ausgewählte Aktion: vollständiges Ziel, Ergebnis mit Dauer, die technische Zusammenfassung und das Ende der Ausgabe (so viele letzte Zeilen, wie Platz haben).
- `f` wechselt den Filter: alle, automatisch, manuell, Probleme (alles außer „erledigt“). `/` öffnet die Suche, die beim Tippen filtert und Ziel, Zusammenfassung und Ausgabe durchsucht; `Enter` behält den Suchtext, `Esc` verwirft ihn.

## 6. Bewegung

- Einzige Dauerbewegung: Braille-Spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` für laufende Aktionen (ein Frame pro Render-Tick, 250 ms).
- Läuft ein Scan, zeigt die Fußzeile auf allen Tabs eine zusätzliche Zeile: Spinner, Art des Laufs („Lokaler Scan“ oder „Fetch und Sync“), Balken aus `█`/`░` (20 Zellen) und `erledigt / gesamt`. Repos erscheinen einzeln, sobald ihr Status gelesen ist; leere Workspaces zeigen währenddessen „Suche Repositories …“.
- Sparklines rollen pro Messintervall (2 s) um einen Wert weiter, ohne Übergang.
- Kein Blinken, keine Farbanimation. Ein erledigter Status bleibt 30 s grün und kehrt dann in den Normalzustand zurück.

## 7. Sprache und Ton

- UI zweisprachig **Deutsch/Englisch**, umschaltbar über `language = "auto" | "de" | "en"`. `auto` folgt der bevorzugten Systemsprache von macOS, Fallback Englisch.
- Satzanfang groß, sonst Kleinschreibung, keine Großbuchstaben-Labels.
- Aktionen heißen nach dem, was passiert: „Fast-Forward ausführen“, „Aktualisieren“. Die Bestätigung verwendet dasselbe Verb: „Fast-Forward ausgeführt“.
- Fehler nennen Ursache und nächsten Schritt, ohne Entschuldigung: „npm nicht gefunden. PATH der Login-Shell prüfen oder `npm_path` in der Config setzen.“
- Geschätzte Werte tragen immer `≈` und das Wort „Schätzung“ oder „API-Äquivalent“.

## 8. Wortmarke und README

- Wortmarke: `hud` in Kleinbuchstaben, Chakra Petch 600, mit einer magentafarbenen Kurslinie als Unterstrich. Datei: `docs/assets/wordmark.svg`.
- Logo: Flight-Path-Vector-Symbol (Kreis mit Flügeln) in `focus` auf `panel` dunkel, mit Horizontlinie in `soft`. Datei: `docs/assets/logo.svg`.
- README (Englisch): zentrierter Kopf mit Logo, Claim, Badges und Navigation → Showcase der Übersicht hell/dunkel (`docs/assets/showcase-{light,dark}.svg`) → Why hud → Features → Statussymbole → Safety → Install → First run → Keys → Configuration → Data and privacy → Architecture → Documentation.
- Alle Grafiken für die README enthalten Text als Pfade (Chakra Petch, IBM Plex Mono), damit GitHub sie ohne Webfonts korrekt darstellt. Erzeugt werden sie mit `uv run scripts/gen-readme-assets.py`.

## 9. Diagramme

- Alle Diagramme nutzen das diagram-design-Profil **`hud`** (`~/.diagram-design/profiles/hud.md`, Projekt-Marker `.diagram-design`).
- Quelle: `docs/diagrams/<name>.html`, eingebettet als exportiertes `docs/diagrams/<name>.svg`.
- Statusfarben in Diagrammen nur für Ergebnis-Markierungen, immer mit Symbol und Text.

## 10. Barrierefreiheit

- Volle Tastaturbedienung, Maus optional.
- Kein Status nur über Farbe (siehe 3.2).
- `NO_COLOR` und 16-Farben-Terminals voll unterstützt.
- Kontraste ≥ 4.5:1 in hell und dunkel.
