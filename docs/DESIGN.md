# Product Design — hud

**Version:** 0.7.0 · **Status:** zur Prüfung · **Stand:** 2026-10-18

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
| `rule` | `#C4C8C2` | `#3A424D` | Trennlinien, leere Balkenzellen, Kalenderstufe 0 |

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
- Pro Listenzeile höchstens ein Statussymbol; Sammelzeilen der Übersicht zeigen je Kategorie ein Symbol.
- Ein Repo zeigt genau einen Status: den ersten zutreffenden Grund in der festen Regelreihenfolge (laufende Git-Operation, detached HEAD, keine Commits, kein Upstream, lokale Änderungen, ahead/divergiert). Ein divergiertes Repo mit lokalen Änderungen zeigt deshalb ▲ „lokale Änderungen“.

### 3.3 Terminal-Abbildung

| Modus | Erkennung | Abbildung |
|---|---|---|
| Truecolor | `COLORTERM=truecolor\|24bit` | Hex-Werte aus 3.1/3.2, hell oder dunkel nach Theme |
| 256 Farben | `TERM` enthält `256color` | nächstliegende xterm-256-Farbe (beim Start berechnet) |
| 16 Farben | sonst | `focus`→Magenta, `advisory`→Cyan, `caution`→Yellow, `warning`→Red, `ok`→Green, `ink`→Standardvordergrund, `muted`/`soft`/`rule`→BrightBlack; Kalenderstufen: KI BrightBlack→Blue→Cyan→LightCyan, Git BrightBlack→Green→LightGreen |
| Keine Farbe | `NO_COLOR` gesetzt | nur Symbole, Fett und Unterstreichung |

**Theme-Erkennung:** `theme = "auto" | "dark" | "light"` in der Config, Default `auto`. hud fragt beim Start den Terminal-Hintergrund per OSC 11 ab (nur wenn die Ausgabe ein Terminal ist, Timeout 100 ms, Fallback `dark`); verwendet wird er bei `auto`. hud malt **keinen** eigenen Hintergrund: Es nutzt den Hintergrund des Terminals und färbt nur Vordergrund und Rahmen.

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
| CPU / Speicher / Prozesse | — | `nf-oct-cpu` / `nf-md-memory` / `nf-fa-tasks` vor der Beschriftung in der Übersicht | — |

- Quell-Icons (Git, Homebrew, npm, Skills, KI) erscheinen nur im Satz `nerd`; in `unicode` und `ascii` steht dort der Name der Quelle.
- Der Einrichtungsdialog (Schritt 3) zeigt alle drei Sätze untereinander und fragt, in welcher Zeile alle Symbole korrekt erscheinen; die gewählte Zeile setzt `icons`.
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
  Übersicht    Workspaces    Updates    KI    Protokoll    Einstellungen   hud 0.11.0   Tabs, aktiver in focus, fett, unterstrichen; Version rechts in soft
  ─────────────────────────────────────────────────────────────────────          Trennlinie in rule

  CPU          26 %     ▁▁▂▂▃▃▄▄▅▁▁▂▂▃▃▄▄▅…(60 Werte = 2 min)                       Summen + Sparklines
  Speicher     67 %     ▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅…   10,7 GB / 16,0 GB
  Prozesse    512

  ◆ 3 Repos zurück   ▲ 1 ungepusht   ▲ 2 übersprungen   ✖ 1 divergiert            nur Abweichungen
  ◆ 11 Updates (Homebrew 1, npm (global) 10), 2 Major, 1 braucht ein Passwort
  ▲ 2 Probleme in 24 h (Protokoll)
  ◆ Codex-Limit 5 Stunden: 86 %, neu ab 14:14
  ▲ Macintosh HD: nur 8,0 GB frei

  KI heute    192,9 M Token   ≈ 137,67 $   API-Äquivalent

  KI-Aktivität   letzte 53 Wochen                                                  Kalender, darunter Git-Aktivität
  ─────────────────────────────────────────────────────────────────────
  ⠋ 1 Installation läuft   npm install --global wrangler@4.151.1                   laufende Aktion
  r aktualisieren   tab wechseln   ? Hilfe   q beenden                             klickbare Fußzeile
