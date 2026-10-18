# PRD — hud: System & Workspace HUD

| | |
|---|---|
| **Version** | 0.7.0 |
| **Status** | zur Prüfung |
| **Stand** | 2026-10-18 |
| **Zielplattform** | macOS (Apple Silicon und Intel), Terminal |
| **Repository** | `nimble-123/macos-hud` (öffentlich) |
| **Verwandte Dokumente** | [ARCHITECTURE.md](ARCHITECTURE.md) · [DESIGN.md](DESIGN.md) · Planung: GitHub Issues |

## 1. Ziel

`hud` ist eine lokale Terminal-App, die Systemstatus, Workspaces, verfügbare Software-Updates und KI-Nutzung an einem Ort zusammenfasst. Sie zeigt vor allem, **was Aufmerksamkeit braucht**, und erledigt Routinearbeit sicher:

- Saubere Git-Repositories werden automatisch per Fast-Forward aktualisiert (pro Workspace abschaltbar).
- Paket-Updates werden angezeigt, gezielt ausgelöst oder nach ausdrücklichem Opt-in automatisch installiert.

![Systemkontext von hud](diagrams/context.svg)

## 2. Zielgruppe

Entwickler:innen, die mehrere lokale Repositories und CLI-Werkzeuge nutzen und deren Zustand schnell überblicken möchten.

## 3. Begriffe

| Begriff | Bedeutung |
|---|---|
| **Workspace** | Ein konfigurierter Ordner, unter dem hud Git-Repositories sucht |
| **Fast-Forward (FF)** | Lokalen Branch auf den Stand seines Upstreams vorziehen, ohne Merge-Commit |
| **Update-Quelle** | Homebrew (Formeln, Casks), globale npm-Pakete, Skills (`npx skills`) |
| **Opt-in / Opt-out** | Ausdrückliches Ein- bzw. Ausschalten einer Automatik |
| **Genauigkeit** | Kennzeichnung eines Werts als exakt, geschätzt, unvollständig oder nicht verfügbar |

## 4. Kernfunktionen

### 4.1 Übersicht (Dashboard)

- CPU-Last und Speicher mit Mini-Verlauf (Sparkline, letzte 2 min, Messung alle 2 s), dazu die Prozessanzahl; CPU oder Speicher ab 90 % in `caution`
- Anzahl Repositories nach Status (zurück – auch bei Opt-out –, übersprungen, divergiert, Fehler), Anzahl nicht verfügbarer Workspaces und verfügbare Updates pro Quelle (ohne gepinnte Pakete)
- KI-Nutzung heute über alle aktiven Quellen (Token, geschätzte Kosten)
- KI-Aktivität (Antworten pro Tag) und Git-Aktivität (eigene Commits laut `git config user.email` über alle Branches aller Repositories) als Kalender im Stil des GitHub-Contribution-Graphs, bis zu 53 Wochen; ohne Aktivität entfällt der Kalender
- Zeigt nur Abweichungen; ist alles normal, erscheint ein einzelner neutraler Satz mit dem Zeitpunkt der letzten Prüfung („Alles aktuell. Letzte Prüfung vor N min.“)
- Laufende Installationen, Scans, Syncs und manuell gestartete Update-Prüfungen erscheinen auf allen Tabs in einer Fortschrittszeile der Fußzeile

### 4.2 Workspaces und Git

- Workspace-Ordner werden beim **ersten Start** in einem Einrichtungsdialog festgelegt; weitere lassen sich jederzeit in den Einstellungen hinzufügen oder entfernen
- hud findet Repositories bis zur eingestellten Suchtiefe unter dem Workspace, auch eigenständige Klone innerhalb eines anderen Repositorys; Submodule gehören zu ihrem Repository und erscheinen nicht einzeln
- Nicht erreichbare Workspace-Ordner (z. B. ausgehängtes externes Volume) erscheinen als „nicht verfügbar“, ohne Fehler für andere Workspaces
- Pro Repository: Statussymbol, Name, Branch, Ahead-/Behind-Zahlen (↑/↓) und Status bzw. Grund des Überspringens
- Upstream-Stand wird per `git fetch` ermittelt: direkt nach dem Start, danach periodisch (Default alle 15 min) und per Taste `r`, ohne interaktive Passwortabfragen
- **Automatischer Fast-Forward ist standardmäßig aktiv** und pro Workspace per Opt-out abschaltbar
- Fast-Forward nur, wenn alle Regeln in Abbildung 2 erfüllt sind; sonst wird das Repo übersprungen und der Grund angezeigt
- Untracked Dateien blockieren den Fast-Forward nicht; Git bricht ab, falls der Upstream eine gleichnamige Datei bringt
- Manueller Fast-Forward per Taste `f` oder `Enter` (oder zweiter Klick auf die gewählte Zeile bzw. Klick auf den Fußzeilen-Hinweis), auch bei Workspaces mit Opt-out; es gelten dieselben Regeln wie beim automatischen; die Fußzeile bestätigt die Eingabe sofort und nennt danach das Ergebnis, auch „bereits aktuell“ oder den Grund fürs Überspringen

