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
│ CPU  23 %  ▁▂▂▃▅▃▂▁▁▂   RAM 61 %  ▅▅▅▆▆▆▅▅▅▅   Prozesse 512        │  Summen + Sparklines
│                                                                    │
│ ◆ 3 Repos zurück   ▲ 2 übersprungen   ✖ 1 divergiert               │  nur Abweichungen
│ ◆ 11 Updates (brew 1 · npm 10)                                     │
│ KI heute  1,2 M Token  ≈ 4,10 $ API-Äquivalent                     │
│                                                                    │
├────────────────────────────────────────────────────────────────────┤
│ ⠋ npm install -g wrangler@4.151.1                    seit 00:12    │  laufende Aktion
│ ? Hilfe  Tab Wechseln  ↵ Aktion  r Aktualisieren  q Beenden        │  klickbare Fußzeile
└────────────────────────────────────────────────────────────────────┘
```

- **Linksbündig**, eine Spalte für Text, Zahlen rechtsbündig in eigenen Spalten.
- **Rahmen** in `muted`, nur der aktive Bereich in `focus`.
- **Fußzeile** zeigt kontextabhängige Aktionen; jedes Element ist per Maus klickbar und hat ein Tastenkürzel.
- **Übersicht zeigt nur Abweichungen** (Dark Cockpit). Ist alles normal, steht dort ein einziger neutraler Satz, z. B. „Alles aktuell. Letzte Prüfung vor 4 min.“

### 5.1 Tastenbelegung

| Taste | Aktion |
|---|---|
| `Tab` / `Shift-Tab` | nächster / vorheriger Tab |
| `↑` `↓` | Auswahl bewegen |
| `Enter` | ausgewählte Aktion ausführen |
| `f` | Fast-Forward für das ausgewählte Repo |
| `u` | ausgewähltes Paket aktualisieren |
| `r` | jetzt aktualisieren |
| `?` | Hilfe |
| `q` | beenden (fragt nach, wenn noch etwas läuft) |

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