```

- Rechts in der Tab-Leiste steht die laufende Version („hud 0.11.0“) in `soft`. Hat hud sich während des Laufs selbst aktualisiert, steht dort „hud 0.11.0 → 0.12.0, neu starten“ in `advisory`, denn die neue Version startet erst mit dem Neustart. Reicht die Breite nicht für Tabs und Text, entfällt der Text.
- Kein äußerer Rahmen, links und rechts zwei Spalten Rand. Trennlinien in `rule` unter den Tabs und über der Fußzeile.
- **Linksbündig**, eine Spalte für Text, Zahlen rechtsbündig in eigenen Spalten.
- **Rahmen** nur für Dialoge und die Hilfe, in `focus`; Bereiche haben keine Rahmen.
- **Fußzeile** zeigt kontextabhängige Aktionen; jede Aktion hat ein Tastenkürzel und ist per Maus klickbar. Hinweise, die nur Tasten erklären (z. B. ↑↓), reagieren nicht auf Klicks.
- **Maus**: Tab-Beschriftungen, Zeitraumauswahl und Listenzeilen sind klickbar; ein zweiter Klick auf die gewählte Zeile wirkt wie Enter (Workspaces: Fast-Forward, Updates: Bestätigungsdialog, Protokoll: Details vergrößern, Einstellungen: Wert ändern). Das Mausrad bewegt die Auswahl. Solange ein Dialog offen ist, werden Klicks ignoriert; ein Klick schließt die Hilfe.
- **Fensterbreite**: Spalten mit Namen (Modelle, Pakete, Repositories) wachsen mit dem Fenster und werden in schmalen Fenstern gekürzt (`…`); Zahlenspalten bleiben fest. Wird es eng, entfallen nachrangige Spalten: in der Modelltabelle zuerst Cache schreiben bzw. Reasoning, dann Antworten, Input und Cache lesen (Output und Kosten bleiben); im Protokoll unter 95 Spalten der Auslöser, unter 78 die Aktion. Lange Zeilen wie die Token-Zeile brechen unter die Werte um; Zustands- und Job-Texte enden mit `…`, wenn sie nicht passen. Die Sparklines zeigen in schmalen Fenstern nur die jüngsten Messungen, die Einstellungen verkleinern die Label-Spalte, die Hilfe nimmt höchstens die Fensterbreite.
- **Scrollen**: Workspaces, Updates, Protokoll und Einstellungen halten die Auswahl im sichtbaren Bereich. Übersicht und KI-Tab blättern frei mit `↑` `↓`, Bild↑/Bild↓, Pos1/Ende und dem Mausrad, nie über das Ende des Inhalts hinaus; im KI-Tab bleibt die Zeitraumleiste oben stehen. Passt der Inhalt nicht, zeigt ein Scrollbalken im rechten Rand (Spur `│` in `rule`, Griff `┃` in `muted`) die Position, und die Fußzeile nennt „↑↓ blättern“.
- **Übersicht zeigt nur Abweichungen** (Dark Cockpit). Ist alles normal, steht dort ein einziger neutraler Satz, z. B. „Alles aktuell. Letzte Prüfung vor 4 min.“ Repos zählen als „zurück“ (auch bei Opt-out), „ungepusht“ (nur eigene Commits, kein Upstream-Rückstand), „übersprungen“ (andere Regel blockiert), „divergiert“ oder „Fehler“; nicht verfügbare Workspaces erscheinen eigens. Updates zählen ohne gepinnte Pakete; dahinter stehen, falls vorhanden, Major-Updates und Pakete, die laut Updates-Tab ein Passwort brauchen. Darunter in `caution` die Zahl der Protokolleinträge der letzten 24 Stunden, die nicht erfolgreich endeten (wie der Filter „Probleme“, ohne laufende). Je laufendes Abo-Limit-Fenster ab 80 % eine Zeile mit Prozent und Rücksetzzeitpunkt, ab 90 % in `caution` wie im KI-Tab. Hat das Volume der Systemplatte oder eines Workspaces weniger als 10 % frei, steht es mit Namen und freiem Platz in `caution` da; jedes Volume nur einmal.
- **Alles aktualisieren** (`a`): Der Bestätigungsdialog „Alles aktualisieren?“ nennt zuerst die Zahl der Fast-Forwards, dann die Pakete je Quelle und warnt wie der Dialog „alle“ vor Major-Updates. Während des Laufs zeigt die Fortschrittszeile auf allen Tabs „Alles aktualisieren ███░░ 3 / 12“ (vor laufenden Installationen), danach 5 s „● Alles aktualisiert: N erledigt“ in `ok` bzw. mit „, N Probleme (Protokoll)“ in `caution`. Ohne Repos und Pakete öffnet `a` nichts; während eines Laufs startet `a` keinen zweiten.
- Die Sparklines sind rechtsbündig: der neueste Wert steht immer an derselben Stelle. CPU oder Speicher ab 90 % erscheinen in `caution`.
- Darunter die **KI-Aktivität** im Stil des GitHub-Contribution-Graphs: Kopf „KI-Aktivität   letzte N Wochen“, eine Spalte je Woche (so viele, wie in die Breite passen, höchstens 53), sieben Zeilen Mo bis So, beschriftet Mo, Mi, Fr links und noch einmal rechts nach der letzten Woche (die rechte Beschriftung zählt bei der Breite mit); Monatskürzel über der Woche, in der ein Monat beginnt. Mit Farbe besteht eine Zelle aus `■` und Leerzeichen, fünf Stufen von `rule` bis `advisory` (0 und die Quartile der aktiven Tage); ohne Farben (`NO_COLOR`) zeigen `·░▒▓█` die Stufe. In der letzten Zeile Antworten, aktive Tage und längste Serie, rechts daneben die Legende „weniger ■■■■■ mehr“. Ohne jede Aktivität entfällt der Kalender.
- Darunter im selben Aufbau die **Git-Aktivität**: eigene Commits über alle getrackten Repositories, Farbverlauf von `rule` zu `ok`, damit beide Kalender auf einen Blick unterscheidbar sind.

### 5.1 Tastenbelegung

| Taste | Aktion |
|---|---|
| `Tab` / `Shift-Tab` | nächster / vorheriger Tab |
| `↑` `↓`, `k` `j` | Auswahl bewegen; am Listenende geht es oben weiter und umgekehrt. In Übersicht und KI-Tab: blättern |
| Bild↑ Bild↓, Pos1 Ende | Übersicht und KI-Tab: seitenweise bzw. an Anfang oder Ende blättern |
| `Enter` | Workspaces: auf einem Workspace-Kopf ein- oder ausklappen, auf einem Repo Fast-Forward wie `f`; Updates: Bestätigungsdialog öffnen; Protokoll: Details vergrößern oder verkleinern; Einstellungen: Wert ändern; in Dialogen: bestätigen |
| `f` | Workspaces: Fast-Forward für das ausgewählte Repo; Protokoll: Filter wechseln |
| `u` | Updates: ausgewähltes Paket aktualisieren |
| `v` | Workspaces: zwischen „nur Abweichungen“ (Standard) und allen Repos umschalten |
| `s` | Workspaces: zwischen Sortierung nach Name (Standard) und nach Status umschalten |
| `←` `→`, `h` `l` | Workspaces: Workspace oder Repo mit Unterrepos einklappen bzw. ausklappen; `←` auf einer Zeile ohne offene Gruppe springt zum Eltern-Repo bzw. Workspace-Kopf |
| `←` `→`, `h` `l`, `1`–`5` | KI: Zeitraum wechseln (läuft am Ende um) |
| `t` | KI: Telemetrie in Claude Code einschalten (mit Rückfrage) |
| `/` / `Esc` | Protokoll, Workspaces: Suche öffnen / Suche verwerfen |
| `Enter`, Leertaste, `←` `→` | Einstellungen: Wert ändern bzw. Schalter kippen |
| `a` | Übersicht: alles aktualisieren, also zurückliegende Repos per Fast-Forward und alle Updates (mit Rückfrage); Updates: alle verfügbaren Updates installieren (mit Rückfrage); Einstellungen: Workspace hinzufügen |
| `d`, `Entf` | Einstellungen: ausgewählten Workspace entfernen (mit Rückfrage) |
| `r` | Übersicht, Workspaces, KI: Fetch und Sync starten; Updates: Quellen prüfen; Protokoll: neu laden; Einstellungen: Config neu laden |
| `?` | Hilfe-Overlay mit den Tasten des aktuellen Tabs und den globalen Tasten; jede Taste schließt es |
| `n`, `Esc` | in Dialogen: abbrechen bzw. zurück |
| `q`, `Ctrl-C` | beenden; läuft eine Installation, fragt ein Dialog: warten (`Enter` oder `w`, Default), abbrechen (`a`) oder zurück (`Esc` oder `n`). Ein zweites `q`/`Ctrl-C` beendet sofort, die Fußzeile zeigt dann „q sofort beenden“. `Ctrl-C` wirkt auch aus Eingaben heraus |

### 5.2 Workspaces-Tab

```
 ▾ /Users/nimble/workspace/projects   64 Repos   ◆ 2  ✖ 1