![Regeln für den automatischen Fast-Forward](diagrams/sync-decision.svg)

*Abbildung 2: Entscheidungsregeln für den Fast-Forward.*

### 4.3 Updates

- Erkennung verfügbarer Updates für Homebrew-Formeln und -Casks, globale npm-Pakete und Skills beim Start, periodisch (Default alle 6 h) und per Taste `r` im Updates-Tab
- Installierte Skills (`npx skills`) werden aufgelistet und mit einer Aktion „Alle aktualisieren“ aktualisiert; die Anzahl verfügbarer Skill-Updates ist als „nicht verfügbar“ gekennzeichnet, weil die Skills-CLI keinen Prüfmodus ohne Installation hat
- Pro Quelle: Verfügbarkeit der CLI und letzte Fehlermeldung
- Updates gezielt aus hud starten; vor manuellen systemweiten Paket-Updates eine Bestätigung einholen
- **Automatische Installation nur per Opt-in**, standardmäßig aus, getrennt schaltbar für: Homebrew-Formeln, Homebrew-Casks, globale npm-Pakete
- Skills haben keine automatische Installation
- npm-Updates mit Major-Sprung oder nicht bestimmbarem Versionssprung werden nie automatisch installiert; manuell bleiben sie möglich
- Gepinnte Pakete werden angezeigt, aber nie installiert
- Ein Cask, dessen Update eine Passwortabfrage braucht, wird als „manuell nötig“ markiert und nicht erneut automatisch versucht
- Automatische Aktionen laufen nur, solange hud läuft
- Jeder Fast-Forward, jede Installation und jeder fehlgeschlagene Fetch wird mit Ergebnis protokolliert; erfolgreiche Fetches und von den Regeln abgelehnte automatische Installationen nicht

![Regeln für die automatische Installation](diagrams/auto-install-decision.svg)

*Abbildung 3: Entscheidungsregeln für die automatische Installation.*

### 4.4 KI-Dashboard

- **Claude Code** standardmäßig aktiv; **Codex** optional (im Einrichtungsdialog vorausgewählt, wenn es Logs gibt). Beide Quellen lassen sich im Einrichtungsdialog, im Einstellungen-Tab und in der Config ein- und ausschalten; das wirkt ohne Neustart, gespeicherte Daten einer abgeschalteten Quelle bleiben erhalten
- Jede Quelle hat einen eigenen Abschnitt; nur „KI heute“ und der Kalender der Übersicht zählen alle Quellen zusammen
- Zeiträume: heute, 7 Tage, 30 Tage, dieser Kalendermonat, letzter Kalendermonat
- Sessions, Modelle und Token pro Zeitraum; Input-, Output-, Cache-Schreib-, Cache-Lese- und (Codex) Reasoning-Token getrennt, sofern die Quelle sie liefert
- Kennzahlen pro Zeitraum: Cache-Hit-Rate, Modellanteile, Subagenten-Anteil (Claude Code), Thinking- bzw. Reasoning-Anteil, Aktivität nach Stunde (heute) oder Tag, Sessions, Antworten und Tool-Calls, Projektanteile; dazu aktuell die Kontextauslastung der letzten Antwort
- Kosten immer als **Schätzung** mit `≈` gekennzeichnet; bei Abo-Nutzung als „API-Äquivalent“
- Preistabelle wird mit hud ausgeliefert, zeigt ihr Stand-Datum und ist in der Config überschreibbar
- Codex: zusätzlich Auslastung des Abo-Limits (Prozent, Rücksetzzeitpunkt), soweit lokal verfügbar
- Jeder Wert trägt Quelle und Genauigkeit; nicht verfügbare oder nicht vergleichbare Werte werden als solche gekennzeichnet
- Es werden nur Nutzungsmetadaten gelesen, **keine** Prompt- oder Antwortinhalte gespeichert
- Zusätzlich empfängt hud, solange es läuft, die OpenTelemetry-Metriken von Claude Code auf `127.0.0.1` (aktive Zeit, Codezeilen, Commits, PRs, Kosten laut Claude); die Logs bleiben die vollständige Historie
- Taste `t` trägt nach Rückfrage die nötigen OTel-Einträge in `~/.claude/settings.json` ein; bei eigenen OTel-Einstellungen ändert hud nichts

