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