▌ ◆   hud                        main              ↓3       3 Commits zurück, Fast-Forward möglich
  ✖   takt                       feature/x         ↑1 ↓2    divergiert
  ○ ▸ NETZbau                    main                       50 Unterrepos   ◆ 1
  ○ 11 weitere aktuell
 ▸ /Users/nimble/oss   8 Repos   alles aktuell
   /Volumes/extern   ✖ nicht verfügbar
```

- Je Workspace ein Kopf: Auswahlmarker, Klapp-Symbol `▾`/`▸` in `soft` (ASCII `v`/`>`), Pfad in `ink`, fett (ausgewählt in `focus`); dahinter die Zahl der Repos in `soft` und je abweichendem Status Symbol und Anzahl in der Statusfarbe (ohne Abweichung „alles aktuell“ in `soft`), danach gegebenenfalls „automatischer Fast-Forward aus“ in `soft`. Ein nicht verfügbarer Workspace zeigt nur „✖ nicht verfügbar“ in `warning`.
- Je Repo: Auswahlmarker `▌` in `focus`, Statussymbol, Name relativ zum Workspace (ausgewählt in `focus`, fett; ein in einem anderen Repo liegendes Repo steht direkt darunter, je Ebene um zwei Spalten eingerückt, mit dem Pfad ab dem äußeren Repo), Branch (oder „(detached)“), `↑a ↓b` und Zustandstext in der Statusfarbe; aktuelle Repos mit Namen in `muted` und Details in `soft`.
- Zurückliegende Repos zeigen „N Commits zurück, Fast-Forward möglich“ (◆), bei Opt-out „zurück, automatischer Fast-Forward aus (f)“ (▲); ein erledigter Fast-Forward „Fast-Forward: N Commits“ (●, 30 s).
- **Nur Abweichungen** (Standard): Aktuelle Repos stehen je Workspace in einer Zeile „○ N weitere aktuell“ in `soft` (ohne Abweichung „○ alle N Repos aktuell“). Ein aktuelles Repo bleibt sichtbar, wenn es ein abweichendes Unterrepo enthält, ein frischer Fast-Forward (●) für 30 s. `v` schaltet auf alle Repos um und zurück; die Fußzeile nennt die jeweils andere Ansicht.
- **Einklappen**: Workspace-Köpfe und Repos mit Unterrepos tragen `▾` (offen) bzw. `▸` (eingeklappt); jeder Repo-Name hält dafür zwei Spalten frei. Ein eingeklapptes Repo zeigt statt der Unterrepos „N Unterrepos“ in `soft` und je abweichendem Status Symbol und Anzahl; den eigenen Zustand nur, wenn er abweicht, gekürzt, damit die Summe passt. In „nur Abweichungen“ starten Workspaces ohne Abweichung eingeklappt, in „alle“ ist alles offen; eigene Klapp-Entscheidungen gelten für die Sitzung.
- **Sortierung**: nach Name (Standard) oder mit `s` nach Status: Fehler, divergiert, übersprungen, zurück, frischer Fast-Forward, aktuell; bei gleichem Rang nach Name. Sortiert wird unter Geschwistern, Unterrepos bleiben unter ihrem Repo. Die Wahl gilt für die Sitzung.
- **Suche**: `/` öffnet über der Liste „Suche: Text█“ wie im Protokoll-Tab; solange die Eingabe offen ist, gehen alle Tasten an sie, die Fußzeile zeigt „Enter fertig   Esc abbrechen“. Gefiltert wird ohne Groß-/Kleinschreibung nach Name (Pfad relativ zum Workspace) und Branch. Sichtbar sind die Treffer mit ihren Eltern-Repos, alle Gruppen offen, ohne Sammelzeile; Workspaces ohne Treffer stehen eingeklappt da. `Enter` schließt die Eingabe und behält den Filter, `Esc` verwirft ihn.
- Die Auswahl hängt am Workspace-Kopf oder Repo, nicht an der Zeile: Verschwinden Zeilen darüber, bleibt sie stehen; ist das gewählte Repo ausgeblendet, gilt das erste sichtbare.
- Die Liste scrollt so, dass die Auswahl sichtbar bleibt; leere Workspaces zeigen während eines Scans „Suche Repositories …“.

### 5.3 Updates-Tab

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

- Je Quelle ein Kopf mit Werkzeugversion; fehlt das Werkzeug, steht dort „✖ nicht gefunden“ und darunter in `muted`, wo hud gesucht hat und welcher Schlüssel unter `[tools]` hilft (Skills: `npx`, gesucht neben `npm`); ist es defekt, „✖ Fehler: …“. Scheitert die Prüfung, steht darunter eine Zeile „✖ Fehler: …“. Ohne Updates zeigt die Quelle „○ alles aktuell“. Zwischen den Quellen steht eine Leerzeile.
- Je Paket Statussymbol, Name, `aktuell → neu` und eine Notiz: Zustand der Installation, „Major-Update“ oder „gepinnt“. Gepinnte Pakete sind nicht installierbar.
- Installationszustände: „wartet“, „läuft“, „installiert“ (●), „braucht ein Passwort, im Terminal ausführen“ (▲), „abgebrochen“ (▲), „nicht erlaubt“, „fehlgeschlagen: …“, „Zeitüberschreitung“ (✖). „installiert“ bleibt bis zur nächsten Prüfung stehen; bei der Homebrew-Formel `hud` selbst heißt es „installiert, hud neu starten“.
- Skills meldet keine einzelnen Updates; der Tab zeigt die Zahl installierter Skills und eine Zeile „Alle Skills aktualisieren“.
- `u` oder `Enter` öffnet einen zentrierten Bestätigungsdialog in `focus` mit dem genauen Befehl. Bei einem Major-Update warnt eine Zeile in `caution` (▲ Major-Update: kann inkompatible Änderungen enthalten.). `Enter` installiert, `Esc` oder `n` bricht ab; solange der Dialog offen ist, gehen alle Tasten an ihn.
- `a` öffnet denselben Dialog für alle Zeilen, die sich jetzt installieren lassen (ohne gepinnte und bereits laufende): Titel „Alle Updates installieren?“, je Quelle Name und Anzahl („Homebrew 3 Pakete“, bei Skills „alle Skills“), darunter bei Major-Updates „▲ Major-Updates: <Namen>“ in `caution`. `Enter` startet für jedes Paket einen eigenen Job mit eigener Permit-Prüfung; gibt es nichts zu installieren, öffnet sich kein Dialog.
- `r` prüft die Quellen sofort.
- Laufende Installationen erscheinen auf allen Tabs in der Fortschrittszeile der Fußzeile, vor Scan und Sync.
- Nach `f` (oder `Enter`) im Workspaces-Tab zeigt die Fortschrittszeile gleich nach den Installationen „Fast-Forward <Repo> …“ und nach dem Ergebnis 5 s lang eine Rückmeldung mit Statussymbol: „<Repo>: bereits aktuell, nichts zu tun“ (●), „<Repo>: Fast-Forward: N Commits“ (●), „<Repo>: nicht aktualisiert, <Grund>“ (▲) oder die Fehlermeldung (✖).
- Nach `r` im Updates-Tab zeigt die Fortschrittszeile „Update-Prüfung läuft…“, bis die Ergebnisse da sind; ein weiteres `r` startet währenddessen keine zweite Prüfung.

### 5.4 KI-Tab

```
Heute   7 Tage   30 Tage   Dieser Monat   Letzter Monat     gilt für den ganzen Tab

