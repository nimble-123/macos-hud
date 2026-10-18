tab-overview = Übersicht
tab-workspaces = Workspaces
tab-updates = Updates
tab-ai = KI
tab-log = Protokoll
tab-settings = Einstellungen
key-tab = tab
key-quit = q
hint-switch = wechseln
hint-quit = beenden
key-arrows = ↑↓
key-f = f
key-r = r
hint-select = auswählen
hint-fast-forward = Fast-Forward
hint-refresh = aktualisieren
hint-check = prüfen
hint-reload-log = neu laden
syncing = Fetch und Sync laufen…
checking-updates = Update-Prüfung läuft…
ws-none = Noch keine Workspaces eingerichtet.
ws-unavailable = nicht verfügbar
ws-auto-off = automatischer Fast-Forward aus
ws-empty = Keine Repositories gefunden.
branch-detached = (detached)
state-up-to-date = aktuell
state-behind = { $count ->
    [one] 1 Commit zurück, Fast-Forward möglich
   *[other] { $count } Commits zurück, Fast-Forward möglich
}
state-fast-forwarded = { $count ->
    [one] Fast-Forward: 1 Commit
   *[other] Fast-Forward: { $count } Commits
}
state-error = Fehler: { $message }
ff-running = Fast-Forward { $repo } …
ff-up-to-date = { $repo }: bereits aktuell, nichts zu tun
ff-result = { $repo }: { $result }
ff-skipped = { $repo }: nicht aktualisiert, { $reason }
skip-operation = { $operation } läuft
skip-detached = detached HEAD
skip-no-commits = noch keine Commits
skip-no-upstream = kein Upstream
skip-tracked = lokale Änderungen
skip-diverged = divergiert
skip-ahead = { $count ->
    [one] 1 lokaler Commit nicht gepusht
   *[other] { $count } lokale Commits nicht gepusht
}
skip-opted-out = zurück, automatischer Fast-Forward aus (f)
op-merge = Merge
op-rebase = Rebase
op-cherry-pick = Cherry-Pick
op-revert = Revert
op-bisect = Bisect
setup-title = Willkommen bei hud
setup-step-workspaces = Schritt 1 von 3: Workspaces
setup-workspaces-help = Ordner, in denen hud nach Git-Repositories sucht. Leertaste wählt einen Vorschlag; eigene Pfade unten eintippen.
setup-add-path = Pfad hinzufügen:
setup-path-invalid = Ein Pfad muss mit / oder ~/ beginnen.
setup-need-workspace = Mindestens einen Workspace auswählen.
setup-step-ai = Schritt 2 von 3: KI-Quellen
setup-ai-help = Wessen Nutzung soll der KI-Tab zeigen? hud liest nur die Logs der gewählten Werkzeuge; später im Einstellungen-Tab änderbar.
setup-ai-found = Logs gefunden
setup-ai-missing = noch keine Logs
setup-step-icons = Schritt 3 von 3: Symbole
setup-icons-help = In welcher Zeile werden alle Symbole angezeigt? Nerd-Symbole brauchen eine Nerd Font im Terminal.
setup-icons-unicode = Unicode (empfohlen)
setup-icons-nerd = Nerd Font
setup-icons-ascii = ASCII
key-space = Leertaste
key-enter = Enter
key-esc = Esc
hint-toggle = auswählen
hint-next = weiter
hint-finish = fertig
hint-cancel = abbrechen
hint-move = bewegen
scan-local = Lokaler Scan
scan-sync = Fetch und Sync
ws-scanning = Suche Repositories …
src-homebrew = Homebrew
src-npm = npm (global)
src-skills = Skills
src-missing = nicht gefunden
src-missing-hint = { $program } nicht im PATH der Login-Shell gefunden. Installieren oder in der Config `[tools] { $key } = "…"` setzen.
src-broken = Fehler: { $message }
src-checking = wird geprüft …
src-up-to-date = alles aktuell
src-skills-installed = { $count ->
    [one] 1 Skill installiert, Updates unbekannt
   *[other] { $count } Skills installiert, Updates unbekannt
}
src-update-all = Alle Skills aktualisieren
bump-major = Major-Update
pinned = gepinnt
job-queued = wartet
job-running = läuft
job-succeeded = installiert
job-needs-interaction = braucht ein Passwort, im Terminal ausführen
job-denied = nicht erlaubt
job-failed = fehlgeschlagen: { $message }
job-timed-out = Zeitüberschreitung
key-u = u
hint-install = installieren
hint-install-all = alle
confirm-title = Update installieren?
confirm-major = Major-Update: kann inkompatible Änderungen enthalten.
confirm-install = installieren
confirm-cancel = abbrechen
confirm-all-title = Alle Updates installieren?
confirm-all-packages = { $count ->
    [one] 1 Paket
   *[other] { $count } Pakete
}
confirm-all-skills = alle Skills
confirm-all-major = Major-Updates: { $names }
confirm-install-all = alle installieren
jobs-running = { $count ->
    [one] 1 Installation läuft
   *[other] { $count } Installationen laufen
}
settings-loading = Einstellungen werden geladen …
settings-display = Darstellung
settings-language = Sprache
settings-theme = Theme
settings-icons = Symbole
settings-workspaces = Workspaces
settings-auto-ff = Automatischer Fast-Forward
settings-add-workspace = Workspace hinzufügen
settings-ai-sources = KI-Quellen
settings-auto-install = Automatische Installation
settings-overview = Übersicht
insight-unpushed = Ungepushte Commits
insight-update-attention = Major-Updates und Passwort-Bedarf
insight-problems = Probleme der letzten 24 h
insight-plan-limits = Abo-Limits ab 80 %
insight-disk = Wenig Speicherplatz
settings-rejected = Nicht gespeichert: { $reason }
settings-remove-title = Workspace entfernen?
settings-remove-body = Nur aus der Config. Ordner und Repositories bleiben unverändert.
pref-auto = Automatisch
lang-de = Deutsch
lang-en = Englisch
theme-dark = Dunkel
theme-light = Hell
opt-brew-formulae = Homebrew-Formeln
opt-brew-casks = Homebrew-Casks
opt-npm = npm global (nie über Major-Versionen)
toggle-on = an
toggle-off = aus
confirm-remove = entfernen
key-a = a
key-d = d
hint-change = ändern
hint-add = hinzufügen
hint-remove = entfernen
hint-reload = Config neu laden
log-filter = Filter:
log-filter-all = alle
log-filter-auto = automatisch
log-filter-manual = manuell
log-filter-problems = Probleme
log-search = Suche:
log-empty = Noch keine Aktionen protokolliert.
log-no-match = Keine Aktion passt zu Filter und Suche.
log-col-time = Zeit
log-col-action = Aktion
log-col-trigger = Auslöser
log-col-result = Ergebnis
log-col-target = Ziel
log-detail-target = Ziel
log-detail-output = Ausgabe-Ende
log-duration = Dauer { $duration }
kind-fetch = Fetch
kind-fast-forward = Fast-Forward
kind-install = Installation
trigger-auto = auto
trigger-manual = manuell
outcome-running = läuft
outcome-succeeded = erledigt
outcome-failed = fehlgeschlagen
outcome-timed-out = Zeitüberschreitung
outcome-skipped = übersprungen
outcome-cancelled = abgebrochen
key-slash = /
hint-details = Details
hint-search = suchen
hint-filter = filtern
log-detail-summary = Zusammenfassung
job-cancelled = abgebrochen
quit-title = hud beenden?
quit-running = { $count ->
    [one] 1 Installation läuft noch.
   *[other] { $count } Installationen laufen noch.
}
quit-cancel-warning = Abbrechen kann ein Paket halb installiert zurücklassen.
quit-wait = warten, dann beenden
quit-cancel = abbrechen und beenden
quit-back = zurück
quit-waiting = { $count ->
    [one] Warte auf 1 Installation, danach beendet sich hud
   *[other] Warte auf { $count } Installationen, danach beendet sich hud
}
hint-quit-now = sofort beenden
ai-loading = KI-Nutzung wird gelesen …
ai-no-sources = Keine KI-Quelle aktiviert.
ai-claude = Claude Code
ai-codex = Codex
ai-prices-as-of = Preise Stand { $date }
ai-no-logs = Keine Logs unter { $path }
ai-read-failed = Logs nicht lesbar: { $message }
ai-unreadable = { $count ->
    [one] unvollständig: 1 Zeile nicht lesbar
   *[other] unvollständig: { $count } Zeilen nicht lesbar
}
ai-sessions = Sessions
ai-responses = Antworten
ai-tokens = Token
ai-input = Input
ai-output = Output
ai-cache-write = Cache schr.
ai-cache-read = Cache lesen
ai-cost = Kosten
ai-model = Modell
ai-no-usage = Keine Nutzung in diesem Zeitraum.
ai-partial = unvollständig
ai-api-equivalent = API-Äquivalent
ai-estimate = Schätzung
period-today = Heute
period-7-days = 7 Tage
period-30-days = 30 Tage
period-this-month = Dieser Monat
period-last-month = Letzter Monat
key-left-right = ←→
hint-scroll = blättern
hint-period = Zeitraum
telemetry = Telemetrie
telemetry-off = aus (Config)
telemetry-port-busy = Port { $port } nicht verfügbar: { $reason }
telemetry-enabled = empfängt auf 127.0.0.1:{ $port }, Claude Code sendet
telemetry-disabled = Claude Code sendet noch nicht
telemetry-enable = einschalten
telemetry-conflict = eigene OTel-Einstellungen in Claude Code ({ $keys }), hud ändert nichts
telemetry-unreadable = settings.json nicht lesbar: { $reason }
telemetry-restart = Laufende Claude-Code-Sitzungen neu starten, damit sie senden.
telemetry-dialog-title = Telemetrie in Claude Code einschalten?
telemetry-dialog-body = hud ergänzt in ~/.claude/settings.json unter "env":
telemetry-dialog-note = Nur Metriken, keine Prompts oder Antworten. Andere Einträge bleiben unverändert.
telemetry-dialog-write = eintragen
key-t = t
ai-active-time = Aktive Zeit
ai-active-user = Eingabe
ai-active-cli = Claude
ai-telemetry-note = nur solange hud läuft
ai-code = Code
ai-lines = +{ $added } / −{ $removed } Zeilen
ai-commits = { $count ->
    [one] 1 Commit
   *[other] { $count } Commits
}
ai-pull-requests = { $count ->
    [one] 1 PR
   *[other] { $count } PRs
}
ai-cost-claude = Kosten laut Claude
overview-measuring = Systemwerte werden gemessen …
overview-cpu = CPU
overview-memory = Speicher
overview-processes = Prozesse
overview-repos-behind = { $count ->
    [one] 1 Repo zurück
   *[other] { $count } Repos zurück
}
overview-repos-unpushed = { $count } ungepusht
overview-repos-skipped = { $count } übersprungen
overview-repos-diverged = { $count } divergiert
overview-repos-errors = { $count ->
    [one] 1 Fehler
   *[other] { $count } Fehler
}
overview-workspaces-unavailable = { $count ->
    [one] 1 Workspace nicht verfügbar
   *[other] { $count } Workspaces nicht verfügbar
}
overview-updates = { $count ->
    [one] 1 Update
   *[other] { $count } Updates
}
overview-updates-major = { $count } Major
overview-updates-password = { $count ->
    [one] 1 braucht ein Passwort
   *[other] { $count } brauchen ein Passwort
}
overview-problems = { $count ->
    [one] 1 Problem in 24 h (Protokoll)
   *[other] { $count } Probleme in 24 h (Protokoll)
}
overview-plan-limit = { $source }-Limit { $window }: { $percent }, neu ab { $time }
overview-disk-low = { $name }: nur { $free } frei
overview-calm = Alles aktuell.
overview-calm-since = Alles aktuell. Letzte Prüfung vor { $minutes } min.
overview-checking = Erste Prüfung läuft …
overview-ai-today = KI heute
overview-calm-now = Alles aktuell. Gerade eben geprüft.
ai-files-read = { $files } Logdateien in { $ms } ms
kpi-today = heute
kpi-7-days = 7 T
kpi-30-days = 30 T
kpi-output-today = Output heute
kpi-cache-hit = Cache-Hit
kpi-models = Modelle
kpi-effort = Aufwand
kpi-subagents = Subagenten
kpi-of-fresh = der neuen Token
kpi-thinking = Thinking
kpi-of-output = vom Output
kpi-activity = Aktivität
kpi-sessions = { $count ->
    [one] 1 Session
   *[other] { $count } Sessions
}
kpi-sessions-30 = { $count } Sessions in 30 T
kpi-responses = { $count ->
    [one] 1 Antwort
   *[other] { $count } Antworten
}
kpi-tool-calls = { $count ->
    [one] 1 Tool-Call
   *[other] { $count } Tool-Calls
}
kpi-projects = Projekte
kpi-just-now = gerade eben
kpi-minutes-ago = vor { $count } min
kpi-hours-ago = vor { $count } h
kpi-days-ago = vor { $count } T
ai-token-kinds = Token-Arten
kpi-consumption = Verbrauch
kpi-today-label = Heute
kpi-details = Details
kpi-totals = Gesamt
kpi-axis-midnight = 0 Uhr
kpi-axis-day-end = 24 Uhr
kpi-axis-month-start = Monatsanfang
kpi-axis-month-end = Monatsende
calendar-title = KI-Aktivität
calendar-weeks = letzte { $count } Wochen
calendar-mon = Mo
calendar-wed = Mi
calendar-fri = Fr
calendar-summary = { $answers } Antworten, { $days } aktive Tage, längste Serie { $streak } Tage
calendar-less = weniger
calendar-more = mehr
calendar-git-title = Git-Aktivität
calendar-git-summary = { $answers } Commits, { $days } aktive Tage, längste Serie { $streak } Tage
help-title = Tasten
help-everywhere = Überall
help-close = Beliebige Taste schließt die Hilfe.
help-key-fast-forward = f, Enter
help-key-install = u, Enter
help-key-scroll = ↑↓, Bild↑↓, Pos1, Ende
help-key-period = ←→, 1–5
help-key-change = Enter, Leertaste, ←→
help-key-tabs = Tab, Shift-Tab
help-key-help = ?
help-key-ctrl-c = Ctrl-C
help-select = Auswahl bewegen, am Ende geht es oben weiter
help-details = Details vergrößern oder verkleinern
help-fast-forward = Fast-Forward für das ausgewählte Repo
help-sync = Fetch und Sync jetzt starten
help-install = ausgewähltes Paket installieren (mit Rückfrage)
help-install-all = alle verfügbaren Updates installieren (mit Rückfrage, ohne gepinnte)
help-check = Update-Quellen jetzt prüfen
help-scroll = durch den Tab blättern, wenn er nicht ganz passt
help-period = Zeitraum für den ganzen Tab wechseln
help-telemetry = Telemetrie in Claude Code einschalten (mit Rückfrage)
help-search = suchen, filtert beim Tippen
help-filter = Filter wechseln: alle, automatisch, manuell, Probleme
help-clear-search = Suche verwerfen
help-reload-log = Protokoll neu laden
help-change = Wert ändern oder Schalter kippen
help-add-workspace = Workspace hinzufügen
help-remove-workspace = Workspace entfernen (mit Rückfrage)
help-reload-config = Config-Datei neu laden
help-tabs = nächster bzw. vorheriger Tab
help-help = diese Hilfe
help-quit = beenden, fragt bei laufender Installation nach
help-quit-now = beenden, auch aus Eingaben heraus
hint-help = Hilfe
ai-reasoning = Reasoning
kpi-reasoning = Reasoning
kpi-plan = Abo-Auslastung
kpi-plan-note = Plan { $plan }, Stand { $ago }
kpi-plan-as-of = Stand { $ago }
plan-window-minutes = { $count } min
plan-window-hours = { $count ->
    [one] 1 Stunde
   *[other] { $count } Stunden
}
plan-window-days = { $count ->
    [one] 1 Tag
   *[other] { $count } Tage
}
plan-window-primary = Hauptfenster
plan-window-secondary = Zweites Fenster
plan-resets = neu ab { $time }
plan-reset-done = seit { $time } neu, Stand folgt mit der nächsten Nutzung