### 4.5 Protokoll

- Chronologische Liste (neueste oben) aller Fast-Forwards, Installationen und fehlgeschlagenen Fetches, manuell und automatisch, mit Auslöser, Ziel, Ergebnis und Ausgabe-Ende
- Filter alle/automatisch/manuell/Probleme; Suche über Ziel, Zusammenfassung und Ausgabe; Detailbereich mit Dauer, per `Enter` vergrößerbar für mehr Ausgabe
- Persistiert in SQLite

## 5. Bedienung

- Vollständig per Tastatur bedienbar; `?` zeigt die Tasten des aktuellen Tabs und die globalen Tasten
- Listenauswahl und Zeitraumwahl laufen am Ende um
- Maus optional: Klick auf Tab, Zeitraum, Zeile oder Fußzeilen-Aktion; ein zweiter Klick auf die gewählte Zeile wirkt wie `Enter`; das Mausrad bewegt die Auswahl bzw. blättert; Dialoge reagieren nur auf Tasten, ein Klick schließt die Hilfe
- Inhalte, die nicht ins Fenster passen, sind erreichbar: Listen halten die Auswahl sichtbar, Übersicht und KI-Tab lassen sich blättern
- Ohne Maus-Support des Terminals funktioniert hud vollständig per Tastatur
- Einstellungen-Tab: Sprache, Theme, Symbolsatz, Workspaces verwalten, Fast-Forward-Opt-out, Update-Opt-ins
- Hintergrundaktionen mit Status in einer Fortschrittszeile der Fußzeile; Ergebnis und Dauer stehen im Protokoll
- **Beenden während einer laufenden Installation:** hud fragt nach und wartet standardmäßig (`Enter`); Abbrechen (`a`) ist ausdrücklich wählbar, `Esc` kehrt zurück, ein zweites `q` beendet sofort. Ohne laufende Installation beendet hud sofort; laufende Fetches und Fast-Forwards werden dabei abgebrochen
- Oberfläche auf **Deutsch und Englisch**, umschaltbar; Default folgt der Systemsprache
- Theme hell/dunkel wird automatisch am Terminal erkannt und ist überschreibbar; `NO_COLOR` wird respektiert
- Symbolsatz wählbar: Unicode (Default), Nerd-Font-Icons oder ASCII; der Einrichtungsdialog zeigt alle drei zur Auswahl
- Gestaltung nach [DESIGN.md](DESIGN.md) (Dark-Cockpit-Prinzip, EICAS-Statusfarben)

## 6. Konfiguration

- TOML-Datei unter `~/.config/hud/config.toml` (respektiert `XDG_CONFIG_HOME`)
- Fehlt die Datei oder enthält sie keinen Workspace, startet der Einrichtungsdialog in drei Schritten: Workspaces (mit Vorschlägen aus dem Home-Verzeichnis), KI-Quellen (Claude Code, Codex), dann Symbolsatz; `Esc` bricht ab, ohne etwas zu schreiben
- Eine ungültige Config beendet hud vor der Übernahme des Terminals mit Fehlermeldung
- Änderungen aus dem Einstellungen-Tab werden kommentarerhaltend zurückgeschrieben; die Datei wird vor jedem Schreiben neu gelesen, damit manuelle Änderungen nicht verloren gehen
- Manuelle Änderungen wirken nach Neustart oder per Taste `r` im Einstellungen-Tab; Werkzeugpfade, die Skills-Quelle und Telemetrie-Einstellungen erst nach Neustart
- Kommandozeile: `--help`, `--version`, `--reset-config`, `--reset-data`, `--yes`; ein Reset listet die Dateien und fragt nach, ohne Terminal nur mit `--yes`

## 7. Datenschutz und Leitplanken