Claude Code   474 Logdateien in 239 ms   Preise Stand 2026-10-17
Telemetrie    ● empfängt auf 127.0.0.1:4318, Claude Code sendet

Verbrauch
  Token               192,9 M   Input 99,1 k   Output 230,7 k   Cache schr. 12,4 M   Cache lesen 180,2 M
  Kosten              ≈ 137,67 $      API-Äquivalent
  Cache-Hit           97 %

Modelle
  ████████████████████████████████████████████████████████████        Balkenbreite folgt dem Fenster, je Modell eine Farbe
  ■ Opus 5.5 71 %      ■ Sonnet 5.5 28 %      ■ Haiku 4.5 <1 %
  Modell                          Antworten        Input       Output  Cache schr.  Cache lesen         Kosten

Aufwand
  Subagenten          31 % der neuen Token      Thinking 37 % vom Output

Aktivität
  ▁▁▂▂▃▃▅▅▂▂ …                                                       Stunden (heute) oder Tage
  0 Uhr                                      24 Uhr
  Gesamt              5 Sessions, 1.751 Antworten, 837 Tool-Calls

Projekte
  macos-hud 34 %      takt 14 %      skills-hub 7 %

Telemetrie   nur solange hud läuft
  Aktive Zeit, Code, Kosten laut Claude


