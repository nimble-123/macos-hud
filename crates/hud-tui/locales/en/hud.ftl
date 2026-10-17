tab-overview = Overview
tab-workspaces = Workspaces
tab-updates = Updates
tab-ai = AI
tab-log = Log
tab-settings = Settings
tab-placeholder = { $tab } is not built yet.
key-tab = tab
key-quit = q
hint-switch = switch
hint-quit = quit
key-arrows = ↑↓
key-f = f
key-r = r
hint-select = select
hint-fast-forward = fast-forward
hint-refresh = refresh
syncing = Fetching and syncing…
ws-none = No workspaces configured yet.
ws-unavailable = not available
ws-auto-off = automatic fast-forward off
ws-empty = No repositories found.
branch-detached = (detached)
state-up-to-date = up to date
state-behind = { $count ->
    [one] 1 commit behind, fast-forward possible
   *[other] { $count } commits behind, fast-forward possible
}
state-fast-forwarded = { $count ->
    [one] fast-forwarded 1 commit
   *[other] fast-forwarded { $count } commits
}
state-error = error: { $message }
skip-operation = { $operation } in progress
skip-detached = detached HEAD
skip-no-commits = no commits yet
skip-no-upstream = no upstream
skip-tracked = local changes
skip-diverged = diverged
skip-ahead = { $count ->
    [one] 1 local commit not pushed
   *[other] { $count } local commits not pushed
}
skip-opted-out = behind, automatic fast-forward off (f)
op-merge = merge
op-rebase = rebase
op-cherry-pick = cherry-pick
op-revert = revert
op-bisect = bisect