- Keine Telemetrie an Dritte; ausgehender Netzverkehr nur durch die aufgerufenen Werkzeuge (`git fetch`, `brew`, `npm`, `npx`)
- Eingehend lauscht hud nur auf `127.0.0.1` (OTLP von Claude Code, abschaltbar); `~/.claude/settings.json` wird gelesen und nur nach Bestätigung (`t`) im Block `env` ergänzt
- Repository-Dateien mit lokalen Änderungen werden nie automatisch überschrieben
- Jeder automatische Fast-Forward und jede automatische Installation ist einzeln nachvollziehbar protokolliert
- Kostenwerte werden nie als exakte Abrechnung ausgegeben
- Zugangsdaten anderer Werkzeuge (z. B. `~/.codex/auth.json`) werden nie gelesen
- Logs enthalten keine Prompt-Inhalte, Tokens oder Zugangsdaten

## 8. Technische Leitplanken

- **Rust** (Edition 2024), **Ratatui + Crossterm** für die Oberfläche, **Tokio** für Nebenläufigkeit
- Bestehende Werkzeuge (Git, Homebrew, npm, `npx skills`) werden als Prozesse aufgerufen, nicht nachgebaut
- **sysinfo** für Systemwerte, **SQLite** für Verlauf und Cache, **TOML + Serde** für die Konfiguration
- Update- und KI-Integrationen als getrennte Adapter hinter Ports
- Alle Daten lokal; keine Serverdienste

## 9. Architektur- und Qualitätsanforderungen

- **Clean Architecture (Ports & Adapters):** Domain, Application, Infrastructure und Präsentation als getrennte Crates; Abhängigkeiten zeigen nur nach innen (Details: [ARCHITECTURE.md](ARCHITECTURE.md))
- Geschäftsregeln (Abbildungen 2 und 3, Kostenschätzung) sind reine, ohne I/O testbare Funktionen
- Fast-Forward und automatische Installation sind nur mit vorheriger Regel-Entscheidung aufrufbar, abgesichert über das Typsystem
- Kein `unsafe`, kein `unwrap` im Produktivcode, typisierte Fehler pro Schicht
- Qualitäts-Gates vor jedem Merge: Formatierung, Clippy ohne Warnungen, alle Tests, Dokumentation ohne Warnungen, Abhängigkeitsprüfung, Secret-Scan (gitleaks), PR-Titel nach Conventional Commits
- Versionierung per Conventional Commits und release-please

## 10. Abnahmekriterien

1. hud startet im macOS-Terminal; beim ersten Start fragt der Einrichtungsdialog nach Workspaces, danach erscheinen deren Repositories.
2. Alle zentralen Funktionen sind per Tastatur bedienbar; Tab-Wechsel, Zeilenauswahl, Scrollen und Fußzeilen-Aktionen funktionieren auch per Maus.
3. Git-Status und Update-Kandidaten werden pro Quelle mit Statussymbol und Text dargestellt.
4. Ein sauberer, ausschließlich hinter seinem Upstream liegender Branch wird automatisch per Fast-Forward aktualisiert, sofern sein Workspace kein Opt-out gesetzt hat.
5. Repositories mit getrackten Änderungen, divergierter Historie, Ahead-Commits, detached HEAD, Branch ohne Commits, fehlendem Upstream oder laufender Git-Operation werden übersprungen und mit Grund markiert.
6. Homebrew- und npm-Updates werden standardmäßig nur angezeigt; nach separatem Opt-in werden sie automatisch installiert, npm ohne Major- oder unbestimmbare Sprünge.
7. Claude-Code-Daten werden standardmäßig angezeigt; Codex lässt sich in der Config aktivieren und zeigt zusätzlich die Abo-Auslastung.
8. KI-Sessions, Token und Kosten erscheinen mit Kennzeichnung von Quelle und Genauigkeit für alle fünf Zeiträume.
9. Beenden während einer laufenden Installation fragt nach und wartet standardmäßig.
10. Die Oberfläche lässt sich zwischen Deutsch und Englisch umschalten.
11. Alle Qualitäts-Gates laufen fehlerfrei.

## 11. Abgrenzung (nicht enthalten)

- GUI-App oder Web-Dashboard
- Automatische Aktionen, solange hud nicht läuft (kein Daemon, kein launchd-Agent)
- Automatische Updates, solange das jeweilige Opt-in deaktiviert ist
- Verwaltung entfernter Repositories oder Team-Accounts
- KI-Werkzeuge außer Claude Code und Codex
- Prozessliste oder Prozesse beenden
- Weitere Sprachen außer Deutsch und Englisch