Codex   165 Logdateien in 84 ms   Preise Stand 2026-10-17

Verbrauch
  Token               10,0 M   Input 1,2 M   Output 40,0 k   Cache lesen 8,8 M   Reasoning 25,0 k
  …                                                                   Modelle und Aktivität wie oben
Aufwand
  Reasoning           39 % vom Output

Abo-Auslastung   Plan Plus, Stand vor 3 min
  5 Stunden           ███████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   13 %   neu ab 14:14
  7 Tage              █░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   3 %   neu ab 21.10. 12:00
```

- Die Zeitraumauswahl steht ganz oben und gilt für alle Abschnitte; jede Kennzahl zeigt genau einen Wert. Nur „Abo-Auslastung“ zeigt den aktuellen Stand.
- Reihenfolge der Abschnitte: Verbrauch, Modelle, Aufwand, Aktivität, Abo-Auslastung (Codex), Projekte, Telemetrie (Claude Code).
- Jede Quelle hat einen eigenen Block mit eigener Überschrift; zwischen den Blöcken stehen zwei Leerzeilen. Telemetrie gibt es nur bei Claude Code.
- Codex: Statt „Cache schr.“ steht „Reasoning“ in Verbrauch und Modelltabelle; „Aufwand“ zeigt nur den Reasoning-Anteil. „Abo-Auslastung“ zeigt je Limit-Fenster einen Balken mit Prozent und lokalem Rücksetzzeitpunkt (mit Datum, wenn nicht heute), ab 90 % in `caution`. Ist ein Fenster schon zurückgesetzt, steht dort nur, seit wann, bis Codex den neuen Stand meldet.
- Abschnitte haben eine fette Überschrift und eine Leerzeile davor. Token und Kosten stehen nur unter „Verbrauch“ und in der Modelltabelle, nie doppelt.
- „Neue Token“ sind alle außer Cache-Lesen. Modell- und Projektanteile beziehen sich darauf, Thinking und Reasoning auf den gesamten Output, Cache-Hit auf alle Prompt-Token.
- Alle Balken (Modelle, Kontext, Abo-Auslastung) und die Aktivitätskurve sind gleich breit: Fensterbreite minus 54 Spalten für Beschriftung und Text danach, mindestens 10, höchstens 80. Sie nutzen `█` und `░` (in `rule`); die drei größten Modelle bekommen `advisory`, `focus` und `caution`, der Rest `muted`. Die Legende zeigt höchstens 4 Modelle, „Projekte“ höchstens 5. Ab 80 % Kontext wird der Kontextbalken `caution`.
- Die Aktivität zeigt bei „Heute“ die 24 Stunden („0 Uhr … 24 Uhr“), sonst die Tage des Zeitraums („vor N T … heute“, beim letzten Monat „Monatsanfang … Monatsende“).
- Telemetrie-Zeile: „aus (Config)“, „✖ Port N nicht verfügbar: …“, „● empfängt auf 127.0.0.1:N, Claude Code sendet“, „◆ Claude Code sendet noch nicht“ mit Hinweis `t einschalten`, „▲ eigene OTel-Einstellungen in Claude Code (…), hud ändert nichts“ oder „✖ settings.json nicht lesbar: …“; nach dem Einschalten der Hinweis, laufende Claude-Code-Sitzungen neu zu starten.
- Unlesbare Logzeilen erscheinen am Ende der Quelle als „~ unvollständig: N Zeilen nicht lesbar“ in `caution`.
- Kennzeichnung nach Genauigkeit: `≈` geschätzt, `~` unvollständig, `—` nicht verfügbar. Kosten tragen in „Verbrauch“ „API-Äquivalent“ oder „Schätzung“ und das Stand-Datum der Preistabelle; Tabellenzellen nur `≈`.
- Zahlen kompakt mit einer Nachkommastelle (`192,9 M`, englisch `192.9M`; Milliarden `Mrd` bzw. `B`), Beträge auf Cent gerundet, Tausender gruppiert.
- Werte aus der Telemetrie erscheinen, sobald Claude Code sendet oder Daten vorliegen, und tragen immer `~`. `t` schaltet die Telemetrie in Claude Code ein (Dialog mit den genauen Einträgen für `settings.json`). Die KI-Daten werden alle 60 s neu gelesen.

### 5.5 Protokoll-Tab

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
- Der Detailbereich unten (Trennlinie in `rule`) zeigt die ausgewählte Aktion: vollständiges Ziel, Ergebnis mit Dauer, die technische Zusammenfassung und das Ende der Ausgabe (so viele letzte Zeilen, wie Platz haben). `Enter` vergrößert ihn bis auf fünf Listenzeilen und verkleinert ihn wieder; die Größe bleibt beim Wechsel der Auswahl.
- `f` wechselt den Filter: alle, automatisch, manuell, Probleme (alles außer „erledigt“). `/` öffnet die Suche, die beim Tippen filtert und Ziel, Zusammenfassung und Ausgabe durchsucht; `Enter` behält den Suchtext, `Esc` verwirft ihn.

### 5.6 Einstellungen-Tab

```
Darstellung
▌ Sprache                       Automatisch (Deutsch)      ermittelter Wert in soft
  Theme                         Automatisch (Dunkel)
  Symbole                       Unicode (empfohlen)
