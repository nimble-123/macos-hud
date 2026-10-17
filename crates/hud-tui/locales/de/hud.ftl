tab-overview = Übersicht
tab-workspaces = Workspaces
tab-updates = Updates
tab-ai = KI
tab-log = Protokoll
tab-settings = Einstellungen
tab-placeholder = { $tab } ist noch nicht gebaut.
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
syncing = Fetch und Sync laufen…
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
setup-step-workspaces = Schritt 1 von 2: Workspaces
setup-workspaces-help = Ordner, in denen hud nach Git-Repositories sucht. Leertaste wählt einen Vorschlag; eigene Pfade unten eintippen.
setup-add-path = Pfad hinzufügen:
setup-path-invalid = Ein Pfad muss mit / oder ~/ beginnen.
setup-need-workspace = Mindestens einen Workspace auswählen.
setup-step-icons = Schritt 2 von 2: Symbole
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
confirm-title = Update installieren?
confirm-major = Major-Update: kann inkompatible Änderungen enthalten.
confirm-install = installieren
confirm-cancel = abbrechen
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
settings-auto-install = Automatische Installation
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
hint-period = Zeitraum