Workspaces                      Automatischer Fast-Forward
  /Users/nimble/workspace       ● an
  /Volumes/1TB EXT/workspace    ○ aus                      aus in muted
  + Workspace hinzufügen
  Suchtiefe                     3 Ebenen
Automatische Installation
  Homebrew-Formeln              ○ aus
  Homebrew-Casks                ○ aus
  npm global (nie über Major-Versionen)  ○ aus
KI-Quellen
  Claude Code                   ● an
  Codex                         ○ aus
Übersicht
  Ungepushte Commits            ● an
  Major-Updates und Passwort-Bedarf  ● an
  Probleme der letzten 24 h     ● an
  Abo-Limits ab 80 %            ● an
  Wenig Speicherplatz           ● an
```

- Fünf Abschnitte, je ein fetter Kopf in `ink`; Werte in einer eigenen Spalte.
- `Enter`, Leertaste oder `→` ändern die Zeile: Auswahlen wechseln zum nächsten Wert, `←` zum vorherigen; Schalter kippen. Die Anzeige zeigt immer den gespeicherten Stand, nicht eine lokale Vorschau.
- `a` oder `Enter` auf „Workspace hinzufügen“ öffnet eine Eingabezeile in `focus` mit Cursor `█`; Pfade beginnen mit `/` oder `~/`. Solange die Eingabe offen ist, gehen alle Tasten an sie.
- „Suchtiefe“ geht mit `Enter`/`→` eine Ebene tiefer, mit `←` eine flacher, zwischen 1 und 6 Ebenen; danach scannt hud sofort lokal neu.
- `d` oder `Entf` öffnet einen zentrierten Dialog „Workspace entfernen?“ mit dem Hinweis, dass nur die Config geändert wird.
- Sprache, Theme, Symbolsatz, KI-Quellen und die Zusatzanzeigen der Übersicht gelten sofort; eine abgeschaltete Zusatzanzeige verschwindet aus der Übersicht; eine abgeschaltete KI-Quelle verschwindet aus dem KI-Tab, ihre gespeicherten Daten bleiben. Ein abgelehnter Schreibversuch erscheint unter den Einstellungen in `warning`, ein ungültiger Pfad in `caution`.

### 5.7 Hilfe und Dialoge

```
┌ Tasten ──────────────────────────────────────────────────┐   Hilfe (?), Titel in focus
│  Protokoll                                               │
│  ↑↓                Auswahl bewegen, am Ende geht es …    │
│  /                 suchen, filtert beim Tippen           │
│  Überall                                                 │
│  Tab, Shift-Tab    nächster bzw. vorheriger Tab          │
│  q                 beenden, fragt bei laufender …        │
└──────────────────────────────────────────────────────────┘

┌ hud beenden? ────────────────────────────────────────────┐   Beenden während einer Installation
│  1 Installation läuft noch.                              │
│  ▲ Abbrechen kann ein Paket halb installiert …           │
│  Enter warten, dann beenden   a abbrechen und beenden   Esc zurück
└──────────────────────────────────────────────────────────┘

┌ Telemetrie in Claude Code einschalten? ──────────────────┐   t im KI-Tab
│  hud ergänzt in ~/.claude/settings.json unter "env":     │
│    "CLAUDE_CODE_ENABLE_TELEMETRY": "1"  …                │
│  Nur Metriken, keine Prompts oder Antworten. …           │
│  Enter eintragen   Esc abbrechen                         │
└──────────────────────────────────────────────────────────┘
```

- Dialoge sind zentriert, mit Rahmen und Titel in `focus`; solange einer offen ist, gehen alle Tasten an ihn, Klicks werden ignoriert.
- Die Hilfe zeigt zuerst die Tasten des aktuellen Tabs, dann „Überall“; unten „Beliebige Taste schließt die Hilfe.“

### 5.8 Einrichtungsdialog

```
  Willkommen bei hud
  ──────────────────────────────────────────────────
  Schritt 1 von 3: Workspaces
  Ordner, in denen hud nach Git-Repositories sucht. Leertaste wählt einen Vorschlag;
  eigene Pfade unten eintippen.

  ▌ [x] ~/workspace
    Pfad hinzufügen:
  ──────────────────────────────────────────────────
  ↑↓ bewegen   Leertaste auswählen   Enter weiter   Esc abbrechen
```

- Schritt 1: Vorschläge aus dem Home-Verzeichnis als `[x]`/`[ ]`, eigene Pfade in der Eingabezeile; mindestens ein Workspace ist nötig.
- Schritt 2 „KI-Quellen“: Claude Code und Codex als `[x]`/`[ ]`, Leertaste schaltet; Claude Code ist vorausgewählt, Codex nur, wenn es Logs gibt (sonst steht „noch keine Logs“ daneben). `Enter` weiter, `←`/Backspace zurück.
- Schritt 3 „Symbole“: drei Zeilen (Unicode, Nerd Font, ASCII) mit Statussymbolen und Quell-Icons; `↑↓` oder `1`–`3` wählt, `Enter` beendet, `←`/Backspace geht zurück.
- `Esc` oder `Ctrl-C` brechen ab, ohne etwas zu schreiben. Der Dialog wird nur per Tastatur bedient.

## 6. Bewegung

- Einzige Dauerbewegung: Braille-Spinner `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` für laufende Aktionen (ein Frame pro Render-Tick, 250 ms).
- Läuft ein Scan, zeigt die Fußzeile auf allen Tabs eine zusätzliche Zeile: Spinner, Art des Laufs („Lokaler Scan“ oder „Fetch und Sync“), Balken aus `█`/`░` (20 Zellen) und `erledigt / gesamt`. Repos erscheinen einzeln, sobald ihr Status gelesen ist; leere Workspaces zeigen währenddessen „Suche Repositories …“.
- Sparklines rollen pro Messintervall (2 s) um einen Wert weiter, ohne Übergang.
- Kein Blinken, keine Farbanimation. Ein Fast-Forward bleibt 30 s grün und kehrt dann in den Normalzustand zurück; eine Installation zeigt „installiert“ bis zur nächsten Prüfung.

## 7. Sprache und Ton

- UI zweisprachig **Deutsch/Englisch**, umschaltbar über `language = "auto" | "de" | "en"`. `auto` folgt der bevorzugten Systemsprache von macOS, Fallback Englisch.
- Satzanfang groß, sonst Kleinschreibung, keine Großbuchstaben-Labels.
- Beschriftungen in der Label-Spalte (z. B. „Token“, „Cache-Hit“ / „Cache hit“, „Subagenten“ / „Subagents“) und Überschriften beginnen groß. Wörter innerhalb einer Zeile folgen der Grammatik der Sprache (englisch klein, deutsche Substantive groß), z. B. „Thinking 37 % vom Output“ / „thinking 37 % of output“.
- Fußzeilen-Hinweise sind Taste plus Wort in Fließtext-Schreibung: englisch klein, deutsch Verben klein und Substantive groß („r prüfen“, „? Hilfe“). Der Hinweis zu `r` nennt, was die Taste im Tab tut: aktualisieren (Übersicht, Workspaces, KI), prüfen (Updates), neu laden (Protokoll), Config neu laden (Einstellungen).
- Aktionen heißen nach dem, was passiert: „Fast-Forward“, „Aktualisieren“. Die Bestätigung verwendet dasselbe Wort: „Fast-Forward: 3 Commits“.
- Fehler nennen Ursache und nächsten Schritt, ohne Entschuldigung: „npm nicht im PATH der Login-Shell gefunden. Installieren oder in der Config `[tools] npm = "…"` setzen.“
- Geschätzte Werte tragen immer `≈`; in Zusammenfassungen zusätzlich das Wort „Schätzung“ oder „API-Äquivalent“, in Tabellenzellen nur `≈`.

## 8. Wortmarke und README

- Wortmarke: `hud` in Kleinbuchstaben, Chakra Petch 600, mit einer magentafarbenen Kurslinie als Unterstrich. Dateien: `docs/assets/wordmark.svg` (hell) und `docs/assets/wordmark-dark.svg` (dunkel).
- Logo: Flight-Path-Vector-Symbol (Kreis mit Flügeln) in `focus` auf `panel` dunkel, mit Horizontlinie in `soft`. Datei: `docs/assets/logo.svg`.
- README (Englisch): zentrierter Kopf mit Logo, Wortmarke, Claim, Badges und Navigation → Showcase der Übersicht hell/dunkel (`docs/assets/showcase-{light,dark}.png`) → Why hud → Features → Statussymbole → Safety → Install → First run → Keys → Configuration → Data and privacy → Architecture → Documentation → Contributing → License.
- Social Preview des Repos: `docs/assets/social-preview.png`, 1280 × 640, auf `panel` dunkel. Links stehen Logo und Wortmarke, der Claim „A heads-up display for your developer machine“ (Chakra Petch 600, `ink`), die Kurzbeschreibung (Chakra Petch 400, `muted`) und `$ brew install nimble-123/tap/hud` (IBM Plex Mono, `$` in `focus`) in einem Kasten in `chrome`. Rechts läuft ein Ausschnitt der dunklen Übersicht im Fensterrahmen über die Bildkante hinaus. Text bleibt innerhalb des sicheren Rands von 40 px. Hochgeladen wird das Bild unter Settings → General → Social preview.
- Logo, Wortmarke und Social Preview enthalten Text als Pfade (Chakra Petch, IBM Plex Mono), damit GitHub sie ohne Webfonts korrekt darstellt. Erzeugt werden sie mit `uv run scripts/gen-readme-assets.py`, die Social Preview aus dem vorher gerenderten Showcase.
- Der Showcase ist ein Rendering der echten `view` mit Beispieldaten (115 × 38 Zellen, Hack Nerd Font Mono, PNG). Erzeugt wird er mit `scripts/gen-showcase.sh`.

## 9. Diagramme

- Alle Diagramme nutzen das diagram-design-Profil **`hud`** (`~/.diagram-design/profiles/hud.md`, Projekt-Marker `.diagram-design`).
- Quelle: `docs/diagrams/<name>.html`, eingebettet als exportiertes `docs/diagrams/<name>.svg`.
- Statusfarben in Diagrammen nur für Ergebnis-Markierungen, immer mit Symbol und Text.

## 10. Barrierefreiheit

- Volle Tastaturbedienung, Maus optional.
- Kein Status nur über Farbe (siehe 3.2).
- `NO_COLOR` und 16-Farben-Terminals voll unterstützt.
- Kontraste ≥ 4.5:1 in hell und dunkel.
